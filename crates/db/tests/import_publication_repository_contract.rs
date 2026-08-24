use chrono::{Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tjxy_common::Username;
use tjxy_db::{
    AuthRepository, ImportJobRepository, ImportJobState, ImportPublicationRepository,
    ImportPublicationTarget, ImportStagingItem,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_target(database: &DatabaseConnection) -> (Uuid, Uuid) {
    let user = AuthRepository::new(database)
        .create_user(
            &Username::parse("import-user").unwrap(),
            "$argon2id$test",
            false,
            false,
            Utc::now(),
        )
        .await
        .unwrap()
        .id()
        .as_uuid();
    let library = Uuid::new_v4();
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
                        library.into(),
                        "Imported".into(),
                        "Manual".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "manual".into(),
                        "on_playback".into(),
                        1.into(),
                        "mixed".into(),
                        b"imported".to_vec().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    (library, user)
}

async fn sealed_job(database: &DatabaseConnection) -> Uuid {
    let repository = ImportJobRepository::new(database);
    let job = repository
        .create("EmbyApi", "legacy-server", false)
        .await
        .unwrap();
    let claimed = repository
        .claim_next("EmbyApi", "publisher-test", Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    repository
        .stage_item(
            &claimed,
            &ImportStagingItem::new(
                "Series",
                "series-1",
                None,
                json!({
                    "version":1,"name":"A Show","production_year":2020,"overview":"Overview",
                    "provider_ids":{"Imdb":"tt100"},"genres":["Drama"],
                    "people":[{"Name":"Director One","Type":"Director","Role":""}],
                    "studios":[{"Name":"Studio One"}],
                    "user_data":{"is_favorite":false,"played":false,"play_count":0,"playback_position_ticks":0}
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    repository
        .stage_item(
            &claimed,
            &ImportStagingItem::new(
                "Episode",
                "episode-1",
                Some("series-1".to_owned()),
                json!({
                    "version":1,"name":"Pilot","production_year":2020,"overview":null,
                    "provider_ids":{},"genres":[],
                    "user_data":{"is_favorite":true,"played":true,"play_count":2,"playback_position_ticks":42}
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    repository
        .seal_for_publication(&claimed, json!({"items":2,"conflicts":0,"errors":0}))
        .await
        .unwrap();
    job.id()
}

#[tokio::test]
async fn sealed_generation_publishes_atomically_and_replays_idempotently() {
    let database = database().await;
    let (library, user) = seed_target(&database).await;
    let job = sealed_job(&database).await;
    let repository = ImportPublicationRepository::new(&database);

    let first = repository
        .publish(job, ImportPublicationTarget::new(library, user))
        .await
        .unwrap();
    let replay = repository
        .publish(job, ImportPublicationTarget::new(library, user))
        .await
        .unwrap();

    assert_eq!(first.items(), 2);
    assert!(!first.replayed());
    assert!(replay.replayed());
    assert_eq!(count(&database, "catalog_items").await, 2);
    assert_eq!(count(&database, "library_catalog_items").await, 2);
    assert_eq!(count(&database, "legacy_item_mappings").await, 2);
    assert_eq!(count(&database, "provider_ids").await, 1);
    assert_eq!(count(&database, "item_genres").await, 1);
    assert_eq!(count(&database, "item_people").await, 1);
    assert_eq!(count(&database, "item_studios").await, 1);
    assert_eq!(count(&database, "user_data").await, 2);
    assert_eq!(count(&database, "cache_invalidation_outbox").await, 0);
    assert_eq!(
        ImportJobRepository::new(&database)
            .get(job)
            .await
            .unwrap()
            .unwrap()
            .state(),
        ImportJobState::Completed
    );
}

#[tokio::test]
async fn publication_failure_rolls_back_every_catalog_side_effect() {
    let database = database().await;
    let (library, _) = seed_target(&database).await;
    let job = sealed_job(&database).await;

    ImportPublicationRepository::new(&database)
        .publish(job, ImportPublicationTarget::new(library, Uuid::new_v4()))
        .await
        .unwrap_err();

    assert_eq!(count(&database, "catalog_items").await, 0);
    assert_eq!(count(&database, "library_catalog_items").await, 0);
    assert_eq!(count(&database, "legacy_item_mappings").await, 0);
    assert_eq!(count(&database, "provider_ids").await, 0);
    assert_eq!(count(&database, "item_genres").await, 0);
    assert_eq!(count(&database, "user_data").await, 0);
    assert_eq!(
        ImportJobRepository::new(&database)
            .get(job)
            .await
            .unwrap()
            .unwrap()
            .state(),
        ImportJobState::ReadyToPublish
    );
}

async fn count(database: &DatabaseConnection, table: &str) -> i64 {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new(table)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap()
}
