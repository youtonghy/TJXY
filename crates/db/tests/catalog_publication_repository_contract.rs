use chrono::{Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{CatalogItemId, ImageType, StorageObjectRecordId, StorageRootId, Username};
use tjxy_db::{
    AuthRepository, BrowseParent, CatalogPageRequest, CatalogPublicationError,
    CatalogPublicationRepository, CatalogQueryRepository, StructurePublicationManifest,
    StructurePublicationRow, WorkJobRepository, WorkJobSpec, WorkJobState, WorkScope, WorkTaskKind,
};
use tjxy_test_support::test_database;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_series(database: &DatabaseConnection, revision: i64) -> CatalogItemId {
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
                        "Series".into(),
                        "The Series".into(),
                        "the series".into(),
                        "Matched".into(),
                        "Ready".into(),
                        "Unexpanded".into(),
                        "Unknown".into(),
                        revision.into(),
                        0.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    id
}

async fn seed_browse_scope(
    database: &DatabaseConnection,
    owner: CatalogItemId,
) -> tjxy_common::UserId {
    let user = AuthRepository::new(database)
        .create_user(
            &Username::parse("reader").unwrap(),
            "$argon2id$test",
            true,
            false,
            Utc::now(),
        )
        .await
        .unwrap()
        .id();
    let library = uuid::Uuid::new_v4();
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
                        "TV".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "tvshows".into(),
                        b"tv".to_vec().into(),
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
                        uuid::Uuid::new_v4().into(),
                        library.into(),
                        owner.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    user
}

async fn seed_primary_asset(database: &DatabaseConnection, item: CatalogItemId) {
    let blob = uuid::Uuid::new_v4();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("asset_blobs"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("sha256"),
                        Alias::new("mime_type"),
                        Alias::new("byte_size"),
                        Alias::new("local_relative_path"),
                    ])
                    .values_panic([
                        blob.into(),
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                        "image/jpeg".into(),
                        4_i64.into(),
                        "aa/primary.jpg".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("item_assets"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_id"),
                        Alias::new("asset_blob_id"),
                        Alias::new("image_type"),
                        Alias::new("priority"),
                        Alias::new("source_provider"),
                    ])
                    .values_panic([
                        uuid::Uuid::new_v4().into(),
                        item.as_uuid().into(),
                        blob.into(),
                        ImageType::Primary.as_str().into(),
                        0.into(),
                        "fixture".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

#[allow(clippy::too_many_lines)] // Seeds the complete owner/root authorization graph used by publication tests.
async fn structure_rows(
    database: &DatabaseConnection,
    owner: CatalogItemId,
) -> Vec<StructurePublicationRow> {
    let season = CatalogItemId::new();
    let storage_root = StorageRootId::new();
    let scope = StorageObjectRecordId::new();
    let account = uuid::Uuid::new_v4();
    let library = uuid::Uuid::new_v4();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_accounts"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("provider"),
                        Alias::new("display_name"),
                        Alias::new("account_identity"),
                        Alias::new("credential_ref"),
                        Alias::new("status"),
                    ])
                    .values_panic([
                        account.into(),
                        "filesystem".into(),
                        "Structure scope".into(),
                        account.to_string().into(),
                        "fixture".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_drive_id"),
                        Alias::new("provider_object_id"),
                        Alias::new("name"),
                        Alias::new("normalized_name"),
                        Alias::new("object_type"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("identity_quality"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        scope.as_uuid().into(),
                        account.into(),
                        "local".into(),
                        scope.to_string().into(),
                        "Season 1".into(),
                        "season 1".into(),
                        "Directory".into(),
                        1_i64.into(),
                        true.into(),
                        1_i64.into(),
                        "StableFileId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                    ])
                    .values_panic([
                        storage_root.as_uuid().into(),
                        account.into(),
                        storage_root.to_string().into(),
                        1_i64.into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
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
                        "Structure".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "tvshows".into(),
                        b"structure".to_vec().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    for (table, columns, values) in [
        (
            "library_catalog_items",
            ["id", "library_id", "catalog_item_id"],
            [uuid::Uuid::new_v4(), library, owner.as_uuid()],
        ),
        (
            "library_storage_roots",
            ["id", "library_id", "storage_root_id"],
            [uuid::Uuid::new_v4(), library, storage_root.as_uuid()],
        ),
    ] {
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new(table))
                        .columns(columns.map(Alias::new))
                        .values_panic(values.map(Into::into)),
                ),
            )
            .await
            .unwrap();
    }
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_root_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        uuid::Uuid::new_v4().into(),
                        storage_root.as_uuid().into(),
                        scope.as_uuid().into(),
                        1_i64.into(),
                        true.into(),
                        1_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    vec![
        StructurePublicationRow::new(
            season,
            owner,
            storage_root,
            scope,
            "Season",
            "Season 1",
            "season 1",
            Some(2026),
            None,
        )
        .unwrap(),
        StructurePublicationRow::new(
            CatalogItemId::new(),
            season,
            storage_root,
            scope,
            "Episode",
            "Episode 1",
            "episode 1",
            Some(2026),
            Some("Pilot".to_owned()),
        )
        .unwrap(),
    ]
}

async fn claimed_expand(
    database: &DatabaseConnection,
    owner: CatalogItemId,
    revision: i64,
) -> (WorkJobRepository<'_>, tjxy_db::ClaimedWorkJob) {
    let jobs = WorkJobRepository::new(database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(owner),
            revision,
            100,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ExpandItem],
            "publisher",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    (jobs, claimed)
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the before/seal/publish/nested-read contract together.
async fn structure_publication_is_invisible_until_one_atomic_pointer_switch() {
    let database = database().await;
    let owner = seed_series(&database, 4).await;
    let user = seed_browse_scope(&database, owner).await;
    let (jobs, claimed) = claimed_expand(&database, owner, 4).await;
    let rows = structure_rows(&database, owner).await;
    let manifest = StructurePublicationManifest::from_rows(&rows).unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_structure(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_structure_batch(&claimed, publication, &rows[..1])
        .await
        .unwrap();

    let browse = CatalogQueryRepository::new(&database);
    let before_page = browse
        .items(
            user,
            BrowseParent::Item(owner),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();
    assert!(before_page.items().is_empty());

    let backend = database.get_database_backend();
    let before = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("active_structure_publication_id"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        before
            .try_get::<Option<uuid::Uuid>>("", "active_structure_publication_id")
            .unwrap()
            .is_none()
    );

    publications
        .stage_structure_batch(&claimed, publication, &rows[1..])
        .await
        .unwrap();
    publications
        .seal_structure(&claimed, publication)
        .await
        .unwrap();
    let generation = publications
        .publish_structure(&jobs, &claimed, publication)
        .await
        .unwrap();

    assert_eq!(generation, 1);
    let invalidations = database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("cache_invalidation_outbox")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(invalidations.try_get::<i64>("", "count").unwrap(), 1);
    let season_page = browse
        .items(
            user,
            BrowseParent::Item(owner),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(season_page.items().len(), 1);
    assert_eq!(season_page.items()[0].name(), "Season 1");
    let season = season_page.items()[0].id();
    seed_primary_asset(&database, season).await;
    assert!(
        browse
            .image(season, ImageType::Primary, 0)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        browse.resolve_parent(season.as_uuid()).await.unwrap(),
        Some(BrowseParent::Item(season))
    );
    let episode_page = browse
        .items(
            user,
            BrowseParent::Item(season),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(episode_page.items().len(), 1);
    assert_eq!(episode_page.items()[0].name(), "Episode 1");
    let owner_row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("active_structure_publication_id"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("structure_state"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        owner_row
            .try_get::<uuid::Uuid>("", "active_structure_publication_id")
            .unwrap(),
        publication.as_uuid()
    );
    assert_eq!(
        owner_row
            .try_get::<i64>("", "structure_expansion_revision")
            .unwrap(),
        4
    );
    assert_eq!(
        owner_row.try_get::<String>("", "structure_state").unwrap(),
        "Expanded"
    );
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Completed
    );
    let event = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([Alias::new("generation"), Alias::new("publication_id")])
                    .from(Alias::new("catalog_change_outbox")),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.try_get::<i64>("", "generation").unwrap(), 1);
    assert_eq!(
        event.try_get::<uuid::Uuid>("", "publication_id").unwrap(),
        publication.as_uuid()
    );
}

#[tokio::test]
async fn stale_revision_rejects_the_whole_publish_transaction() {
    let database = database().await;
    let owner = seed_series(&database, 2).await;
    let (jobs, claimed) = claimed_expand(&database, owner, 2).await;
    let rows = structure_rows(&database, owner).await;
    let manifest = StructurePublicationManifest::from_rows(&rows).unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_structure(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_structure_batch(&claimed, publication, &rows)
        .await
        .unwrap();
    publications
        .seal_structure(&claimed, publication)
        .await
        .unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("structure_expansion_revision"), 3)
                    .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid())),
            ),
        )
        .await
        .unwrap();

    let error = publications
        .publish_structure(&jobs, &claimed, publication)
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            tjxy_db::CatalogPublicationError::StaleExpectedRevision
        ),
        "unexpected error: {error:?}"
    );
    let generation: i64 = database
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
        .unwrap();
    assert_eq!(generation, 0);
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
}

#[tokio::test]
async fn pending_structure_scope_rejects_the_whole_publish_transaction() {
    let database = database().await;
    let owner = seed_series(&database, 1).await;
    let (jobs, claimed) = claimed_expand(&database, owner, 1).await;
    let rows = structure_rows(&database, owner).await;
    let manifest = StructurePublicationManifest::from_rows(&rows).unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_structure(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_structure_batch(&claimed, publication, &rows)
        .await
        .unwrap();
    publications
        .seal_structure(&claimed, publication)
        .await
        .unwrap();
    let backend = database.get_database_backend();
    let scope = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("storage_root_id"),
                        Alias::new("storage_object_id"),
                    ])
                    .from(Alias::new("storage_root_objects"))
                    .limit(1),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let root: uuid::Uuid = scope.try_get("", "storage_root_id").unwrap();
    let object: uuid::Uuid = scope.try_get("", "storage_object_id").unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .value(Alias::new("sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(root)),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("observed_sync_revision"), 2_i64)
                    .value(Alias::new("facts_observed_storage_root_id"), root)
                    .and_where(Expr::col(Alias::new("id")).eq(object)),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("observed_sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(object)),
            ),
        )
        .await
        .unwrap();

    let error = publications
        .publish_structure(&jobs, &claimed, publication)
        .await
        .unwrap_err();

    assert!(
        matches!(error, CatalogPublicationError::StorageInputPending),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
}

#[tokio::test]
async fn absent_structure_scope_is_rejected_before_the_pointer_switch() {
    let database = database().await;
    let owner = seed_series(&database, 1).await;
    let (jobs, claimed) = claimed_expand(&database, owner, 1).await;
    let rows = structure_rows(&database, owner).await;
    let manifest = StructurePublicationManifest::from_rows(&rows).unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_structure(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_structure_batch(&claimed, publication, &rows)
        .await
        .unwrap();
    publications
        .seal_structure(&claimed, publication)
        .await
        .unwrap();
    let backend = database.get_database_backend();
    let scope = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("storage_root_id"),
                        Alias::new("storage_object_id"),
                    ])
                    .from(Alias::new("storage_root_objects"))
                    .limit(1),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let root: uuid::Uuid = scope.try_get("", "storage_root_id").unwrap();
    let object: uuid::Uuid = scope.try_get("", "storage_object_id").unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("presence_state"), "ConfirmedAbsent")
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(object)),
            ),
        )
        .await
        .unwrap();

    let error = publications
        .publish_structure(&jobs, &claimed, publication)
        .await
        .unwrap_err();

    assert!(
        matches!(error, CatalogPublicationError::UnauthorizedStorageObject),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
}

