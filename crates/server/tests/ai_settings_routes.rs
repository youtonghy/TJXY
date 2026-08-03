use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
    routing::get,
};
use chrono::{Duration, Local, Utc};
use http_body_util::BodyExt;
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tjxy_application::{AuthService, SystemClock};
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_db::{
    AI_PROVIDER_KEY, AiExecutionInput, AiExecutionOutcome, AiModelInput, AiSettingsRepository,
    AiUsageRepository,
};
use tjxy_server::{AppState, ServerIdentity, build_router};
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;

fn state() -> AppState {
    AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
}

#[tokio::test]
async fn ai_settings_require_admin_and_public_models_require_a_user() {
    for (path, expected) in [
        ("/Admin/Ai/Settings", StatusCode::UNAUTHORIZED),
        ("/Ai/Models", StatusCode::UNAUTHORIZED),
    ] {
        let response = build_router(state())
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "path {path}");
    }
}

#[tokio::test]
async fn models_fail_closed_without_encryption_and_delete_uses_a_revision_fence() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    auth.create_user("Admin", "correct horse", true)
        .await
        .unwrap();
    let cipher = Arc::new(
        CredentialCipher::new(CredentialKey::new(1, [31_u8; 32]).unwrap(), Vec::new()).unwrap(),
    );
    let sealed = cipher
        .seal_bound(Uuid::new_v4(), AI_PROVIDER_KEY, b"test-secret")
        .unwrap();
    AiSettingsRepository::new(&database)
        .put(
            &sealed,
            true,
            "https://ai.example.test/v1",
            "Only discuss media.",
            &[AiModelInput::new(
                Uuid::new_v4(),
                "media-model",
                "Cinema Guide",
                true,
                true,
                0,
            )],
            None,
        )
        .await
        .unwrap();

    let router_without_key = build_router(
        AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
            .with_auth(Arc::clone(&auth))
            .with_ai(database.clone(), None),
    );
    let token = login(router_without_key.clone()).await;
    let settings = router_without_key
        .clone()
        .oneshot(authenticated_request(
            Method::GET,
            "/Admin/Ai/Settings",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(settings.status(), StatusCode::OK);
    assert_eq!(
        json_body(settings).await["Models"][0]["ReasoningEffort"],
        "off"
    );
    let models = router_without_key
        .clone()
        .oneshot(authenticated_request(
            Method::GET,
            "/Ai/Models",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(models.status(), StatusCode::OK);
    assert_eq!(json_body(models).await, json!({"Items": []}));

    let stale = router_without_key
        .clone()
        .oneshot(authenticated_request(
            Method::DELETE,
            "/Admin/Ai/Settings",
            &token,
            Some(json!({"Revision": 2})),
        ))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let deleted = router_without_key
        .clone()
        .oneshot(authenticated_request(
            Method::DELETE,
            "/Admin/Ai/Settings",
            &token,
            Some(json!({"Revision": 1})),
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn administrators_can_discover_sorted_models_with_the_saved_credential() {
    let provider = Router::new().route(
        "/v1/models",
        get(|headers: axum::http::HeaderMap| async move {
            assert_eq!(
                headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok()),
                Some("Bearer test-secret")
            );
            Json(json!({
                "data": [
                    {"id": "zeta-model"},
                    {"id": "alpha-model"},
                    {"id": "alpha-model"},
                    {"id": ""}
                ]
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let provider_task = tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });

    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    auth.create_user("Admin", "correct horse", true)
        .await
        .unwrap();
    let cipher = Arc::new(
        CredentialCipher::new(CredentialKey::new(1, [41_u8; 32]).unwrap(), Vec::new()).unwrap(),
    );
    let sealed = cipher
        .seal_bound(Uuid::new_v4(), AI_PROVIDER_KEY, b"test-secret")
        .unwrap();
    let base_url = format!("http://{address}/v1");
    AiSettingsRepository::new(&database)
        .put(
            &sealed,
            true,
            &base_url,
            "Only discuss media.",
            &[AiModelInput::new(
                Uuid::new_v4(),
                "existing-model",
                "Existing",
                true,
                true,
                0,
            )],
            None,
        )
        .await
        .unwrap();
    let router = build_router(
        AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
            .with_auth(auth)
            .with_ai(database, Some(cipher)),
    );
    let token = login(router.clone()).await;
    let changed_origin = router
        .clone()
        .oneshot(authenticated_request(
            Method::POST,
            "/Admin/Ai/Settings/Models",
            &token,
            Some(json!({"BaseUrl": "http://127.0.0.1:9/v1"})),
        ))
        .await
        .unwrap();
    assert_eq!(changed_origin.status(), StatusCode::BAD_REQUEST);
    let response = router
        .oneshot(authenticated_request(
            Method::POST,
            "/Admin/Ai/Settings/Models",
            &token,
            Some(json!({"BaseUrl": base_url})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    assert_eq!(
        json_body(response).await,
        json!({"Items": [{"Id": "alpha-model"}, {"Id": "zeta-model"}]})
    );
    provider_task.abort();
}

#[tokio::test]
async fn ai_analytics_are_admin_only_bounded_and_secret_free() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    let admin = auth
        .create_user("Admin", "correct horse", true)
        .await
        .unwrap();
    let model_id = Uuid::new_v4();
    let now = Utc::now();
    AiUsageRepository::new(&database)
        .record(
            &AiExecutionInput::new(
                admin.id(),
                model_id,
                "Cinema Guide",
                "private-upstream-id",
                Local::now().date_naive().to_string(),
                now,
                now,
                800,
                AiExecutionOutcome::Success,
            )
            .with_usage(70, 30),
        )
        .await
        .unwrap();
    let router = build_router(
        AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
            .with_auth(auth)
            .with_ai(database, None),
    );

    let unauthorized = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/Admin/Ai/Analytics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let token = login(router.clone()).await;
    let response = router
        .oneshot(authenticated_request(
            Method::GET,
            "/Admin/Ai/Analytics",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let body = json_body(response).await;
    assert_eq!(body["Summary"]["TotalRequests"], 1);
    assert_eq!(body["Summary"]["TotalTokens"], 100);
    assert_eq!(body["Users"][0]["Username"], "Admin");
    assert_eq!(body["Models"][0]["DisplayName"], "Cinema Guide");
    assert_eq!(body["Daily"].as_array().unwrap().len(), 14);
    let serialized = body.to_string();
    assert!(!serialized.contains("prompt"));
    assert!(!serialized.contains("response"));
    assert!(!serialized.contains("api-key"));
}

async fn login(router: axum::Router) -> String {
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/Users/AuthenticateByName")
                .header(
                    header::AUTHORIZATION,
                    r#"MediaBrowser Client="Browser", Device="QA", DeviceId="qa-1", Version="1""#,
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": "Admin", "Pw": "correct horse"}).to_string(),
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
