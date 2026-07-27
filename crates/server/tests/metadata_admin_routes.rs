use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Query},
};
use serde_json::Value;
use tempfile::TempDir;
use tjxy_server::{BootstrapAdmin, ServerIdentity, StartupOptions, build_router, initialize};
use tjxy_test_support::reconnectable_test_database;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn nfo_metadata_import_requires_an_administrator_and_publishes_the_result() {
    let data = TempDir::new().unwrap();
    let database = reconnectable_test_database().await.unwrap();
    let state = initialize(
        StartupOptions::new(
            database.database_url(),
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(data.path().join("assets"))
        .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password")),
    )
    .await
    .unwrap();
    let app = build_router(state);
    let item = seed_item(database.connection()).await;
    let uri = format!("/Admin/Items/{item}/Metadata/Nfo");
    let nfo = r#"<movie><title>Arrival</title><year>2016</year><plot>A linguist meets visitors.</plot><uniqueid type="tmdb">329865</uniqueid></movie>"#;

    let unauthenticated = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&uri)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from(nfo))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let token = login(&app).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&uri)
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
                .body(Body::from(nfo))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["Changed"], true);
    assert_eq!(body["State"], "Ready");
}

async fn seed_item(database: &sea_orm::DatabaseConnection) -> Uuid {
    let item = Uuid::new_v4();
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
                        item.into(),
                        "Movie".into(),
                        "Fallback".into(),
                        "fallback".into(),
                        b"fallback".to_vec().into(),
                        "Matched".into(),
                        "Empty".into(),
                        "NotApplicable".into(),
                        "Unknown".into(),
                        0_i64.into(),
                        0_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    item
}

async fn login(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(
                    header::AUTHORIZATION,
                    r#"MediaBrowser Client="MetadataAdminTest", Device="Test", DeviceId="1", Version="1""#,
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"Username":"admin","Pw":"first password"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    body["AccessToken"].as_str().unwrap().to_owned()
}