#[tokio::test]
async fn incomplete_manifest_cannot_be_published() {
    let database = database().await;
    let owner = seed_series(&database, 1).await;
    let (jobs, claimed) = claimed_expand(&database, owner, 1).await;
    let rows = structure_rows(&database, owner).await;
    let manifest = StructurePublicationManifest::from_rows(&rows).unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_structure(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_structure_batch(&claimed, publication, &rows[..1])
        .await
        .unwrap();

    let error = publications
        .seal_structure(&claimed, publication)
        .await
        .unwrap_err();
    assert!(
        matches!(error, tjxy_db::CatalogPublicationError::ManifestMismatch),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
}

#[tokio::test]
async fn structure_staging_rejects_reparenting_an_item_from_another_series() {
    let database = database().await;
    let first_owner = seed_series(&database, 1).await;
    let second_owner = seed_series(&database, 1).await;
    let (_first_jobs, first_claim) = claimed_expand(&database, first_owner, 1).await;
    let first_rows = structure_rows(&database, first_owner).await;
    let first_manifest = StructurePublicationManifest::from_rows(&first_rows).unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let first_publication = publications
        .begin_structure(&first_claim, &first_manifest)
        .await
        .unwrap();
    publications
        .stage_structure_batch(&first_claim, first_publication, &first_rows)
        .await
        .unwrap();

    let (_second_jobs, second_claim) = claimed_expand(&database, second_owner, 1).await;
    let stolen = StructurePublicationRow::new(
        first_rows[0].id(),
        second_owner,
        StorageRootId::new(),
        StorageObjectRecordId::new(),
        "Season",
        "Stolen season",
        "stolen season",
        None,
        None,
    )
    .unwrap();
    let manifest = StructurePublicationManifest::from_rows(std::slice::from_ref(&stolen)).unwrap();
    let second_publication = publications
        .begin_structure(&second_claim, &manifest)
        .await
        .unwrap();

    let error = publications
        .stage_structure_batch(&second_claim, second_publication, &[stolen])
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        tjxy_db::CatalogPublicationError::StableIdentityConflict
    ));
}

#[tokio::test]
async fn structure_publication_rejects_a_scope_outside_the_owner_roots() {
    let database = database().await;
    let owner = seed_series(&database, 1).await;
    let (jobs, claimed) = claimed_expand(&database, owner, 1).await;
    let rows = vec![
        StructurePublicationRow::new(
            CatalogItemId::new(),
            owner,
            StorageRootId::new(),
            StorageObjectRecordId::new(),
            "Season",
            "Season 1",
            "season 1",
            None,
            None,
        )
        .unwrap(),
    ];
    let manifest = StructurePublicationManifest::from_rows(&rows).unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_structure(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_structure_batch(&claimed, publication, &rows)
        .await
        .unwrap();
    publications
        .seal_structure(&claimed, publication)
        .await
        .unwrap();

    let error = publications
        .publish_structure(&jobs, &claimed, publication)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        CatalogPublicationError::UnauthorizedStorageObject
    ));
}
