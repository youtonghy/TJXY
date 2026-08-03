use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use chrono::{Duration, Local};
use http_body_util::BodyExt;
use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tjxy_application::{AuthService, CatalogQueryService, SystemClock};
use tjxy_common::CatalogItemId;
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_db::{
    AI_PROVIDER_KEY, AiModelInput, AiReasoningEffort, AiSettingsRepository, AiUsageRepository,
    UserDataPatch, UserDataRepository,
};
use tjxy_server::{
    AiAdmissionConfig, AiProviderSession, AiProviderTransport, AiProviderTransportError, AppState,
    ProviderMethod, ProviderResponse, ServerIdentity, build_router,
};
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
    alice_user_id: tjxy_common::UserId,
    alice_token: String,
    bob_token: String,
    user_tokens: Vec<String>,
    visible_model: Uuid,
    hidden_model: Uuid,
    upstream: Arc<TestProvider>,
}

struct TestProvider {
    hits: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    entered: Arc<tokio::sync::Notify>,
    releases: Option<Arc<tokio::sync::Semaphore>>,
    failures: Arc<AtomicUsize>,
    always_reject: bool,
    favorites_tool: bool,
    requests: Arc<Mutex<Vec<Value>>>,
}

impl TestProvider {
    async fn start() -> Arc<Self> {
        Self::start_with_behavior(false, false).await
    }

    async fn blocking() -> Arc<Self> {
        Self::start_with_behavior(true, false).await
    }

    async fn rejecting() -> Arc<Self> {
        Self::start_with_behavior(false, true).await
    }

    async fn favorites_tool() -> Arc<Self> {
        let mut provider = Self::start_with_behavior(false, false).await;
        Arc::get_mut(&mut provider).unwrap().favorites_tool = true;
        provider
    }

    async fn start_with_behavior(blocking: bool, always_reject: bool) -> Arc<Self> {
        let hits = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(tokio::sync::Notify::new());
        let releases = blocking.then(|| Arc::new(tokio::sync::Semaphore::new(0)));
        let failures = Arc::new(AtomicUsize::new(0));
        Arc::new(Self {
            hits,
            active,
            max_active,
            entered,
            releases,
            failures,
            always_reject,
            favorites_tool: false,
            requests: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::SeqCst)
    }

    fn release(&self, count: usize) {
        self.releases
            .as_ref()
            .expect("only blocking test servers can release completions")
            .add_permits(count);
    }

    fn reject_next(&self) {
        self.failures.fetch_add(1, Ordering::SeqCst);
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }

    async fn wait_for_hits(&self, expected: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let entered = self.entered.notified();
                if self.hits() >= expected {
                    break;
                }
                entered.await;
            }
        })
        .await
        .expect("upstream did not receive the expected requests");
    }
}

struct TestProviderSession {
    hits: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    entered: Arc<tokio::sync::Notify>,
    releases: Option<Arc<tokio::sync::Semaphore>>,
    failures: Arc<AtomicUsize>,
    always_reject: bool,
    favorites_tool: bool,
    requests: Arc<Mutex<Vec<Value>>>,
}

#[async_trait]
impl AiProviderTransport for TestProvider {
    async fn open(
        &self,
        base_url: &reqwest::Url,
    ) -> Result<Arc<dyn AiProviderSession>, AiProviderTransportError> {
        assert_eq!(base_url.as_str(), "https://provider.example.test/v1/");
        Ok(Arc::new(TestProviderSession {
            hits: Arc::clone(&self.hits),
            active: Arc::clone(&self.active),
            max_active: Arc::clone(&self.max_active),
            entered: Arc::clone(&self.entered),
            releases: self.releases.clone(),
            failures: Arc::clone(&self.failures),
            always_reject: self.always_reject,
            favorites_tool: self.favorites_tool,
            requests: Arc::clone(&self.requests),
        }))
    }
}

