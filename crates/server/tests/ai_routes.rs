use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
    routing::post,
};
use chrono::{Duration, Local};
use http_body_util::BodyExt;
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tjxy_application::{AuthService, CatalogQueryService, SystemClock};
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_db::{
    AI_PROVIDER_KEY, AiModelInput, AiReasoningEffort, AiSettingsRepository, AiUsageRepository,
};
use tjxy_server::{AppState, ServerIdentity, build_router};
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;

const IDENTITY: &str =
    r#"MediaBrowser Client="Findroid", Device="Pixel", DeviceId="phone-1", Version="0.16.0""#;

fn state() -> AppState {
    AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
}

struct ConfiguredApp {
    router: axum::Router,
    database: sea_orm::DatabaseConnection,
    alice_token: String,
    bob_token: String,
    visible_model: Uuid,
    hidden_model: Uuid,
    upstream: TestServer,
}

struct TestServer {
    base_url: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl TestServer {
    async fn start() -> Self {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(|Json(request): Json<Value>| async move {
                assert_eq!(request["reasoning_effort"], "high");
                Json(json!({
                    "choices": [{
                        "message": {
                            "role": "assistant",
                            "content": "Based on your library, try Arrival.",
                            "tool_calls": []
                        }
                    }],
                    "usage": {
                        "prompt_tokens": 120,
                        "completion_tokens": 30,
                        "total_tokens": 150
                    }
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown, receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = receiver.await;
                })
                .await
                .unwrap();
        });
        Self {
            base_url: format!("http://{address}/v1"),
            shutdown: Some(shutdown),
        }
    }

    async fn rejecting() -> Self {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(|| async { StatusCode::BAD_GATEWAY }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown, receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = receiver.await;
                })
                .await
                .unwrap();
        });
        Self {
            base_url: format!("http://{address}/v1"),
            shutdown: Some(shutdown),
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

async fn configured_app() -> ConfiguredApp {
    configured_app_with(TestServer::start().await).await
}

async fn configured_app_with(upstream: TestServer) -> ConfiguredApp {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    auth.create_user("Alice", "correct horse", true)
        .await
        .unwrap();
    auth.create_user("Bob", "ordinary password", false)
        .await
        .unwrap();
    let cipher = Arc::new(
        CredentialCipher::new(CredentialKey::new(1, [19_u8; 32]).unwrap(), Vec::new()).unwrap(),
    );
    let visible_model = Uuid::new_v4();
    let hidden_model = Uuid::new_v4();
    let sealed = cipher
        .seal_bound(Uuid::new_v4(), AI_PROVIDER_KEY, b"test-secret")
        .unwrap();
    AiSettingsRepository::new(&database)
        .put(
            &sealed,
            true,
            &upstream.base_url,
            "Only discuss movies and television using catalog tools.",
            &[
                AiModelInput::new(
                    visible_model,
                    "model-visible",
                    "Cinema Guide",
                    true,
                    true,
                    0,
                )
                .with_reasoning_effort(AiReasoningEffort::High),
                AiModelInput::new(hidden_model, "model-hidden", "Internal", false, false, 1),
            ],
            None,
        )
        .await
        .unwrap();
    let router = build_router(
        AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
            .with_auth(auth)
            .with_ai(database.clone(), Some(cipher))
            .with_catalog(Arc::new(CatalogQueryService::new(database.clone())))
            .with_client_portal(database.clone())
            .with_ready(true),
    );
    let alice_token = login(router.clone(), "Alice", "correct horse").await;
    let bob_token = login(router.clone(), "Bob", "ordinary password").await;
    ConfiguredApp {
        router,
        database,
        alice_token,
        bob_token,
        visible_model,
        hidden_model,
        upstream,
    }
}

async fn login(router: axum::Router, username: &str, password: &str) -> String {
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/Users/AuthenticateByName")
                .header(header::AUTHORIZATION, IDENTITY)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": username, "Pw": password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await["AccessToken"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn authenticated_request(
    method: Method,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> Request<Body> {
    let builder = Request::builder().method(method).uri(uri).header(
        header::AUTHORIZATION,
        format!(r#"MediaBrowser Token="{token}""#),
    );
    match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn json_body(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn conversation_and_chat_routes_require_authentication() {
    for (method, path) in [
        (Method::GET, "/Ai/Conversations"),
        (Method::POST, "/Ai/Chat"),
        (
            Method::DELETE,
            "/Ai/Conversations/018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11",
        ),
    ] {
        let response = build_router(state())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "path {path}");
    }
}

#[tokio::test]
async fn chat_rejects_hidden_models_and_persists_an_sse_exchange_for_its_owner() {
    let app = configured_app().await;
    let new_conversation_id = Uuid::new_v4();
    let hidden = app
        .router
        .clone()
        .oneshot(authenticated_request(
            Method::POST,
            "/Ai/Chat",
            &app.alice_token,
            Some(json!({"ModelId": app.hidden_model, "Message": "Recommend a film"})),
        ))
        .await
        .unwrap();
    assert_eq!(hidden.status(), StatusCode::BAD_REQUEST);

    let missing_conversation_id = app
        .router
        .clone()
        .oneshot(authenticated_request(
            Method::POST,
            "/Ai/Chat",
            &app.alice_token,
            Some(json!({"ModelId": app.visible_model, "Message": "Recommend a film"})),
        ))
        .await
        .unwrap();
    assert_eq!(missing_conversation_id.status(), StatusCode::BAD_REQUEST);

    let response = app
        .router
        .clone()
        .oneshot(authenticated_request(
            Method::POST,
            "/Ai/Chat",
            &app.alice_token,
            Some(json!({
                "NewConversationId": new_conversation_id,
                "ModelId": app.visible_model,
                "Message": "Recommend a film"
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    let events = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(events.contains("event: conversation"));
    assert!(events.contains(&new_conversation_id.to_string()));
    assert!(events.contains("event: delta"));
    assert!(events.contains("Based on your library,"));
    assert!(events.contains("Arrival."));
    assert!(events.contains("event: done"));

    let day = Local::now().date_naive().to_string();
    let usage = AiUsageRepository::new(&app.database)
        .analytics(&day, &day, &day, 10)
        .await
        .unwrap();
    assert_eq!(usage.summary.total_requests, 1);
    assert_eq!(usage.summary.total_tokens, Some(150));

    let list = app
        .router
        .clone()
        .oneshot(authenticated_request(
            Method::GET,
            "/Ai/Conversations",
            &app.alice_token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    assert_eq!(
        list.headers().get(header::CACHE_CONTROL),
        Some(&header::HeaderValue::from_static("no-store"))
    );
    let conversations = json_body(list).await;
    let conversation_id = conversations["Items"][0]["Id"].as_str().unwrap();
    assert_eq!(conversation_id, new_conversation_id.to_string());

    let own = app
        .router
        .clone()
        .oneshot(authenticated_request(
            Method::GET,
            &format!("/Ai/Conversations/{conversation_id}"),
            &app.alice_token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(own.status(), StatusCode::OK);
    assert_eq!(
        own.headers().get(header::CACHE_CONTROL),
        Some(&header::HeaderValue::from_static("no-store"))
    );
    let own = json_body(own).await;
    assert_eq!(own["Messages"].as_array().unwrap().len(), 2);
    assert_eq!(own["Messages"][0]["Role"], "user");
    assert_eq!(own["Messages"][1]["Role"], "assistant");

    let other_user = app
        .router
        .oneshot(authenticated_request(
            Method::GET,
            &format!("/Ai/Conversations/{conversation_id}"),
            &app.bob_token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(other_user.status(), StatusCode::NOT_FOUND);
    drop(app.upstream);
}

#[tokio::test]
async fn connection_test_requires_a_new_key_when_the_provider_origin_changes() {
    let app = configured_app().await;
    let response = app
        .router
        .clone()
        .oneshot(authenticated_request(
            Method::POST,
            "/Admin/Ai/Settings/Test",
            &app.alice_token,
            Some(json!({
                "BaseUrl": "https://other.example/v1",
                "UpstreamModel": "model-visible"
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upstream_rejections_are_counted_without_storing_provider_details() {
    let app = configured_app_with(TestServer::rejecting().await).await;
    let response = app
        .router
        .clone()
        .oneshot(authenticated_request(
            Method::POST,
            "/Ai/Chat",
            &app.alice_token,
            Some(json!({
                "NewConversationId": Uuid::new_v4(),
                "ModelId": app.visible_model,
                "Message": "Recommend a film"
            })),
        ))
        .await
        .unwrap();
    let events = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(events.contains("AssistantUnavailable"));
    let day = Local::now().date_naive().to_string();
    let usage = AiUsageRepository::new(&app.database)
        .analytics(&day, &day, &day, 10)
        .await
        .unwrap();
    assert_eq!(usage.summary.failed_requests, 1);
    assert_eq!(usage.summary.total_tokens, None);
    assert_eq!(
        usage.recent_failures[0].outcome,
        tjxy_db::AiExecutionOutcome::UpstreamRejected
    );
}
