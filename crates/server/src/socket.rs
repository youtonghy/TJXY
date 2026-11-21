use axum::{
    extract::{
        RawQuery, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tjxy_common::UserId;
use tokio::{sync::broadcast, time::Instant};

use crate::{AppState, auth};

const EVENT_BUFFER_CAPACITY: usize = 256;
/// How often an idle socket re-checks the credential it connected with.
const REVALIDATE_EVERY: std::time::Duration = std::time::Duration::from_secs(30);
/// An event is only delivered on a credential checked at most this long ago.
const EVENT_REVALIDATE_AFTER: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Clone)]
pub(crate) struct RealtimeEvents {
    sender: broadcast::Sender<RealtimeEvent>,
}

impl RealtimeEvents {
    #[must_use]
    pub(crate) fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_BUFFER_CAPACITY);
        Self { sender }
    }

    pub(crate) fn publish_user_data_changed(&self, user_id: UserId, user_revision: i64) {
        let _ = self.sender.send(RealtimeEvent::UserDataChanged {
            user_id,
            user_revision,
        });
    }

    pub(crate) fn publish_library_changed(&self, catalog_revision: i64) {
        let _ = self
            .sender
            .send(RealtimeEvent::LibraryChanged { catalog_revision });
    }

    fn subscribe(&self) -> broadcast::Receiver<RealtimeEvent> {
        self.sender.subscribe()
    }
}

#[derive(Clone)]
enum RealtimeEvent {
    LibraryChanged { catalog_revision: i64 },
    UserDataChanged { user_id: UserId, user_revision: i64 },
}

pub(crate) async fn connect(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    websocket: WebSocketUpgrade,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if !socket_query_matches(&principal, raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let receiver = state.realtime_events().subscribe();
    let credential = SocketCredential {
        user_id: principal.user().id(),
        state: state.clone(),
        headers,
        raw_query,
    };
    websocket.on_upgrade(move |socket| async move {
        serve(socket, credential, receiver).await;
    })
}

/// The credential a socket connected with, re-checked while the socket lives so a
/// logout, revoked session or device, password change, disabled user, or deleted API key
/// also ends the event stream.
struct SocketCredential {
    user_id: UserId,
    state: AppState,
    headers: HeaderMap,
    raw_query: Option<String>,
}

impl SocketCredential {
    async fn still_valid(&self) -> bool {
        auth::authenticated_principal(&self.state, &self.headers, self.raw_query.as_deref())
            .await
            .is_ok_and(|principal| principal.user().id() == self.user_id)
    }
}

async fn serve(
    mut socket: WebSocket,
    credential: SocketCredential,
    mut receiver: broadcast::Receiver<RealtimeEvent>,
) {
    let mut validated_at = Instant::now();
    let mut revalidation =
        tokio::time::interval_at(validated_at + REVALIDATE_EVERY, REVALIDATE_EVERY);
    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
            _ = revalidation.tick() => {
                if !credential.still_valid().await {
                    break;
                }
                validated_at = Instant::now();
            }
            event = receiver.recv() => match event {
                Ok(event) => {
                    let Some(payload) = event.payload_for(credential.user_id) else {
                        continue;
                    };
                    if validated_at.elapsed() >= EVENT_REVALIDATE_AFTER {
                        if !credential.still_valid().await {
                            break;
                        }
                        validated_at = Instant::now();
                    }
                    if socket.send(Message::Text(payload.into())).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    let _ = socket.send(Message::Close(None)).await;
}

impl RealtimeEvent {
    fn payload_for(&self, user_id: UserId) -> Option<String> {
        match self {
            Self::LibraryChanged { catalog_revision } => event_json(
                "LibraryChanged",
                LibraryChangedData {
                    catalog_revision: *catalog_revision,
                },
            ),
            Self::UserDataChanged {
                user_id: event_user_id,
                user_revision,
            } if *event_user_id == user_id => event_json(
                "UserDataChanged",
                UserDataChangedData {
                    user_revision: *user_revision,
                },
            ),
            Self::UserDataChanged { .. } => None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SocketEvent<Data> {
    message_type: &'static str,
    data: Data,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LibraryChangedData {
    catalog_revision: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UserDataChangedData {
    user_revision: i64,
}

fn event_json<Data: Serialize>(message_type: &'static str, data: Data) -> Option<String> {
    serde_json::to_string(&SocketEvent { message_type, data }).ok()
}

fn socket_query_matches(
    principal: &tjxy_application::AuthenticatedPrincipal,
    raw_query: Option<&str>,
) -> bool {
    let Ok(mut query) = auth::request_query(raw_query) else {
        return false;
    };
    query.remove("ApiKey");
    query.remove("api_key");
    if let Some(device_id) = query.remove("deviceId")
        && principal.device_id() != Some(device_id.as_str())
    {
        return false;
    }
    query.is_empty()
}
