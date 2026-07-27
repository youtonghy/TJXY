use std::sync::{Arc, Mutex};

use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Order, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tjxy_db::{ImportJobRepository, ImportJobState, ImportStagingItem};
use tjxy_import::{EmbyApiCredentials, EmbyApiImporter, EmbyApiTransport};
use tjxy_test_support::test_database;

struct FakeTransport {
    starts: Mutex<Vec<u64>>,
}

#[async_trait::async_trait]
impl EmbyApiTransport for FakeTransport {
    async fn fetch_items(
        &self,
        _base_url: &reqwest::Url,
        user_id: &str,
        api_key: &str,
        start_index: u64,
        limit: u64,
    ) -> Result<Value, tjxy_import::EmbyImportError> {
        assert_eq!(user_id, "emby-user");
        assert_eq!(api_key, "api-secret");
        assert_eq!(limit, 200);
        self.starts.lock().unwrap().push(start_index);
        Ok(match start_index {
            0 => json!({
                "Items": [{
                    "Id":"movie-1","Name":"Arrival","Type":"Movie","ProductionYear":2016,
                    "Overview":"First contact","Path":"/legacy/Arrival.mkv",
                    "ProviderIds":{"Imdb":"tt2543164"},"Genres":["Science Fiction"],
                    "UserData":{"IsFavorite":true,"Played":false,"PlayCount":1,"PlaybackPositionTicks":123}
                }],
                "TotalRecordCount": 2
            }),
            1 => json!({
                "Items": [{
                    "Id":"episode-1","Name":"Pilot","Type":"Episode","ParentId":"season-1",
                    "SeriesId":"series-1","SeasonId":"season-1","IndexNumber":1,
                    "UserData":{"IsFavorite":false,"Played":true,"PlayCount":2,"PlaybackPositionTicks":0}
                }],
                "TotalRecordCount": 2
            }),
            _ => panic!("unexpected page start {start_index}"),
        })
    }
}

#[tokio::test]
async fn dry_run_pages_into_replay_safe_staging_and_completes_without_catalog_publish() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let repository = ImportJobRepository::new(&database);
    let job = repository
        .create("EmbyApi", "emby-instance", true)
        .await
        .unwrap();
    let claimed = repository
        .claim_next("EmbyApi", "import-worker", chrono::Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    let transport = Arc::new(FakeTransport {
        starts: Mutex::new(Vec::new()),
    });
    let credentials =
        EmbyApiCredentials::new("http://127.0.0.1:8096", "emby-user", "api-secret").unwrap();
    assert!(!format!("{credentials:?}").contains("api-secret"));
    let payload = credentials.to_payload_json().unwrap();
    let credentials = EmbyApiCredentials::from_payload_json(&payload).unwrap();
    let importer = EmbyApiImporter::new(database.clone(), credentials)
        .unwrap()
        .with_transport(transport.clone());

    let report = importer.run_claimed(&claimed).await.unwrap();

    assert_eq!(report.items(), 2);
    assert_eq!(transport.starts.lock().unwrap().as_slice(), [0, 1]);
    let completed = repository.get(job.id()).await.unwrap().unwrap();
    assert_eq!(completed.state(), ImportJobState::Completed);
    assert_eq!(completed.checkpoint(), &json!({"start_index":2}));
    let backend = database.get_database_backend();
    let staged = database
        .query_all(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("entity_kind"),
                        Alias::new("legacy_item_id"),
                        Alias::new("payload"),
                    ])
                    .from(Alias::new("import_staging_items"))
                    .order_by(Alias::new("legacy_item_id"), Order::Asc),
            ),
        )
        .await
        .unwrap();
    assert_eq!(staged.len(), 2);
    assert_eq!(
        staged[0].try_get::<String>("", "entity_kind").unwrap(),
        "Episode"
    );
    let movie: Value = staged[1].try_get("", "payload").unwrap();
    assert_eq!(movie["user_data"]["is_favorite"], true);
    assert_eq!(movie["provider_ids"]["Imdb"], "tt2543164");
    let catalog_count = database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("catalog_items")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_eq!(catalog_count, 0);
}

#[tokio::test]
async fn resumed_import_reports_the_total_including_checkpointed_items() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let repository = ImportJobRepository::new(&database);
    let job = repository
        .create("EmbyApi", "emby-instance", true)
        .await
        .unwrap();
    let claimed = repository
        .claim_next("EmbyApi", "import-worker", chrono::Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    repository
        .stage_item(
            &claimed,
            &ImportStagingItem::new("Movie", "movie-1", None, json!({"version":1})).unwrap(),
        )
        .await
        .unwrap();
    repository
        .save_checkpoint(&claimed, json!({"start_index":1}))
        .await
        .unwrap();
    repository.pause(job.id()).await.unwrap();
    repository.resume(job.id()).await.unwrap();
    let claimed = repository
        .claim_next("EmbyApi", "import-worker", chrono::Duration::seconds(30))
        .await
        .unwrap()
        .unwrap();
    let transport = Arc::new(FakeTransport {
        starts: Mutex::new(Vec::new()),
    });
    let importer = EmbyApiImporter::new(
        database.clone(),
        EmbyApiCredentials::new("http://127.0.0.1:8096", "emby-user", "api-secret").unwrap(),
    )
    .unwrap()
    .with_transport(Arc::clone(&transport));

    let report = importer.run_claimed(&claimed).await.unwrap();

    assert_eq!(report.items(), 2);
    assert_eq!(transport.starts.lock().unwrap().as_slice(), [1]);
    let completed = repository.get(job.id()).await.unwrap().unwrap();
    assert_eq!(completed.counters()["items"], 2);
}