#[async_trait]
impl AiProviderSession for TestProviderSession {
    async fn request(
        &self,
        method: ProviderMethod,
        endpoint: reqwest::Url,
        api_key: &str,
        body: Option<Value>,
    ) -> Result<ProviderResponse, AiProviderTransportError> {
        assert_eq!(method, ProviderMethod::Post);
        assert_eq!(endpoint.path(), "/v1/chat/completions");
        assert_eq!(api_key, "test-secret");
        let request = body.expect("chat completions include a request body");
        self.requests.lock().unwrap().push(request.clone());
        let request_index = self.hits.fetch_add(1, Ordering::SeqCst);
        let current = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(current, Ordering::SeqCst);
        let _activity = ProviderActivity(Arc::clone(&self.active));
        self.entered.notify_waiters();
        assert_eq!(request["reasoning_effort"], "high");
        if let Some(releases) = &self.releases {
            releases.acquire().await.unwrap().forget();
        }
        let reject_once = self
            .failures
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok();
        let status = if self.always_reject || reject_once {
            StatusCode::BAD_GATEWAY
        } else {
            StatusCode::OK
        };
        let body = if status.is_success() && self.favorites_tool && request_index.is_multiple_of(2)
        {
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "favorites-1",
                            "type": "function",
                            "function": {
                                "name": "get_favorites",
                                "arguments": "{\"limit\":10}"
                            }
                        }]
                    }
                }]
            })
        } else if status.is_success() {
            json!({
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
            })
        } else {
            json!({})
        };
        Ok(ProviderResponse { status, body })
    }
}

struct ProviderActivity(Arc<AtomicUsize>);

impl Drop for ProviderActivity {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn configured_app() -> ConfiguredApp {
    configured_app_with(TestProvider::start().await).await
}

async fn configured_app_with(upstream: Arc<TestProvider>) -> ConfiguredApp {
    configured_app_with_config(upstream, AiAdmissionConfig::default()).await
}

async fn configured_app_with_config(
    upstream: Arc<TestProvider>,
    admission_config: AiAdmissionConfig,
) -> ConfiguredApp {
    configured_app_with_user_count(upstream, admission_config, 2).await
}

async fn configured_app_with_user_count(
    upstream: Arc<TestProvider>,
    admission_config: AiAdmissionConfig,
    user_count: usize,
) -> ConfiguredApp {
    assert!(user_count >= 2);
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    let alice_user_id = auth
        .create_user("Alice", "correct horse", true)
        .await
        .unwrap()
        .id();
    auth.create_user("Bob", "ordinary password", false)
        .await
        .unwrap();
    let mut users = vec![
        ("Alice".to_owned(), "correct horse".to_owned()),
        ("Bob".to_owned(), "ordinary password".to_owned()),
    ];
    for index in 2..user_count {
        let username = format!("AdmissionUser{index}");
        let password = format!("admission password {index}");
        auth.create_user(&username, &password, false).await.unwrap();
        users.push((username, password));
    }
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
            "https://provider.example.test/v1",
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
            .with_ai_transport_config(
                database.clone(),
                Some(cipher),
                upstream.clone(),
                admission_config,
            )
            .with_catalog(Arc::new(CatalogQueryService::new(database.clone())))
            .with_client_portal(database.clone())
            .with_ready(true),
    );
    let mut user_tokens = Vec::with_capacity(users.len());
    for (username, password) in users {
        user_tokens.push(login(router.clone(), &username, &password).await);
    }
    let alice_token = user_tokens[0].clone();
    let bob_token = user_tokens[1].clone();
    ConfiguredApp {
        router,
        database,
        alice_user_id,
        alice_token,
        bob_token,
        user_tokens,
        visible_model,
        hidden_model,
        upstream,
    }
}

fn chat_request(token: &str, model_id: Uuid) -> Request<Body> {
    authenticated_request(
        Method::POST,
        "/Ai/Chat",
        token,
        Some(json!({
            "NewConversationId": Uuid::new_v4(),
            "ModelId": model_id,
            "Message": "Recommend a film"
        })),
    )
}

fn assert_rate_limited(response: &axum::response::Response) {
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL),
        Some(&header::HeaderValue::from_static("no-store"))
    );
    let retry_after = response.headers()[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert!(retry_after >= 1);
}

fn spawn_body_consuming_chat(
    router: axum::Router,
    token: String,
    model_id: Uuid,
    statuses: tokio::sync::mpsc::UnboundedSender<StatusCode>,
) -> tokio::task::JoinHandle<StatusCode> {
    tokio::spawn(async move {
        let response = router
            .oneshot(chat_request(&token, model_id))
            .await
            .unwrap();
        let status = response.status();
        statuses.send(status).unwrap();
        response.into_body().collect().await.unwrap();
        status
    })
}

async fn receive_statuses(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<StatusCode>,
    count: usize,
) -> Vec<StatusCode> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut statuses = Vec::with_capacity(count);
        while statuses.len() < count {
            statuses.push(receiver.recv().await.unwrap());
        }
        statuses
    })
    .await
    .expect("chat handlers did not return response headers")
}

