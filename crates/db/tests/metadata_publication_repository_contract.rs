use chrono::NaiveDate;
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Order, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::CatalogItemId;
use tjxy_db::MetadataPublicationRepository;
use tjxy_metadata::{
    MetadataCandidate, MetadataItemKind, MetadataLookup, MetadataNamedValue, MetadataPerson,
    MetadataResolution, MetadataSource, NfoDocument,
};
use tjxy_test_support::test_database;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_item(database: &DatabaseConnection) -> CatalogItemId {
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
                        "Movie".into(),
                        "Folder Name".into(),
                        "folder name".into(),
                        b"folder name".to_vec().into(),
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
    id
}

#[tokio::test]
async fn audio_metadata_publishes_music_provider_identities() {
    let database = database().await;
    let item = seed_item(&database).await;
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("item_type"), "Audio")
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let lookup = MetadataLookup::new(MetadataItemKind::Audio, "Coldplay - Yellow", None).unwrap();
    let source = MetadataSource::new(
        "MusicBrainz",
        Some("recording:a1f8f8e1-1d21-4b82-9e6f-1f6020480173"),
        7_500,
    )
    .unwrap();
    let resolution = MetadataResolution::from_candidate(
        &lookup,
        MetadataCandidate::new(source)
            .with_title("Yellow")
            .with_year(2000)
            .with_provider_id(
                "musicbrainz:recording",
                "a1f8f8e1-1d21-4b82-9e6f-1f6020480173",
            )
            .with_provider_id("theaudiodb", "32778411"),
    )
    .unwrap();
    let repository = MetadataPublicationRepository::new(&database);

    assert!(
        repository
            .publish(item, &resolution)
            .await
            .unwrap()
            .changed()
    );
    assert!(
        !repository
            .publish(item, &resolution)
            .await
            .unwrap()
            .changed()
    );

    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([Alias::new("name"), Alias::new("production_year")])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "name").unwrap(), "Yellow");
    assert_eq!(row.try_get::<i32>("", "production_year").unwrap(), 2000);
    assert_eq!(count(&database, "provider_ids").await, 2);
}

#[tokio::test]
async fn metadata_publication_is_atomic_provenanced_and_replay_safe() {
    let database = database().await;
    let item = seed_item(&database).await;
    let document = NfoDocument::parse(
        br#"<movie><title>Arrival</title><originaltitle>Story of Your Life</originaltitle><year>2016</year><plot>A linguist meets visitors.</plot><uniqueid type="tmdb">329865</uniqueid></movie>"#,
        "Arrival/movie.nfo",
    )
    .unwrap();
    let lookup = MetadataLookup::new(document.kind(), "Folder Name", None).unwrap();
    let resolution =
        MetadataResolution::from_candidate(&lookup, document.into_candidate()).unwrap();
    let repository = MetadataPublicationRepository::new(&database);

    let first = repository.publish(item, &resolution).await.unwrap();
    let replay = repository.publish(item, &resolution).await.unwrap();

    assert!(first.changed());
    assert!(!replay.changed());
    assert_eq!(count(&database, "cache_invalidation_outbox").await, 1);
    let backend = database.get_database_backend();
    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("name"),
                        Alias::new("original_title"),
                        Alias::new("sort_name"),
                        Alias::new("sort_key"),
                        Alias::new("production_year"),
                        Alias::new("overview"),
                        Alias::new("metadata_state"),
                        Alias::new("metadata_revision"),
                        Alias::new("metadata_resolved_revision"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "name").unwrap(), "Arrival");
    assert_eq!(
        row.try_get::<String>("", "original_title").unwrap(),
        "Story of Your Life"
    );
    assert_eq!(row.try_get::<String>("", "sort_name").unwrap(), "arrival");
    assert_eq!(row.try_get::<Vec<u8>>("", "sort_key").unwrap(), b"arrival");
    assert_eq!(row.try_get::<i32>("", "production_year").unwrap(), 2016);
    assert_eq!(
        row.try_get::<String>("", "overview").unwrap(),
        "A linguist meets visitors."
    );
    assert_eq!(
        row.try_get::<String>("", "metadata_state").unwrap(),
        "Ready"
    );
    assert_eq!(count(&database, "provider_ids").await, 1);
    assert_eq!(count(&database, "metadata_provenance").await, 5);
    assert_eq!(row.try_get::<i64>("", "metadata_revision").unwrap(), 1);
    assert_eq!(
        row.try_get::<i64>("", "metadata_resolved_revision")
            .unwrap(),
        1
    );
    let provenance = database
        .query_all(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("source_provider"),
                        Alias::new("source_reference"),
                        Alias::new("value_hash"),
                    ])
                    .from(Alias::new("metadata_provenance")),
            ),
        )
        .await
        .unwrap();
    for row in provenance {
        assert_eq!(row.try_get::<String>("", "source_provider").unwrap(), "Nfo");
        assert_eq!(
            row.try_get::<String>("", "source_reference").unwrap(),
            "Arrival/movie.nfo"
        );
        let hash = row.try_get::<String>("", "value_hash").unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
    assert_eq!(generation(&database).await, 1);
}

