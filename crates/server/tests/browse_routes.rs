use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::Duration;
use http_body_util::BodyExt;
use sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
    sea_query::{Alias, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tjxy_application::{AuthService, CatalogQueryService, SystemClock};
use tjxy_common::{CatalogItemId, SortKey};
use tjxy_server::{AppState, ServerIdentity, build_router};
use tower::ServiceExt;
use uuid::Uuid;

const SERVER_ID: &str = "018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11";
const IDENTITY: &str =
    r#"MediaBrowser Client="Findroid", Device="Pixel", DeviceId="phone-1", Version="0.16.0""#;

struct TestApp {
    router: axum::Router,
    database: DatabaseConnection,
}

async fn test_app() -> TestApp {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    database
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await
        .unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let auth = Arc::new(
        AuthService::new(database.clone(), SystemClock, Some(Duration::days(30)), 2)
            .await
            .unwrap(),
    );
    auth.create_user("Alice", "correct horse", true)
        .await
        .unwrap();
    let catalog = Arc::new(CatalogQueryService::new(database.clone()));
    let identity = ServerIdentity::new(Uuid::parse_str(SERVER_ID).unwrap(), "TJXY", "Linux")
        .with_startup_wizard_completed(true);
    TestApp {
        router: build_router(
            AppState::new(identity)
                .with_auth(auth)
                .with_catalog(catalog)
                .with_ready(true),
        ),
        database,
    }
}

async fn login(router: &axum::Router) -> (Uuid, Uuid, String) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(header::AUTHORIZATION, IDENTITY)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username": "alice", "Pw": "correct horse"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let authentication: Value = serde_json::from_slice(&body).unwrap();
    (
        Uuid::parse_str(authentication["User"]["Id"].as_str().unwrap()).unwrap(),
        Uuid::parse_str(authentication["SessionInfo"]["Id"].as_str().unwrap()).unwrap(),
        authentication["AccessToken"].as_str().unwrap().to_owned(),
    )
}

async fn seed_library(database: &DatabaseConnection, name: &str, enabled: bool) -> Uuid {
    let id = Uuid::new_v4();
    let backend = database.get_database_backend();
    database
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
                        id.into(),
                        name.into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text(name).into_bytes().into(),
                        enabled.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    id
}

async fn seed_item(
    database: &DatabaseConnection,
    library_id: Uuid,
    name: &str,
    item_type: &str,
) -> CatalogItemId {
    let id = CatalogItemId::new();
    let backend = database.get_database_backend();
    database
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
                        id.as_uuid().into(),
                        item_type.into(),
                        name.into(),
                        name.to_lowercase().into(),
                        SortKey::from_text(name).into_bytes().into(),
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
    database
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
                        id.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    id
}

