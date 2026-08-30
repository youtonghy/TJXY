use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tjxy_common::{CatalogItemId, ImageType};
use tjxy_db::{
    AssetPublication, DemoCatalogPublication, DemoCatalogRepository, Migrator, demo_catalog_item_id,
};
use tjxy_metadata::{
    MetadataItemKind, MetadataProviderError, TmdbCatalogClient, TmdbCatalogTransport,
};
use tjxy_test_support::test_database;

struct FixtureTransport {
    responses: BTreeMap<String, Vec<u8>>,
}

#[async_trait]
impl TmdbCatalogTransport for FixtureTransport {
    async fn get(
        &self,
        path: &str,
        _query: &[(String, String)],
    ) -> Result<Vec<u8>, MetadataProviderError> {
        self.responses
            .get(path)
            .cloned()
            .ok_or(MetadataProviderError::Rejected)
    }
}

fn fixture_transport() -> Arc<FixtureTransport> {
    Arc::new(FixtureTransport {
        responses: BTreeMap::from([
            (
                "/movie/329865".to_owned(),
                serde_json::to_vec(&json!({
                    "id": 329_865,
                    "title": "Arrival",
                    "original_title": "Arrival",
                    "overview": "A linguist communicates with visitors.",
                    "release_date": "2016-11-10",
                    "runtime": 116,
                    "status": "Released",
                    "vote_average": 8.1,
                    "vote_count": 19_000,
                    "poster_path": "/arrival.jpg",
                    "original_language": "en",
                    "genres": [
                        {"id": 878, "name": "Science Fiction"},
                        {"id": 878, "name": "Science Fiction"}
                    ],
                    "production_companies": [
                        {"id": 1, "name": "FilmNation"},
                        {"id": 1, "name": "FilmNation"}
                    ],
                    "production_countries": [
                        {"iso_3166_1": "US", "name": "United States"},
                        {"iso_3166_1": "US", "name": "United States"}
                    ],
                    "spoken_languages": [
                        {"english_name": "English", "iso_639_1": "en", "name": "English"},
                        {"english_name": "English", "iso_639_1": "en", "name": "English"}
                    ],
                    "credits": {
                        "cast": [
                            {"id": 101, "name": "Amy Adams", "character": "Louise Banks", "order": 0, "profile_path": "/amy.jpg"},
                            {"id": 101, "name": "Amy Adams", "character": "Louise Banks", "order": 0, "profile_path": "/amy.jpg"}
                        ],
                        "crew": [
                            {"id": 102, "name": "Denis Villeneuve", "job": "Director", "department": "Directing", "profile_path": "/denis.jpg"},
                            {"id": 102, "name": "Denis Villeneuve", "job": "Director", "department": "Directing", "profile_path": "/denis.jpg"}
                        ]
                    },
                    "external_ids": {"imdb_id": "tt2543164"}
                }))
                .unwrap(),
            ),
            (
                "/tv/87108".to_owned(),
                serde_json::to_vec(&json!({
                    "id": 329_865,
                    "name": "Chernobyl",
                    "original_name": "Chernobyl",
                    "overview": "A disaster and its aftermath.",
                    "first_air_date": "2019-05-06",
                    "last_air_date": "2019-06-03",
                    "status": "Ended",
                    "vote_average": 8.7,
                    "vote_count": 7_000,
                    "episode_run_time": [60],
                    "poster_path": "/chernobyl.jpg",
                    "original_language": "en",
                    "seasons": [{"id": 120_000, "season_number": 1, "name": "Season 1", "episode_count": 1}],
                    "aggregate_credits": {"cast": [], "crew": []},
                    "external_ids": {"imdb_id": "tt7366338", "tvdb_id": 360_893}
                }))
                .unwrap(),
            ),
            (
                "/tv/87108/season/1".to_owned(),
                serde_json::to_vec(&json!({
                    "id": 120_000,
                    "name": "Season 1",
                    "overview": "The only season.",
                    "air_date": "2019-05-06",
                    "season_number": 1,
                    "poster_path": "/chernobyl-s1.jpg",
                    "episodes": [{
                        "id": 170_001,
                        "name": "1:23:45",
                        "overview": "The first episode.",
                        "air_date": "2019-05-06",
                        "episode_number": 1,
                        "season_number": 1,
                        "runtime": 59,
                        "vote_average": 8.7,
                        "vote_count": 600,
                        "still_path": "/e1.jpg",
                        "guest_stars": [],
                        "crew": []
                    }],
                    "credits": {"cast": [], "crew": []}
                }))
                .unwrap(),
            ),
        ]),
    })
}

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    database
}