#[tokio::test]
async fn admission_enforces_per_user_stream_capacity_before_provider_io() {
    let app = configured_app_with_config(
        TestProvider::blocking().await,
        AiAdmissionConfig::new(10, 2, 8, 100).unwrap(),
    )
    .await;
    let (status_sender, mut status_receiver) = tokio::sync::mpsc::unbounded_channel();
    let tasks = (0..4)
        .map(|_| {
            spawn_body_consuming_chat(
                app.router.clone(),
                app.alice_token.clone(),
                app.visible_model,
                status_sender.clone(),
            )
        })
        .collect::<Vec<_>>();
    drop(status_sender);

    let statuses = receive_statuses(&mut status_receiver, 4).await;
    app.upstream.wait_for_hits(2).await;
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        2
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::TOO_MANY_REQUESTS)
            .count(),
        2
    );
    assert_eq!(app.upstream.hits(), 2);
    app.upstream.release(2);
    for task in tasks {
        task.await.unwrap();
    }
    let day = Local::now().date_naive().to_string();
    assert_eq!(
        AiUsageRepository::new(&app.database)
            .analytics(&day, &day, &day, 10)
            .await
            .unwrap()
            .summary
            .total_requests,
        2
    );
}

#[tokio::test]
async fn admission_enforces_global_stream_capacity_across_distinct_users() {
    let app = configured_app_with_user_count(
        TestProvider::blocking().await,
        AiAdmissionConfig::new(10, 2, 8, 100).unwrap(),
        16,
    )
    .await;
    let (status_sender, mut status_receiver) = tokio::sync::mpsc::unbounded_channel();
    let tasks = app
        .user_tokens
        .iter()
        .map(|token| {
            spawn_body_consuming_chat(
                app.router.clone(),
                token.clone(),
                app.visible_model,
                status_sender.clone(),
            )
        })
        .collect::<Vec<_>>();
    drop(status_sender);

    let statuses = receive_statuses(&mut status_receiver, 16).await;
    app.upstream.wait_for_hits(8).await;
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        8
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::TOO_MANY_REQUESTS)
            .count(),
        8
    );
    assert_eq!(app.upstream.hits(), 8);
    assert_eq!(app.upstream.max_active(), 8);
    app.upstream.release(8);
    for task in tasks {
        task.await.unwrap();
    }
}

