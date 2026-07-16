use sea_orm::{
    ConnectionTrait, Database, DbBackend, Statement,
    sea_query::{Alias, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_application::{CatalogQueryService, CatalogServiceError};
use tjxy_common::{SortKey, UserId};
use tjxy_db::{BrowseParent, CatalogPageRequest};
use uuid::Uuid;

async fn service() -> CatalogQueryService {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    database
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await
        .unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
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
                        Uuid::new_v4().into(),
                        "Movies".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text("Movies").into_bytes().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    CatalogQueryService::new(database)
}

#[tokio::test]
async fn requested_user_must_match_the_authenticated_principal() {
    let service = service().await;
    let principal = UserId::new();
    let other = UserId::new();

    let views = service.user_views(principal, Some(other)).await;
    let items = service
        .items(
            principal,
            Some(other),
            BrowseParent::Library(Uuid::new_v4()),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await;

    assert!(matches!(views, Err(CatalogServiceError::ForbiddenUser)));
    assert!(matches!(items, Err(CatalogServiceError::ForbiddenUser)));
}

#[tokio::test]
async fn omitted_or_matching_user_reads_the_principals_catalog() {
    let service = service().await;
    let principal = UserId::new();

    let omitted = service.user_views(principal, None).await.unwrap();
    let matching = service
        .user_views(principal, Some(principal))
        .await
        .unwrap();

    assert_eq!(omitted, matching);
    assert_eq!(omitted.len(), 1);
    assert_eq!(omitted[0].name(), "Movies");
}

#[tokio::test]
async fn unknown_parent_is_distinct_from_a_known_empty_parent() {
    let service = service().await;
    let principal = UserId::new();

    let page = service
        .items_by_parent_id(
            principal,
            None,
            Uuid::new_v4(),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();

    assert!(page.is_none());
}