#[tokio::test]
async fn rich_metadata_publishes_details_and_classification_atomically() {
    let database = database().await;
    let item = seed_item(&database).await;
    let lookup = MetadataLookup::new(MetadataItemKind::Movie, "Toy Story 5", Some(2026)).unwrap();
    let source = MetadataSource::new("Tmdb", Some("movie:1084244"), 8_000).unwrap();
    let resolution = MetadataResolution::from_candidate(
        &lookup,
        MetadataCandidate::new(source)
            .with_title("玩具总动员5")
            .with_original_title("Toy Story 5")
            .with_year(2026)
            .with_overview("Woody and Buzz return.")
            .with_provider_id("tmdb", "1084244")
            .with_community_rating(7.4)
            .with_vote_count(1250)
            .with_runtime_ticks(60_000_000_000)
            .with_premiere_date(NaiveDate::from_ymd_opt(2026, 6, 19).unwrap())
            .with_release_status("Released")
            .with_official_rating("PG")
            .with_original_language("en")
            .with_genres(vec!["Animation".to_owned(), "Family".to_owned()])
            .with_studios(vec!["Pixar".to_owned()])
            .with_countries(vec![
                MetadataNamedValue::new("US", "United States").unwrap(),
            ])
            .with_languages(vec![MetadataNamedValue::new("en", "English").unwrap()])
            .with_people(vec![
                MetadataPerson::new("Tom Hanks", Some("Woody"), Some(0)).unwrap(),
            ])
            .with_details_loaded(),
    )
    .unwrap();
    let repository = MetadataPublicationRepository::new(&database);

    assert!(
        repository
            .publish(item, &resolution)
            .await
            .unwrap()
            .changed()
    );
    assert!(
        !repository
            .publish(item, &resolution)
            .await
            .unwrap()
            .changed()
    );

    let backend = database.get_database_backend();
    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("community_rating"),
                        Alias::new("vote_count"),
                        Alias::new("runtime_ticks"),
                        Alias::new("release_status"),
                        Alias::new("official_rating"),
                        Alias::new("original_language"),
                        Alias::new("metadata_payload_version"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert!((row.try_get::<f64>("", "community_rating").unwrap() - 7.4).abs() < f64::EPSILON);
    assert_eq!(row.try_get::<i64>("", "vote_count").unwrap(), 1250);
    assert_eq!(
        row.try_get::<i64>("", "runtime_ticks").unwrap(),
        60_000_000_000
    );
    assert_eq!(
        row.try_get::<String>("", "release_status").unwrap(),
        "Released"
    );
    assert_eq!(row.try_get::<String>("", "official_rating").unwrap(), "PG");
    assert_eq!(
        row.try_get::<String>("", "original_language").unwrap(),
        "en"
    );
    assert_eq!(
        row.try_get::<i32>("", "metadata_payload_version").unwrap(),
        1
    );
    assert_eq!(count(&database, "item_genres").await, 2);
    assert_eq!(count(&database, "item_studios").await, 1);
    assert_eq!(count(&database, "item_people").await, 1);
    assert_eq!(count(&database, "item_countries").await, 1);
    assert_eq!(count(&database, "item_languages").await, 1);
    assert_eq!(generation(&database).await, 1);
}

#[tokio::test]
async fn concurrent_direct_publications_serialize_and_preserve_provider_identities() {
    let database = database().await;
    let item = seed_item(&database).await;
    let first_document = NfoDocument::parse(
        br#"<movie><title>Arrival</title><uniqueid type="tmdb">329865</uniqueid></movie>"#,
        "admin:first.nfo",
    )
    .unwrap();
    let second_document = NfoDocument::parse(
        br#"<movie><title>Arrival</title><uniqueid type="imdb">tt2543164</uniqueid></movie>"#,
        "admin:second.nfo",
    )
    .unwrap();
    let lookup = MetadataLookup::new(first_document.kind(), "Folder Name", None).unwrap();
    let first =
        MetadataResolution::from_candidate(&lookup, first_document.into_candidate()).unwrap();
    let second =
        MetadataResolution::from_candidate(&lookup, second_document.into_candidate()).unwrap();
    let first_database = database.clone();
    let second_database = database.clone();

    let (first_result, second_result) = tokio::join!(
        async move {
            MetadataPublicationRepository::new(&first_database)
                .publish(item, &first)
                .await
        },
        async move {
            MetadataPublicationRepository::new(&second_database)
                .publish(item, &second)
                .await
        }
    );

    assert!(first_result.unwrap().changed());
    assert!(second_result.unwrap().changed());
    let backend = database.get_database_backend();
    let providers = database
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("provider"))
                    .from(Alias::new("provider_ids"))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item.as_uuid()))
                    .order_by(Alias::new("provider"), Order::Asc),
            ),
        )
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.try_get::<String>("", "provider").unwrap())
        .collect::<Vec<_>>();
    assert_eq!(providers, ["imdb", "tmdb"]);
    let revision = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("metadata_revision"),
                        Alias::new("metadata_resolved_revision"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revision.try_get::<i64>("", "metadata_revision").unwrap(), 2);
    assert_eq!(
        revision
            .try_get::<i64>("", "metadata_resolved_revision")
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn partial_nfo_preserves_unmentioned_sql_fields_and_provider_identities() {
    let database = database().await;
    let item = seed_item(&database).await;
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("overview"), "Existing SQL overview")
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("provider_ids"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("catalog_item_id"),
                        Alias::new("provider"),
                        Alias::new("provider_item_id"),
                    ])
                    .values_panic([
                        uuid::Uuid::new_v4().into(),
                        item.as_uuid().into(),
                        "imdb".into(),
                        "tt2543164".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let document = NfoDocument::parse(
        br#"<movie><title>Arrival</title><year>2016</year><uniqueid type="tmdb">329865</uniqueid></movie>"#,
        "Arrival/movie.nfo",
    )
    .unwrap();
    let lookup = MetadataLookup::new(document.kind(), "Folder Name", None).unwrap();
    let resolution =
        MetadataResolution::from_candidate(&lookup, document.into_candidate()).unwrap();

    MetadataPublicationRepository::new(&database)
        .publish(item, &resolution)
        .await
        .unwrap();

    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("overview"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<String>("", "overview").unwrap(),
        "Existing SQL overview"
    );
    let identities = database
        .query_all(
            backend.build(
                Query::select()
                    .columns([Alias::new("provider"), Alias::new("provider_item_id")])
                    .from(Alias::new("provider_ids"))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item.as_uuid()))
                    .order_by(Alias::new("provider"), Order::Asc),
            ),
        )
        .await
        .unwrap();
    assert_eq!(identities.len(), 2);
    assert_eq!(
        identities[0].try_get::<String>("", "provider").unwrap(),
        "imdb"
    );
    assert_eq!(
        identities[1].try_get::<String>("", "provider").unwrap(),
        "tmdb"
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

async fn generation(database: &DatabaseConnection) -> i64 {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("generation"))
                    .from(Alias::new("catalog_state"))
                    .and_where(Expr::col(Alias::new("id")).eq(1_i32)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "generation")
        .unwrap()
}