async fn get(router: &axum::Router, uri: &str, token: Option<&str>) -> axum::response::Response {
    let mut request = Request::builder().uri(uri);
    if let Some(token) = token {
        request = request.header(
            header::AUTHORIZATION,
            format!(r#"MediaBrowser Token="{token}""#),
        );
    }
    router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn browse_routes_require_a_valid_session() {
    let app = test_app().await;

    assert_eq!(
        get(&app.router, "/UserViews", None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&app.router, "/Items", None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    for path in ["/Sessions/Capabilities/Full", "/Sessions/Capabilities"] {
        let response = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn user_views_and_root_items_return_enabled_libraries_in_the_query_wrapper() {
    let app = test_app().await;
    seed_library(&app.database, "Zeta", true).await;
    seed_library(&app.database, "Alpha", true).await;
    seed_library(&app.database, "Hidden", false).await;
    let (user_id, _, token) = login(&app.router).await;

    for path in [
        format!("/UserViews?userId={user_id}"),
        format!("/Items?userId={user_id}"),
    ] {
        let response = get(&app.router, &path, Some(&token)).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let result: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["TotalRecordCount"], 2);
        assert_eq!(result["StartIndex"], 0);
        assert_eq!(result["Items"][0]["Name"], "Alpha");
        assert_eq!(result["Items"][0]["Type"], "CollectionFolder");
        assert_eq!(result["Items"][0]["CollectionType"], "movies");
        assert_eq!(result["Items"][1]["Name"], "Zeta");
    }
}

#[tokio::test]
async fn items_apply_parent_paging_and_findroid_type_filter() {
    let app = test_app().await;
    let library = seed_library(&app.database, "Library", true).await;
    seed_item(&app.database, library, "Arrival", "Movie").await;
    seed_item(&app.database, library, "Blade Runner", "Movie").await;
    seed_item(&app.database, library, "Dark", "Series").await;
    let (user_id, _, token) = login(&app.router).await;

    let path = format!(
        "/Items?userId={user_id}&parentId={library}&includeItemTypes=Movie&recursive=false&sortBy=SortName&sortOrder=Ascending&startIndex=1&limit=1"
    );
    let response = get(&app.router, &path, Some(&token)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["TotalRecordCount"], 2);
    assert_eq!(result["StartIndex"], 1);
    assert_eq!(result["Items"].as_array().unwrap().len(), 1);
    assert_eq!(result["Items"][0]["Name"], "Blade Runner");
    assert_eq!(result["Items"][0]["Type"], "Movie");
    assert_eq!(result["Items"][0]["MediaType"], "Video");
}

#[tokio::test]
async fn browse_queries_reject_impersonation_unknown_keys_and_invalid_pages() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;

    for path in [
        format!("/UserViews?userId={}", Uuid::new_v4()),
        "/UserViews?unexpected=1".to_owned(),
        "/UserViews?userId=bad".to_owned(),
        "/Items?limit=0".to_owned(),
        "/Items?limit=201".to_owned(),
        "/Items?limit=1&limit=2".to_owned(),
    ] {
        let response = get(&app.router, &path, Some(&token)).await;
        let expected = if path.contains("userId=") && !path.contains("bad") {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::BAD_REQUEST
        };
        assert_eq!(response.status(), expected, "{path}");
    }
}

async fn post(
    router: &axum::Router,
    uri: &str,
    token: &str,
    body: impl Into<Body>,
) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(body.into())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn full_capabilities_are_persisted_atomically_for_the_authenticated_session() {
    let app = test_app().await;
    let (_, session_id, token) = login(&app.router).await;
    let full = json!({
        "PlayableMediaTypes": ["Video", "Audio"],
        "SupportedCommands": ["Play", "Stop"],
        "SupportsMediaControl": true,
        "SupportsPersistentIdentifier": true,
        "DeviceProfile": {"Name": "Findroid"},
        "AppStoreUrl": "https://example.invalid/app",
        "IconUrl": "https://example.invalid/icon"
    });

    let response = post(
        &app.router,
        &format!("/Sessions/Capabilities/Full?id={session_id}"),
        &token,
        full.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .len(),
        0
    );

    let backend = app.database.get_database_backend();
    let row = app
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("playable_media_types"),
                        Alias::new("supported_commands"),
                        Alias::new("supports_media_control"),
                        Alias::new("supports_persistent_identifier"),
                        Alias::new("device_profile"),
                        Alias::new("app_store_url"),
                        Alias::new("icon_url"),
                    ])
                    .from(Alias::new("auth_sessions"))
                    .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(session_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<Value>("", "playable_media_types").unwrap(),
        json!(["Video", "Audio"])
    );
    assert_eq!(
        row.try_get::<Value>("", "supported_commands").unwrap(),
        json!(["Play", "Stop"])
    );
    assert!(row.try_get::<bool>("", "supports_media_control").unwrap());
    assert!(
        row.try_get::<bool>("", "supports_persistent_identifier")
            .unwrap()
    );
    assert_eq!(
        row.try_get::<Value>("", "device_profile").unwrap(),
        json!({"Name": "Findroid"})
    );

    let malformed = post(&app.router, "/Sessions/Capabilities/Full", &token, "{").await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let row = app
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("device_profile"))
                    .from(Alias::new("auth_sessions"))
                    .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(session_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<Value>("", "device_profile").unwrap(),
        json!({"Name": "Findroid"})
    );
}

#[tokio::test]
async fn legacy_capabilities_and_full_query_boundaries_are_explicit() {
    let app = test_app().await;
    let (_, _, token) = login(&app.router).await;

    let response = post(
        &app.router,
        "/Sessions/Capabilities?playableMediaTypes=Video&supportedCommands=Play,Pause&supportsMediaControl=false",
        &token,
        Body::empty(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = post(
        &app.router,
        &format!("/Sessions/Capabilities/Full?id={}", Uuid::new_v4()),
        &token,
        "{}",
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = post(
        &app.router,
        "/Sessions/Capabilities/Full?unexpected=1",
        &token,
        "{}",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn empty_full_capabilities_persist_protocol_defaults() {
    let app = test_app().await;
    let (_, session_id, token) = login(&app.router).await;

    let response = post(&app.router, "/Sessions/Capabilities/Full", &token, "{}").await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let backend = app.database.get_database_backend();
    let row = app
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("playable_media_types"),
                        Alias::new("supported_commands"),
                        Alias::new("supports_media_control"),
                        Alias::new("supports_persistent_identifier"),
                        Alias::new("device_profile"),
                        Alias::new("app_store_url"),
                        Alias::new("icon_url"),
                    ])
                    .from(Alias::new("auth_sessions"))
                    .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(session_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<Value>("", "playable_media_types").unwrap(),
        json!([])
    );
    assert_eq!(
        row.try_get::<Value>("", "supported_commands").unwrap(),
        json!([])
    );
    assert!(!row.try_get::<bool>("", "supports_media_control").unwrap());
    assert!(
        !row.try_get::<bool>("", "supports_persistent_identifier")
            .unwrap()
    );
    assert!(
        row.try_get::<Option<Value>>("", "device_profile")
            .unwrap()
            .is_none()
    );
    assert!(
        row.try_get::<Option<String>>("", "app_store_url")
            .unwrap()
            .is_none()
    );
    assert!(
        row.try_get::<Option<String>>("", "icon_url")
            .unwrap()
            .is_none()
    );
}