async fn count(database: &DatabaseConnection, table: &str) -> i64 {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                &Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new(table))
                    .to_owned(),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap()
}

async fn generation(database: &DatabaseConnection) -> i64 {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                &Query::select()
                    .column(Alias::new("generation"))
                    .from(Alias::new("catalog_state"))
                    .and_where(Expr::col(Alias::new("id")).eq(1))
                    .to_owned(),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "generation")
        .unwrap()
}

async fn library_names(database: &DatabaseConnection) -> Vec<String> {
    let backend = database.get_database_backend();
    let rows = database
        .query_all(
            backend.build(
                &Query::select()
                    .column(Alias::new("name"))
                    .from(Alias::new("libraries"))
                    .order_by(Alias::new("name"), sea_orm::sea_query::Order::Asc)
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    rows.into_iter()
        .map(|row| row.try_get("", "name").unwrap())
        .collect()
}

async fn library_policies(
    database: &DatabaseConnection,
) -> Vec<(String, String, String, String, String)> {
    let backend = database.get_database_backend();
    let rows = database
        .query_all(
            backend.build(
                &Query::select()
                    .columns([
                        Alias::new("scan_profile"),
                        Alias::new("object_selection_scope"),
                        Alias::new("metadata_policy"),
                        Alias::new("expansion_policy"),
                        Alias::new("probe_policy"),
                    ])
                    .from(Alias::new("libraries"))
                    .order_by(Alias::new("name"), sea_orm::sea_query::Order::Asc)
                    .to_owned(),
            ),
        )
        .await
        .unwrap();
    rows.into_iter()
        .map(|row| {
            (
                row.try_get("", "scan_profile").unwrap(),
                row.try_get("", "object_selection_scope").unwrap(),
                row.try_get("", "metadata_policy").unwrap(),
                row.try_get("", "expansion_policy").unwrap(),
                row.try_get("", "probe_policy").unwrap(),
            )
        })
        .collect()
}

#[tokio::test]
async fn demo_publication_is_idempotent_and_keeps_every_descendant_visible_without_sources() {
    let database = database().await;
    let client = TmdbCatalogClient::with_transport("en-US", fixture_transport()).unwrap();
    let movie = client.movie(329_865).await.unwrap();
    let series = client.series(87_108).await.unwrap();
    let poster = AssetPublication::new(
        CatalogItemId::from_uuid(demo_catalog_item_id(MetadataItemKind::Movie, 329_865)),
        ImageType::Primary,
        0,
        "a".repeat(64),
        "image/jpeg",
        1000,
        1500,
        42,
        "aa/arrival.jpg",
        "Tmdb",
        Some("/arrival.jpg".to_owned()),
    )
    .unwrap();
    let publication = DemoCatalogPublication::new(vec![movie], vec![series], "en-US", Utc::now())
        .unwrap()
        .with_assets(vec![poster])
        .unwrap();
    let repository = DemoCatalogRepository::new(&database);
    let initial_generation = generation(&database).await;

    let first = repository.publish(&publication).await.unwrap();
    let after_first = generation(&database).await;
    let second = repository.publish(&publication).await.unwrap();

    assert_eq!(first.movies(), 1);
    assert_eq!(first.series(), 1);
    assert_eq!(first.seasons(), 1);
    assert_eq!(first.episodes(), 1);
    assert_eq!(second, first);
    assert_eq!(count(&database, "libraries").await, 2);
    assert_eq!(library_names(&database).await, ["Movies", "TV Shows"]);
    assert_eq!(
        library_policies(&database).await,
        [
            (
                "Manual".to_owned(),
                "library_roots".to_owned(),
                "none".to_owned(),
                "manual".to_owned(),
                "on_playback".to_owned(),
            ),
            (
                "Manual".to_owned(),
                "library_roots".to_owned(),
                "none".to_owned(),
                "manual".to_owned(),
                "on_playback".to_owned(),
            ),
        ]
    );
    assert_eq!(count(&database, "catalog_items").await, 4);
    assert_eq!(count(&database, "library_catalog_items").await, 4);
    assert_eq!(count(&database, "provider_ids").await, 7);
    assert_eq!(count(&database, "genres").await, 1);
    assert_eq!(count(&database, "studios").await, 1);
    assert_eq!(count(&database, "people").await, 2);
    assert_eq!(count(&database, "item_people").await, 2);
    assert_eq!(count(&database, "countries").await, 1);
    assert_eq!(count(&database, "languages").await, 1);
    assert_eq!(count(&database, "metadata_snapshots").await, 4);
    assert_eq!(count(&database, "asset_blobs").await, 1);
    assert_eq!(count(&database, "item_assets").await, 1);
    assert_eq!(count(&database, "media_sources").await, 0);
    assert_eq!(count(&database, "media_locations").await, 0);
    assert_eq!(after_first, initial_generation + 1);
    assert_eq!(generation(&database).await, after_first + 1);
}

#[tokio::test]
async fn demo_publication_accepts_a_complete_hundred_series_image_set_with_a_bounded_limit() {
    let client = TmdbCatalogClient::with_transport("en-US", fixture_transport()).unwrap();
    let movie = client.movie(329_865).await.unwrap();
    let series = client.series(87_108).await.unwrap();
    let poster = AssetPublication::new(
        CatalogItemId::from_uuid(demo_catalog_item_id(MetadataItemKind::Movie, 329_865)),
        ImageType::Primary,
        0,
        "a".repeat(64),
        "image/jpeg",
        1000,
        1500,
        42,
        "aa/arrival.jpg",
        "Tmdb",
        Some("/arrival.jpg".to_owned()),
    )
    .unwrap();
    let publication = DemoCatalogPublication::new(
        vec![movie.clone()],
        vec![series.clone()],
        "en-US",
        Utc::now(),
    )
    .unwrap();

    assert!(
        publication
            .clone()
            .with_assets(vec![poster.clone(); 1_001])
            .is_ok()
    );
    assert!(
        DemoCatalogPublication::new(vec![movie], vec![series], "en-US", Utc::now())
            .unwrap()
            .with_assets(vec![poster; 50_001])
            .is_err()
    );
}

#[tokio::test]
async fn demo_publication_rolls_back_the_complete_projection_when_an_association_fails() {
    let database = database().await;
    install_reject_demo_credit_trigger(&database).await;
    let client = TmdbCatalogClient::with_transport("en-US", fixture_transport()).unwrap();
    let publication = DemoCatalogPublication::new(
        vec![client.movie(329_865).await.unwrap()],
        vec![client.series(87_108).await.unwrap()],
        "en-US",
        Utc::now(),
    )
    .unwrap();
    let repository = DemoCatalogRepository::new(&database);
    let initial_generation = generation(&database).await;

    assert!(repository.publish(&publication).await.is_err());
    assert_eq!(count(&database, "libraries").await, 0);
    assert_eq!(count(&database, "catalog_items").await, 0);
    assert_eq!(count(&database, "library_catalog_items").await, 0);
    assert_eq!(count(&database, "provider_ids").await, 0);
    assert_eq!(count(&database, "metadata_snapshots").await, 0);
    assert_eq!(generation(&database).await, initial_generation);
}

async fn install_reject_demo_credit_trigger(database: &DatabaseConnection) {
    match database.get_database_backend() {
        DbBackend::Sqlite => {
            database
                .execute_unprepared(
                    "CREATE TRIGGER reject_demo_credit \
                     BEFORE INSERT ON item_people \
                     BEGIN SELECT RAISE(ABORT, 'forced association failure'); END",
                )
                .await
                .unwrap();
        }
        DbBackend::Postgres => {
            database
                .execute_unprepared(
                    "CREATE FUNCTION reject_demo_credit_function() RETURNS trigger \
                     LANGUAGE plpgsql AS $$ BEGIN \
                     RAISE EXCEPTION 'forced association failure'; \
                     END; $$",
                )
                .await
                .unwrap();
            database
                .execute_unprepared(
                    "CREATE TRIGGER reject_demo_credit \
                     BEFORE INSERT ON item_people FOR EACH ROW \
                     EXECUTE FUNCTION reject_demo_credit_function()",
                )
                .await
                .unwrap();
        }
        DbBackend::MySql => {
            database
                .execute_unprepared(
                    "CREATE TRIGGER reject_demo_credit \
                     BEFORE INSERT ON item_people FOR EACH ROW \
                     SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'forced association failure'",
                )
                .await
                .unwrap();
        }
    }
}