#[tokio::test]
async fn admission_releases_stream_permits_on_disconnect_and_provider_error() {
    let app = configured_app_with_config(
        TestProvider::blocking().await,
        AiAdmissionConfig::new(10, 1, 8, 100).unwrap(),
    )
    .await;
    let held = app
        .router
        .clone()
        .oneshot(chat_request(&app.alice_token, app.visible_model))
        .await
        .unwrap();
    assert_eq!(held.status(), StatusCode::OK);
    let rejected = app
        .router
        .clone()
        .oneshot(chat_request(&app.alice_token, app.visible_model))
        .await
        .unwrap();
    assert_rate_limited(&rejected);
    assert_eq!(app.upstream.hits(), 0);

    drop(held);
    let replacement = app
        .router
        .clone()
        .oneshot(chat_request(&app.alice_token, app.visible_model))
        .await
        .unwrap();
    assert_eq!(replacement.status(), StatusCode::OK);
    drop(replacement);

    app.upstream.reject_next();
    let failed = app
        .router
        .clone()
        .oneshot(chat_request(&app.alice_token, app.visible_model))
        .await
        .unwrap();
    assert_eq!(failed.status(), StatusCode::OK);
    let failed_body = tokio::spawn(async move { failed.into_body().collect().await.unwrap() });
    app.upstream.wait_for_hits(1).await;
    app.upstream.release(1);
    let body = String::from_utf8(failed_body.await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(body.contains("AssistantUnavailable"));

    let replacement = app
        .router
        .clone()
        .oneshot(chat_request(&app.alice_token, app.visible_model))
        .await
        .unwrap();
    assert_eq!(replacement.status(), StatusCode::OK);
}

#[tokio::test]
async fn admission_enforces_minute_rate_with_retry_headers_before_provider_io() {
    let minute = configured_app_with_config(
        TestProvider::start().await,
        AiAdmissionConfig::new(1, 2, 8, 100).unwrap(),
    )
    .await;
    let accepted = minute
        .router
        .clone()
        .oneshot(chat_request(&minute.alice_token, minute.visible_model))
        .await
        .unwrap();
    let rejected = minute
        .router
        .clone()
        .oneshot(chat_request(&minute.alice_token, minute.visible_model))
        .await
        .unwrap();
    assert_rate_limited(&rejected);
    assert_eq!(minute.upstream.hits(), 0);
    drop(accepted);
}

#[tokio::test]
async fn admission_enforces_daily_quota_before_provider_io_or_analytics() {
    let daily = configured_app_with_config(
        TestProvider::start().await,
        AiAdmissionConfig::new(10, 2, 8, 1).unwrap(),
    )
    .await;
    let accepted = daily
        .router
        .clone()
        .oneshot(chat_request(&daily.alice_token, daily.visible_model))
        .await
        .unwrap();
    let rejected = daily
        .router
        .clone()
        .oneshot(chat_request(&daily.alice_token, daily.visible_model))
        .await
        .unwrap();
    assert_rate_limited(&rejected);
    assert_eq!(daily.upstream.hits(), 0);
    assert_eq!(
        AiUsageRepository::new(&daily.database)
            .daily_quota_count(daily.alice_user_id, chrono::Utc::now().date_naive())
            .await
            .unwrap(),
        1
    );
    let day = Local::now().date_naive().to_string();
    assert_eq!(
        AiUsageRepository::new(&daily.database)
            .analytics(&day, &day, &day, 10)
            .await
            .unwrap()
            .summary
            .total_requests,
        0
    );
    drop(accepted);
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
async fn favorites_tool_excludes_items_from_disabled_or_unlinked_libraries() {
    let provider = TestProvider::favorites_tool().await;
    let app = configured_app_with(provider.clone()).await;
    let (hidden_item, library_id) = seed_favorite_item(&app, "Hidden Favorite").await;

    let response = app
        .router
        .clone()
        .oneshot(chat_request(&app.alice_token, app.visible_model))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
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
    assert!(events.contains("event: done"));

    assert_hidden_favorite_tool_round(&provider.requests()[1], hidden_item);

    let backend = app.database.get_database_backend();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("is_enabled"), true)
                    .and_where(Expr::col(Alias::new("id")).eq(library_id)),
            ),
        )
        .await
        .unwrap();
    app.database
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("library_catalog_items"))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(hidden_item.as_uuid())),
            ),
        )
        .await
        .unwrap();

    let response = app
        .router
        .clone()
        .oneshot(chat_request(&app.alice_token, app.visible_model))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body().collect().await.unwrap();
    let requests = provider.requests();
    assert_eq!(requests.len(), 4);
    assert_hidden_favorite_tool_round(&requests[3], hidden_item);
}

fn assert_hidden_favorite_tool_round(request: &Value, hidden_item: CatalogItemId) {
    let tool_round = request.to_string();
    assert!(tool_round.contains(r#"\"Items\":[]"#));
    assert!(!tool_round.contains("Hidden Favorite"));
    assert!(!tool_round.contains(&hidden_item.to_string()));
}

async fn seed_favorite_item(app: &ConfiguredApp, name: &str) -> (CatalogItemId, Uuid) {
    let backend = app.database.get_database_backend();
    let library_id = Uuid::new_v4();
    let item_id = CatalogItemId::new();
    app.database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("libraries"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("name"),
                        Alias::new("scan_profile"),
                        Alias::new("object_selection_scope"),
                        Alias::new("metadata_policy"),
                        Alias::new("expansion_policy"),
                        Alias::new("probe_policy"),
                        Alias::new("profile_version"),
                        Alias::new("collection_type"),
                        Alias::new("sort_key"),
                        Alias::new("is_enabled"),
                    ])
                    .values_panic([
                        library_id.into(),
                        "Hidden library".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        b"hidden-library".to_vec().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    app.database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("sort_key"),
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item_id.as_uuid().into(),
                        "Movie".into(),
                        name.into(),
                        name.to_lowercase().into(),
                        name.as_bytes().to_vec().into(),
                        "Matched".into(),
                        "Ready".into(),
                        "NotApplicable".into(),
                        "Indexed".into(),
                        0_i64.into(),
                        0_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    app.database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        library_id.into(),
                        item_id.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    UserDataRepository::new(&app.database)
        .commit(app.alice_user_id, item_id, UserDataPatch::favorite(true))
        .await
        .unwrap();
    app.database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("is_enabled"), false)
                    .and_where(Expr::col(Alias::new("id")).eq(library_id)),
            ),
        )
        .await
        .unwrap();
    (item_id, library_id)
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
    let app = configured_app_with(TestProvider::rejecting().await).await;
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
