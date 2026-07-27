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
use tokio::sync::broadcast;

use crate::{AppState, auth};

const EVENT_BUFFER_CAPACITY: usize = 256;

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
    if !auth_only_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let receiver = state.realtime_events().subscribe();
    let user_id = principal.user().id();
    websocket.on_upgrade(move |socket| async move {
        serve(socket, user_id, receiver).await;
    })
}

async fn serve(
    mut socket: WebSocket,
    user_id: UserId,
    mut receiver: broadcast::Receiver<RealtimeEvent>,
) {
    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            event = receiver.recv() => match event {
                Ok(event) => {
                    let Some(payload) = event.payload_for(user_id) else {
                        continue;
                    };
                    if socket.send(Message::Text(payload.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
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

fn auth_only_query(raw_query: Option<&str>) -> bool {
    let Ok(mut query) = auth::request_query(raw_query) else {
        return false;
    };
    query.remove("ApiKey");
    query.remove("api_key");
    query.is_empty()
}
