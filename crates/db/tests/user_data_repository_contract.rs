use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{CatalogItemId, UserId};
use tjxy_db::{UserDataPatch, UserDataRepository, UserDataRepositoryError};
use tjxy_test_support::test_database;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_user_and_item(database: &DatabaseConnection) -> (UserId, CatalogItemId) {
    let user_id = UserId::new();
    let item_id = CatalogItemId::new();
    let backend = database.get_database_backend();
    let insert_user = Query::insert()
        .into_table(Alias::new("users"))
        .columns([
            Alias::new("id"),
            Alias::new("username"),
            Alias::new("password_hash"),
            Alias::new("is_admin"),
        ])
        .values_panic([
            user_id.as_uuid().into(),
            "alice".into(),
            "test-only".into(),
            false.into(),
        ])
        .to_owned();
    database.execute(backend.build(&insert_user)).await.unwrap();

    let insert_item = Query::insert()
        .into_table(Alias::new("catalog_items"))
        .columns([
            Alias::new("id"),
            Alias::new("item_type"),
            Alias::new("name"),
            Alias::new("sort_name"),
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
            "Arrival".into(),
            "arrival".into(),
            "Matched".into(),
            "Ready".into(),
            "NotApplicable".into(),
            "Indexed".into(),
            0_i64.into(),
            0_i64.into(),
            true.into(),
        ])
        .to_owned();
    database.execute(backend.build(&insert_item)).await.unwrap();
    (user_id, item_id)
}

async fn attach_item_to_library(
    database: &DatabaseConnection,
    item_id: CatalogItemId,
    enabled: bool,
) -> uuid::Uuid {
    let library_id = uuid::Uuid::new_v4();
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
                        library_id.into(),
                        "Movies".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        Vec::<u8>::from("movies").into(),
                        enabled.into(),
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
                        uuid::Uuid::new_v4().into(),
                        library_id.into(),
                        item_id.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    library_id
}

#[tokio::test]
async fn first_partial_commit_creates_defaults_and_revision_one() {
    let database = database().await;
    let (user_id, item_id) = seed_user_and_item(&database).await;
    let repository = UserDataRepository::new(&database);

    let committed = repository
        .commit(user_id, item_id, UserDataPatch::favorite(true))
        .await
        .unwrap();

    assert_eq!(committed.user_revision, 1);
    assert!(committed.data.is_favorite);
    assert!(!committed.data.is_played);
    assert_eq!(committed.data.play_count, 0);
    assert_eq!(committed.data.playback_position_ticks, 0);
}

#[tokio::test]
async fn later_partial_commit_preserves_unmentioned_fields_and_bumps_once() {
    let database = database().await;
    let (user_id, item_id) = seed_user_and_item(&database).await;
    let repository = UserDataRepository::new(&database);
    repository
        .commit(user_id, item_id, UserDataPatch::favorite(true))
        .await
        .unwrap();

    let committed = repository
        .commit(
            user_id,
            item_id,
            UserDataPatch::default()
                .with_playback_position_ticks(900_000)
                .with_played(true)
                .with_play_count_delta(1),
        )
        .await
        .unwrap();

    assert_eq!(committed.user_revision, 2);
    assert!(committed.data.is_favorite);
    assert!(committed.data.is_played);
    assert_eq!(committed.data.play_count, 1);
    assert_eq!(committed.data.playback_position_ticks, 900_000);
}

#[tokio::test]
async fn invalid_item_rolls_back_user_revision() {
    let database = database().await;
    let (user_id, _) = seed_user_and_item(&database).await;
    let repository = UserDataRepository::new(&database);

    let error = repository
        .commit(user_id, CatalogItemId::new(), UserDataPatch::favorite(true))
        .await
        .unwrap_err();

    assert!(matches!(error, UserDataRepositoryError::Database(_)));
    assert_eq!(repository.revision(user_id).await.unwrap(), None);
}

#[tokio::test]
async fn empty_patch_is_rejected_without_incrementing_revision() {
    let database = database().await;
    let (user_id, item_id) = seed_user_and_item(&database).await;
    let repository = UserDataRepository::new(&database);

    let error = repository
        .commit(user_id, item_id, UserDataPatch::default())
        .await
        .unwrap_err();

    assert_eq!(error, UserDataRepositoryError::EmptyPatch);
    assert_eq!(repository.revision(user_id).await.unwrap(), None);
}

#[tokio::test]
async fn concurrent_field_patches_do_not_overwrite_each_other() {
    let database = database().await;
    let (user_id, item_id) = seed_user_and_item(&database).await;
    let first = UserDataRepository::new(&database);
    let second = UserDataRepository::new(&database);

    let (favorite, position) = tokio::join!(
        first.commit(user_id, item_id, UserDataPatch::favorite(true)),
        second.commit(
            user_id,
            item_id,
            UserDataPatch::default().with_playback_position_ticks(42_000),
        )
    );

    let favorite = favorite.unwrap();
    let position = position.unwrap();
    let mut revisions = [favorite.user_revision, position.user_revision];
    revisions.sort_unstable();
    assert_eq!(revisions, [1, 2]);
    let current = first.get(user_id, item_id).await.unwrap().unwrap();
    assert!(current.is_favorite);
    assert_eq!(current.playback_position_ticks, 42_000);
}

#[tokio::test]
async fn visible_commit_checks_enabled_library_inside_the_write_transaction() {
    let database = database().await;
    let (user_id, item_id) = seed_user_and_item(&database).await;
    let library_id = attach_item_to_library(&database, item_id, true).await;
    let repository = UserDataRepository::new(&database);

    let committed = repository
        .commit_visible(user_id, item_id, UserDataPatch::favorite(true))
        .await
        .unwrap();
    assert!(committed.is_some());
    assert_eq!(repository.revision(user_id).await.unwrap(), Some(1));

    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("is_enabled"), false)
                    .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(library_id)),
            ),
        )
        .await
        .unwrap();
    assert!(
        repository
            .commit_visible(user_id, item_id, UserDataPatch::favorite(false))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        repository
            .get_visible(user_id, item_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(repository.revision(user_id).await.unwrap(), Some(1));
}

#[tokio::test]
async fn repeated_visible_absolute_patch_does_not_bump_revision() {
    let database = database().await;
    let (user_id, item_id) = seed_user_and_item(&database).await;
    attach_item_to_library(&database, item_id, true).await;
    let repository = UserDataRepository::new(&database);

    repository
        .commit_visible(user_id, item_id, UserDataPatch::favorite(true))
        .await
        .unwrap();
    let replay = repository
        .commit_visible(user_id, item_id, UserDataPatch::favorite(true))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(replay.user_revision, 1);
    assert!(replay.data.is_favorite);
    assert_eq!(repository.revision(user_id).await.unwrap(), Some(1));
}
