use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_application::MetadataImportService;
use tjxy_common::CatalogItemId;
use tjxy_test_support::test_database;

async fn fixture() -> (DatabaseConnection, CatalogItemId) {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let item = CatalogItemId::new();
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
                        item.as_uuid().into(),
                        "Movie".into(),
                        "Fallback Title".into(),
                        "fallback title".into(),
                        b"fallback title".to_vec().into(),
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
    (database, item)
}

#[tokio::test]
async fn nfo_import_resolves_against_the_existing_item_and_publishes_sql_metadata() {
    let (database, item) = fixture().await;
    let service = MetadataImportService::new(database.clone());

    let report = service
        .import_nfo(
            item,
            br#"<movie><title>Arrival</title><year>2016</year><plot>A linguist meets visitors.</plot><uniqueid type="tmdb">329865</uniqueid></movie>"#,
            "admin:nfo",
        )
        .await
        .unwrap();

    assert!(report.changed());
    assert_eq!(report.state().as_str(), "Ready");
    let backend = database.get_database_backend();
    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("name"),
                        Alias::new("production_year"),
                        Alias::new("overview"),
                        Alias::new("metadata_state"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "name").unwrap(), "Arrival");
    assert_eq!(row.try_get::<i32>("", "production_year").unwrap(), 2016);
    assert_eq!(
        row.try_get::<String>("", "overview").unwrap(),
        "A linguist meets visitors."
    );
    assert_eq!(
        row.try_get::<String>("", "metadata_state").unwrap(),
        "Ready"
    );
}

#[tokio::test]
async fn unsafe_nfo_is_rejected_before_any_catalog_write() {
    let (database, item) = fixture().await;
    let service = MetadataImportService::new(database.clone());

    let result = service
        .import_nfo(
            item,
            br#"<!DOCTYPE movie [<!ENTITY x SYSTEM "file:///etc/passwd">]><movie><title>&x;</title></movie>"#,
            "admin:nfo",
        )
        .await;

    assert!(result.is_err());
    let backend = database.get_database_backend();
    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([Alias::new("name"), Alias::new("metadata_state")])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "name").unwrap(), "Fallback Title");
    assert_eq!(
        row.try_get::<String>("", "metadata_state").unwrap(),
        "Empty"
    );
}

#[tokio::test]
async fn nfo_document_kind_must_match_the_existing_catalog_item() {
    let (database, item) = fixture().await;
    let service = MetadataImportService::new(database.clone());

    let result = service
        .import_nfo(
            item,
            br"<tvshow><title>Not a Movie</title></tvshow>",
            "admin:nfo",
        )
        .await;

    assert!(matches!(
        result,
        Err(tjxy_application::MetadataImportError::NfoKindMismatch)
    ));
    let backend = database.get_database_backend();
    let row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([Alias::new("name"), Alias::new("metadata_state")])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "name").unwrap(), "Fallback Title");
    assert_eq!(
        row.try_get::<String>("", "metadata_state").unwrap(),
        "Empty"
    );
}
