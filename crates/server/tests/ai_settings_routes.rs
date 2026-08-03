use std::{
    net::SocketAddr,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
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
use tjxy_server::{
    AiProviderSession, AiProviderTransport, AiProviderTransportError, AppState,
    ProviderDnsResolver, ProviderMethod, ProviderResponse, SafeReqwestTransport, ServerIdentity,
    build_router,
};
use tjxy_test_support::test_database;
use tower::ServiceExt;
use uuid::Uuid;

fn state() -> AppState {
    AppState::new(ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"))
}

struct ModelProvider;

struct ModelProviderSession;

#[async_trait]
impl AiProviderTransport for ModelProvider {
    async fn open(
        &self,
        base_url: &reqwest::Url,
    ) -> Result<Arc<dyn AiProviderSession>, AiProviderTransportError> {
        assert_eq!(base_url.as_str(), "https://provider.example.test/v1/");
        Ok(Arc::new(ModelProviderSession))
    }
}

#[async_trait]
impl AiProviderSession for ModelProviderSession {
    async fn request(
        &self,
        method: ProviderMethod,
        endpoint: reqwest::Url,
        api_key: &str,
        body: Option<Value>,
    ) -> Result<ProviderResponse, AiProviderTransportError> {
        assert_eq!(method, ProviderMethod::Get);
        assert_eq!(endpoint.path(), "/v1/models");
        assert_eq!(api_key, "test-secret");
        assert!(body.is_none());
        Ok(ProviderResponse {
            status: StatusCode::OK,
            body: json!({
                "data": [
                    {"id": "zeta-model"},
                    {"id": "alpha-model"},
                    {"id": "alpha-model"},
                    {"id": ""}
                ]
            }),
        })
    }
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
    let base_url = "https://provider.example.test/v1";
    AiSettingsRepository::new(&database)
        .put(
            &sealed,
            true,
            base_url,
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
            .with_ai_transport(database, Some(cipher), Arc::new(ModelProvider)),
    );
    let token = login(router.clone()).await;
    let changed_origin = router
        .clone()
        .oneshot(authenticated_request(
            Method::POST,
            "/Admin/Ai/Settings/Models",
            &token,
            Some(json!({"BaseUrl": "https://other.example.test/v1"})),
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

struct PublicTestResolver;

#[async_trait]
impl ProviderDnsResolver for PublicTestResolver {
    async fn resolve(
        &self,
        _host: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, AiProviderTransportError> {
        Ok(vec![SocketAddr::from(([1, 1, 1, 1], port))])
    }
}

#[tokio::test]
async fn provider_transport_proxy_bypass_child() {
    if std::env::var_os("TJXY_PROXY_BYPASS_CHILD").is_none() {
        return;
    }
    let transport = SafeReqwestTransport::with_resolver(Arc::new(PublicTestResolver));
    let base_url = reqwest::Url::parse("http://provider.example.test:9/v1/").unwrap();
    let session = transport.open(&base_url).await.unwrap();
    let endpoint = base_url.join("models").unwrap();
    let _ = session
        .request(ProviderMethod::Get, endpoint, "test-secret", None)
        .await;
}

#[tokio::test]
async fn provider_transport_ignores_proxy_environment() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let trap_hits = Arc::clone(&hits);
    let trap = tokio::spawn(async move {
        if let Ok(Ok((mut stream, _))) =
            tokio::time::timeout(std::time::Duration::from_secs(7), listener.accept()).await
        {
            trap_hits.fetch_add(1, Ordering::SeqCst);
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).await;
            let _ = stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                )
                .await;
        }
    });
    let child = tokio::task::spawn_blocking(move || {
        Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("provider_transport_proxy_bypass_child")
            .arg("--nocapture")
            .env("TJXY_PROXY_BYPASS_CHILD", "1")
            .env("HTTP_PROXY", &proxy_url)
            .env("HTTPS_PROXY", &proxy_url)
            .env("ALL_PROXY", &proxy_url)
            .env_remove("NO_PROXY")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    trap.abort();
    assert!(
        child.status.success(),
        "child test failed: {}",
        String::from_utf8_lossy(&child.stderr)
    );
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}
