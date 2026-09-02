use chrono::Duration;
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{
    CatalogItemId, MediaLocationId, MediaSourceId, PresentationKey, SortKey, StorageObjectRecordId,
    StorageRootId, SubtitleId,
};
use tjxy_db::{
    CatalogPublicationError, CatalogPublicationRepository, ManualProbeError, ManualProbeRepository,
    MediaLocationPublicationRow, MediaSourcePublicationRow, MetadataRequirement, ProbeRepository,
    SeriesSourcePublication, SourcePlaybackPolicy, SourcePublicationManifest,
    StructurePublicationManifest, StructurePublicationRow, SubtitlePublicationRow,
    WorkJobRepository, WorkJobRepositoryError, WorkJobSpec, WorkJobState, WorkScope, WorkTaskKind,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_movie(database: &DatabaseConnection, revision: i64) -> CatalogItemId {
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
                        "Movie".into(),
                        "The Movie".into(),
                        "the movie".into(),
                        "Matched".into(),
                        "Ready".into(),
                        "NotApplicable".into(),
                        "Unknown".into(),
                        0.into(),
                        revision.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    id
}

async fn seed_storage_object(database: &DatabaseConnection, suffix: &str) -> StorageObjectRecordId {
    let account = Uuid::new_v4();
    let object = StorageObjectRecordId::new();
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
                        suffix.into(),
                        format!("account-{suffix}").into(),
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
                        Alias::new("size"),
                        Alias::new("remote_revision"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("identity_quality"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        object.as_uuid().into(),
                        account.into(),
                        "drive".into(),
                        format!("object-{suffix}").into(),
                        format!("{suffix}.bin").into(),
                        format!("{suffix}.bin").into(),
                        "File".into(),
                        10_i64.into(),
                        "rev-1".into(),
                        1_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "ProviderStable".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    object
}

#[allow(clippy::too_many_lines)] // Keeps the complete library/root/object authorization graph visible.
async fn authorize_storage_object(
    database: &DatabaseConnection,
    owner: CatalogItemId,
    object: StorageObjectRecordId,
) -> StorageRootId {
    let backend = database.get_database_backend();
    let account: Uuid = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("storage_account_id"))
                    .from(Alias::new("storage_objects"))
                    .and_where(Expr::col(Alias::new("id")).eq(object.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "storage_account_id")
        .unwrap();
    let library = Uuid::new_v4();
    let root = Uuid::new_v4();
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
                        "Authorized".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text("Authorized").into_bytes().into(),
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
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                    ])
                    .values_panic([
                        root.into(),
                        account.into(),
                        format!("root-{root}").into(),
                        1_i64.into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    for (table, columns, values) in [
        (
            "library_catalog_items",
            ["id", "library_id", "catalog_item_id"],
            [Uuid::new_v4(), library, owner.as_uuid()],
        ),
        (
            "library_storage_roots",
            ["id", "library_id", "storage_root_id"],
            [Uuid::new_v4(), library, root],
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
                        Uuid::new_v4().into(),
                        root.into(),
                        object.as_uuid().into(),
                        1_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    StorageRootId::from_uuid(root)
}

async fn attach_storage_object_to_root(
    database: &DatabaseConnection,
    root: StorageRootId,
    object: StorageObjectRecordId,
    parent: Option<StorageObjectRecordId>,
) {
    let backend = database.get_database_backend();
    let account: Uuid = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("storage_account_id"))
                    .from(Alias::new("storage_roots"))
                    .and_where(Expr::col(Alias::new("id")).eq(root.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "storage_account_id")
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("storage_account_id"), account)
                    .and_where(Expr::col(Alias::new("id")).eq(object.as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_root_objects"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("storage_object_id"),
                        Alias::new("parent_storage_object_id"),
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        root.as_uuid().into(),
                        object.as_uuid().into(),
                        parent.map(StorageObjectRecordId::as_uuid).into(),
                        1_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

async fn claimed_source_index(
    database: &DatabaseConnection,
    owner: CatalogItemId,
    revision: i64,
) -> (WorkJobRepository<'_>, tjxy_db::ClaimedWorkJob) {
    claimed_source_index_with_affinity(database, owner, revision, None).await
}

async fn claimed_source_index_with_affinity(
    database: &DatabaseConnection,
    owner: CatalogItemId,
    revision: i64,
    root_id: Option<StorageRootId>,
) -> (WorkJobRepository<'_>, tjxy_db::ClaimedWorkJob) {
    let jobs = WorkJobRepository::new(database);
    let mut spec = WorkJobSpec::new(
        WorkTaskKind::IndexMediaSources,
        WorkScope::CatalogItem(owner),
        revision,
        100,
    )
    .unwrap()
    .with_input_sync_revision(1)
    .unwrap();
    if let Some(root_id) = root_id {
        spec = spec.with_storage_root_affinity(root_id).unwrap();
    }
    jobs.enqueue_or_join(&spec).await.unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "source-publisher",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    (jobs, claimed)
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
            "series-publisher",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    (jobs, claimed)
}

struct SourceFixture {
    sources: Vec<MediaSourcePublicationRow>,
    locations: Vec<MediaLocationPublicationRow>,
    subtitles: Vec<SubtitlePublicationRow>,
}

fn source_fixture(video: StorageObjectRecordId, subtitle: StorageObjectRecordId) -> SourceFixture {
    let source = MediaSourceId::new();
    SourceFixture {
        sources: vec![
            MediaSourcePublicationRow::new(
                source,
                PresentationKey::new(),
                Some("Director's Cut".to_owned()),
                Some("mkv".to_owned()),
            )
            .unwrap(),
        ],
        locations: vec![
            MediaLocationPublicationRow::new(
                MediaLocationId::new(),
                source,
                video,
                Some("content-1".to_owned()),
                Some("provider_checksum".to_owned()),
                10,
            )
            .unwrap(),
        ],
        subtitles: vec![
            SubtitlePublicationRow::new(
                SubtitleId::new(),
                source,
                subtitle,
                "srt",
                Some("eng".to_owned()),
                None,
                true,
                false,
            )
            .unwrap(),
        ],
    }
}

async fn publish_source_fixture(
    database: &DatabaseConnection,
    owner: CatalogItemId,
    revision: i64,
    fixture: &SourceFixture,
) {
    let (jobs, claimed) = claimed_source_index(database, owner, revision).await;
    let manifest = SourcePublicationManifest::from_rows(
        &fixture.sources,
        &fixture.locations,
        &fixture.subtitles,
    )
    .unwrap();
    let publications = CatalogPublicationRepository::new(database);
    let publication = publications
        .begin_sources(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(
            &claimed,
            publication,
            &fixture.sources,
            &fixture.locations,
            &fixture.subtitles,
        )
        .await
        .unwrap();
    publications
        .seal_sources(&claimed, publication)
        .await
        .unwrap();
    publications
        .publish_sources(&jobs, &claimed, publication)
        .await
        .unwrap();
}

#[tokio::test]
async fn source_playback_policy_is_generationed_and_hides_all_playback_routes() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let video = seed_storage_object(&database, "policy-video").await;
    let subtitle = seed_storage_object(&database, "policy-subtitle").await;
    authorize_storage_object(&database, owner, video).await;
    authorize_storage_object(&database, owner, subtitle).await;
    let fixture = source_fixture(video, subtitle);
    publish_source_fixture(&database, owner, 1, &fixture).await;
    let presentation = fixture.sources[0].presentation_key();
    let publications = CatalogPublicationRepository::new(&database);

    publications
        .set_source_playback_policy(
            owner,
            presentation,
            SourcePlaybackPolicy::new(40, true, false),
        )
        .await
        .unwrap();
    let active = publications.active_sources(owner).await.unwrap();
    assert_eq!(active[0].admin_priority(), 40);
    assert!(active[0].is_default());
    assert!(!active[0].is_hidden());

    publications
        .set_source_playback_policy(
            owner,
            presentation,
            SourcePlaybackPolicy::new(40, false, true),
        )
        .await
        .unwrap();
    assert!(publications.active_sources(owner).await.unwrap()[0].is_hidden());
    assert!(
        publications
            .playback_location(owner, presentation)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        publications
            .subtitle_location(owner, presentation, 0)
            .await
            .unwrap()
            .is_none()
    );
    let generation: i64 = database
        .query_one(
            database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("generation"))
                    .from(Alias::new("catalog_state")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "generation")
        .unwrap();
    assert_eq!(generation, 3);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the publication root and its invalid ancestor path explicit.
async fn playback_fails_closed_when_the_publication_root_ancestor_is_unavailable() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let video = seed_storage_object(&database, "ancestor-video").await;
    let root = authorize_storage_object(&database, owner, video).await;
    let backend = database.get_database_backend();
    let account_id: Uuid = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("storage_account_id"))
                    .from(Alias::new("storage_objects"))
                    .and_where(Expr::col(Alias::new("id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "storage_account_id")
        .unwrap();
    let directory = StorageObjectRecordId::new();
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
                        directory.as_uuid().into(),
                        account_id.into(),
                        "drive".into(),
                        "ancestor-directory".into(),
                        "Ancestor".into(),
                        "ancestor".into(),
                        "Directory".into(),
                        1_i64.into(),
                        true.into(),
                        1_i64.into(),
                        "ProviderStable".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    attach_storage_object_to_root(&database, root, directory, None).await;
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("parent_storage_object_id"), directory.as_uuid())
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let source = MediaSourcePublicationRow::new(
        MediaSourceId::new(),
        PresentationKey::new(),
        None,
        Some("mkv".to_owned()),
    )
    .unwrap();
    let source_id = source.id();
    let presentation_key = source.presentation_key();
    let fixture = SourceFixture {
        locations: vec![
            MediaLocationPublicationRow::new(
                MediaLocationId::new(),
                source.id(),
                video,
                None,
                None,
                10,
            )
            .unwrap(),
        ],
        sources: vec![source],
        subtitles: vec![
            SubtitlePublicationRow::new(
                SubtitleId::new(),
                source_id,
                video,
                "srt",
                Some("eng".to_owned()),
                None,
                true,
                false,
            )
            .unwrap(),
        ],
    };
    let (jobs, claimed) = claimed_source_index_with_affinity(&database, owner, 1, Some(root)).await;
    let manifest = SourcePublicationManifest::from_rows(
        &fixture.sources,
        &fixture.locations,
        &fixture.subtitles,
    )
    .unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_sources(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(
            &claimed,
            publication,
            &fixture.sources,
            &fixture.locations,
            &fixture.subtitles,
        )
        .await
        .unwrap();
    publications
        .seal_sources(&claimed, publication)
        .await
        .unwrap();
    publications
        .publish_sources(&jobs, &claimed, publication)
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("subtitles"))
                    .value(Alias::new("delivery_index"), 0)
                    .and_where(Expr::col(Alias::new("media_source_id")).eq(source_id.as_uuid())),
            ),
        )
        .await
        .unwrap();
    assert!(
        publications
            .playback_location(owner, presentation_key)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        publications
            .subtitle_location(owner, presentation_key, 0)
            .await
            .unwrap()
            .is_some()
    );

    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("presence_state"), "TemporarilyUnavailable")
                    .value(
                        Alias::new("availability_reason"),
                        "moved-to-unmaterialized-parent",
                    )
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(directory.as_uuid())),
            ),
        )
        .await
        .unwrap();

    assert!(
        publications
            .playback_location(owner, presentation_key)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        publications
            .subtitle_location(owner, presentation_key, 0)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn manual_probe_rejects_a_candidate_overflow_without_enqueuing_partial_work() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let first_object = seed_storage_object(&database, "probe-first").await;
    let second_object = seed_storage_object(&database, "probe-second").await;
    authorize_storage_object(&database, owner, first_object).await;
    authorize_storage_object(&database, owner, second_object).await;
    let first_source = MediaSourceId::new();
    let second_source = MediaSourceId::new();
    let fixture = SourceFixture {
        sources: vec![
            MediaSourcePublicationRow::new(
                first_source,
                PresentationKey::from_uuid(Uuid::from_u128(1)),
                None,
                Some("mkv".to_owned()),
            )
            .unwrap(),
            MediaSourcePublicationRow::new(
                second_source,
                PresentationKey::from_uuid(Uuid::from_u128(2)),
                None,
                Some("mp4".to_owned()),
            )
            .unwrap(),
        ],
        locations: vec![
            MediaLocationPublicationRow::new(
                MediaLocationId::new(),
                first_source,
                first_object,
                None,
                None,
                10,
            )
            .unwrap(),
            MediaLocationPublicationRow::new(
                MediaLocationId::new(),
                second_source,
                second_object,
                None,
                None,
                10,
            )
            .unwrap(),
        ],
        subtitles: Vec::new(),
    };
    publish_source_fixture(&database, owner, 1, &fixture).await;

    let error = ManualProbeRepository::new(&database)
        .enqueue_item(owner, 100, 1)
        .await
        .unwrap_err();

    assert!(matches!(error, ManualProbeError::TooManyMediaSources));
    assert_eq!(probe_job_count(&database).await, 0);

    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("media_sources"))
                    .value(Alias::new("probe_revision"), -1_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(second_source.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let error = ManualProbeRepository::new(&database)
        .enqueue_item(owner, 100, 2)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ManualProbeError::Work(WorkJobRepositoryError::InvalidRevision)
    ));
    assert_eq!(probe_job_count(&database).await, 0);
}

#[tokio::test]
async fn manual_probe_rejects_active_sources_when_every_location_is_unavailable() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let video = seed_storage_object(&database, "probe-unavailable").await;
    authorize_storage_object(&database, owner, video).await;
    let source = MediaSourceId::new();
    let fixture = SourceFixture {
        sources: vec![
            MediaSourcePublicationRow::new(
                source,
                PresentationKey::new(),
                None,
                Some("mkv".to_owned()),
            )
            .unwrap(),
        ],
        locations: vec![
            MediaLocationPublicationRow::new(MediaLocationId::new(), source, video, None, None, 10)
                .unwrap(),
        ],
        subtitles: Vec::new(),
    };
    publish_source_fixture(&database, owner, 1, &fixture).await;
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("media_locations"))
                    .value(Alias::new("availability_state"), "TemporarilyUnavailable")
                    .and_where(Expr::col(Alias::new("media_source_id")).eq(source.as_uuid())),
            ),
        )
        .await
        .unwrap();

    let error = ManualProbeRepository::new(&database)
        .enqueue_item(owner, 100, 256)
        .await
        .unwrap_err();

    assert!(matches!(error, ManualProbeError::NoAvailableMediaSources));
    assert_eq!(probe_job_count(&database).await, 0);
}

#[tokio::test]
async fn manual_probe_rejects_a_source_beneath_an_unavailable_ancestor() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let video = seed_storage_object(&database, "probe-ancestor-video").await;
    let root = authorize_storage_object(&database, owner, video).await;
    let directory = seed_storage_object(&database, "probe-ancestor-directory").await;
    let backend = database.get_database_backend();
    let account: Uuid = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("storage_account_id"))
                    .from(Alias::new("storage_objects"))
                    .and_where(Expr::col(Alias::new("id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "storage_account_id")
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("storage_account_id"), account)
                    .value(Alias::new("object_type"), "Directory")
                    .and_where(Expr::col(Alias::new("id")).eq(directory.as_uuid())),
            ),
        )
        .await
        .unwrap();
    attach_storage_object_to_root(&database, root, directory, None).await;
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("parent_storage_object_id"), directory.as_uuid())
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let source = MediaSourcePublicationRow::new(
        MediaSourceId::new(),
        PresentationKey::new(),
        None,
        Some("mkv".to_owned()),
    )
    .unwrap();
    let fixture = SourceFixture {
        locations: vec![
            MediaLocationPublicationRow::new(
                MediaLocationId::new(),
                source.id(),
                video,
                None,
                None,
                10,
            )
            .unwrap(),
        ],
        sources: vec![source],
        subtitles: Vec::new(),
    };
    publish_source_fixture(&database, owner, 1, &fixture).await;
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("presence_state"), "TemporarilyUnavailable")
                    .value(
                        Alias::new("availability_reason"),
                        "moved-to-unmaterialized-parent",
                    )
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(directory.as_uuid())),
            ),
        )
        .await
        .unwrap();

    let error = ManualProbeRepository::new(&database)
        .enqueue_item(owner, 100, 256)
        .await
        .unwrap_err();

    assert!(matches!(error, ManualProbeError::NoAvailableMediaSources));
    assert_eq!(probe_job_count(&database).await, 0);
}

#[tokio::test]
async fn claimed_probe_candidate_disappears_after_library_root_revocation() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let video = seed_storage_object(&database, "probe-revoked-root").await;
    let root = authorize_storage_object(&database, owner, video).await;
    let source = MediaSourcePublicationRow::new(
        MediaSourceId::new(),
        PresentationKey::new(),
        None,
        Some("mkv".to_owned()),
    )
    .unwrap();
    let fixture = SourceFixture {
        locations: vec![
            MediaLocationPublicationRow::new(
                MediaLocationId::new(),
                source.id(),
                video,
                None,
                None,
                10,
            )
            .unwrap(),
        ],
        sources: vec![source],
        subtitles: Vec::new(),
    };
    publish_source_fixture(&database, owner, 1, &fixture).await;
    let submissions = ManualProbeRepository::new(&database)
        .enqueue_item(owner, 100, 256)
        .await
        .unwrap();
    assert_eq!(submissions.len(), 1);
    let jobs = WorkJobRepository::new(&database);
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ProbeMedia],
            "probe-worker",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        ProbeRepository::new(&database)
            .candidate(&claimed)
            .await
            .unwrap()
            .is_some()
    );

    database
        .execute(
            database.get_database_backend().build(
                Query::delete()
                    .from_table(Alias::new("library_storage_roots"))
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid())),
            ),
        )
        .await
        .unwrap();

    assert!(
        ProbeRepository::new(&database)
            .candidate(&claimed)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn source_publication_respects_metadata_none_without_advancing_or_enqueuing() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let video = seed_storage_object(&database, "metadata-none").await;
    authorize_storage_object(&database, owner, video).await;
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("metadata_policy"), "none"),
            ),
        )
        .await
        .unwrap();
    let source = MediaSourceId::new();
    let fixture = SourceFixture {
        sources: vec![
            MediaSourcePublicationRow::new(
                source,
                PresentationKey::new(),
                None,
                Some("mkv".to_owned()),
            )
            .unwrap(),
        ],
        locations: vec![
            MediaLocationPublicationRow::new(MediaLocationId::new(), source, video, None, None, 10)
                .unwrap(),
        ],
        subtitles: Vec::new(),
    };

    publish_source_fixture(&database, owner, 1, &fixture).await;

    let revision: i64 = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("metadata_revision"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "metadata_revision")
        .unwrap();
    assert_eq!(revision, 0);
    assert!(
        WorkJobRepository::new(&database)
            .claim_next(
                &[WorkTaskKind::ResolveMetadata],
                "unexpected-metadata",
                Duration::minutes(5),
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps source, location, subtitle, and publish atomicity together.
async fn source_projection_becomes_visible_only_after_atomic_publish() {
    let database = database().await;
    let owner = seed_movie(&database, 3).await;
    let video = seed_storage_object(&database, "video").await;
    let fallback_video = seed_storage_object(&database, "fallback-video").await;
    let subtitle = seed_storage_object(&database, "subtitle").await;
    let video_root = authorize_storage_object(&database, owner, video).await;
    attach_storage_object_to_root(&database, video_root, fallback_video, None).await;
    attach_storage_object_to_root(&database, video_root, subtitle, None).await;
    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("metadata_policy"), "full")
                    .value(Alias::new("metadata_source_mode"), "local_only")
                    .value(Alias::new("local_metadata_access_mode"), "direct"),
            ),
        )
        .await
        .unwrap();
    let (jobs, claimed) =
        claimed_source_index_with_affinity(&database, owner, 3, Some(video_root)).await;
    let mut fixture = source_fixture(video, subtitle);
    fixture.locations.push(
        MediaLocationPublicationRow::new(
            MediaLocationId::new(),
            fixture.sources[0].id(),
            fallback_video,
            Some("content-2".to_owned()),
            Some("provider_checksum".to_owned()),
            1,
        )
        .unwrap(),
    );
    let manifest = SourcePublicationManifest::from_rows(
        &fixture.sources,
        &fixture.locations,
        &fixture.subtitles,
    )
    .unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_sources(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(
            &claimed,
            publication,
            &fixture.sources,
            &fixture.locations,
            &fixture.subtitles,
        )
        .await
        .unwrap();

    assert!(publications.active_sources(owner).await.unwrap().is_empty());
    for table in ["media_sources", "media_locations", "subtitles"] {
        assert_eq!(
            count(&database, table).await,
            0,
            "stale publication materialized {table}"
        );
    }
    publications
        .seal_sources(&claimed, publication)
        .await
        .unwrap();
    let generation = publications
        .publish_sources(&jobs, &claimed, publication)
        .await
        .unwrap();

    assert_eq!(generation, 1);
    assert_eq!(count(&database, "cache_invalidation_outbox").await, 0);
    let active = publications.active_sources(owner).await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id(), fixture.sources[0].id());
    assert_eq!(
        active[0].presentation_key(),
        fixture.sources[0].presentation_key()
    );
    assert_eq!(active[0].probe_state(), "NotProbed");
    assert_eq!(active[0].locations().len(), 2);
    assert_eq!(active[0].locations()[0].storage_object_id(), video);
    assert_eq!(active[0].subtitles().len(), 1);
    assert_eq!(active[0].subtitles()[0].storage_object_id(), subtitle);
    assert_eq!(active[0].subtitles()[0].delivery_index(), None);
    let playback = publications
        .playback_location(owner, fixture.sources[0].presentation_key())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(playback.storage_object_id(), video);
    assert_eq!(playback.provider(), "filesystem");
    assert_eq!(playback.size(), 10);
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("media_locations"))
                    .value(Alias::new("availability_state"), "TemporarilyUnavailable")
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let active = publications.active_sources(owner).await.unwrap();
    assert_eq!(
        active[0].locations()[0].availability_state(),
        "TemporarilyUnavailable"
    );
    let healthy_fallback = publications
        .playback_location(owner, fixture.sources[0].presentation_key())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(healthy_fallback.storage_object_id(), fallback_video);
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("media_locations"))
                    .value(Alias::new("availability_state"), "TemporarilyUnavailable")
                    .and_where(
                        Expr::col(Alias::new("storage_object_id")).eq(fallback_video.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap();
    assert!(
        publications
            .playback_location(owner, fixture.sources[0].presentation_key())
            .await
            .unwrap()
            .is_some(),
        "a transient location remains retryable when no available copy exists"
    );
    assert!(
        publications
            .playback_location(owner, tjxy_common::PresentationKey::new())
            .await
            .unwrap()
            .is_none()
    );

    let backend = database.get_database_backend();
    let owner_row = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("active_source_publication_id"),
                        Alias::new("source_index_revision"),
                        Alias::new("metadata_revision"),
                        Alias::new("metadata_state"),
                        Alias::new("source_state"),
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
            .try_get::<Uuid>("", "active_source_publication_id")
            .unwrap(),
        publication.as_uuid()
    );
    assert_eq!(
        owner_row
            .try_get::<i64>("", "source_index_revision")
            .unwrap(),
        3
    );
    assert_eq!(
        owner_row.try_get::<String>("", "source_state").unwrap(),
        "Indexed"
    );
    assert_eq!(
        owner_row.try_get::<i64>("", "metadata_revision").unwrap(),
        1
    );
    assert_eq!(
        owner_row.try_get::<String>("", "metadata_state").unwrap(),
        "Resolving"
    );
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Completed
    );
    let metadata = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata-worker",
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata.job().scope(), WorkScope::CatalogItem(owner));
    assert_eq!(metadata.job().expected_revision(), 1);
    assert_eq!(metadata.job().input_sync_revision(), Some(1));
    assert_eq!(metadata.job().storage_root_affinity(), Some(video_root));
    assert_eq!(
        metadata.job().metadata_requirement(),
        Some(MetadataRequirement::Full)
    );
    assert_eq!(
        metadata.job().metadata_source_mode(),
        Some(tjxy_domain::MetadataSourceMode::LocalOnly)
    );
    assert_eq!(
        metadata.job().local_metadata_access_mode(),
        Some(tjxy_domain::LocalMetadataAccessMode::Direct)
    );
    let stream_count = count(&database, "media_streams").await;
    assert_eq!(stream_count, 0, "Source Indexing must not perform Probe");
}

#[tokio::test]
async fn active_sources_hide_a_publication_behind_a_newer_source_revision() {
    let database = database().await;
    let owner = seed_movie(&database, 3).await;
    let video = seed_storage_object(&database, "stale-video").await;
    let subtitle = seed_storage_object(&database, "stale-subtitle").await;
    authorize_storage_object(&database, owner, video).await;
    authorize_storage_object(&database, owner, subtitle).await;
    let fixture = source_fixture(video, subtitle);
    publish_source_fixture(&database, owner, 3, &fixture).await;
    assert_eq!(
        CatalogPublicationRepository::new(&database)
            .active_sources(owner)
            .await
            .unwrap()
            .len(),
        1
    );
    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("source_index_revision"), 4_i64)
                    .value(Alias::new("source_state"), "NotIndexed")
                    .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid())),
            ),
        )
        .await
        .unwrap();

    assert!(
        CatalogPublicationRepository::new(&database)
            .active_sources(owner)
            .await
            .unwrap()
            .is_empty(),
        "a publication at revision 3 must not satisfy canonical source revision 4"
    );
    let playable = CatalogPublicationRepository::new(&database)
        .playable_sources(owner)
        .await
        .unwrap();
    assert_eq!(
        playable.len(),
        1,
        "the last authorized publication should remain playable while revision 4 is built"
    );
    assert_eq!(playable[0].locations().len(), 1);
    database
        .execute(
            database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("libraries"))
                    .value(Alias::new("is_enabled"), false),
            ),
        )
        .await
        .unwrap();
    assert!(
        CatalogPublicationRepository::new(&database)
            .playable_sources(owner)
            .await
            .unwrap()
            .is_empty(),
        "last-known-good playback must fail closed when library access is revoked"
    );
}

#[tokio::test]
async fn source_seal_rejects_a_location_for_an_unpublished_source() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let video = seed_storage_object(&database, "orphan").await;
    authorize_storage_object(&database, owner, video).await;
    let (jobs, claimed) = claimed_source_index(&database, owner, 1).await;
    let orphan = MediaLocationPublicationRow::new(
        MediaLocationId::new(),
        MediaSourceId::new(),
        video,
        None,
        None,
        0,
    )
    .unwrap();
    let manifest =
        SourcePublicationManifest::from_rows(&[], std::slice::from_ref(&orphan), &[]).unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_sources(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(&claimed, publication, &[], &[orphan], &[])
        .await
        .unwrap();

    let error = publications
        .seal_sources(&claimed, publication)
        .await
        .unwrap_err();
    assert!(matches!(error, CatalogPublicationError::InvalidSourceGraph));
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
}

#[tokio::test]
async fn stale_source_revision_rolls_back_pointer_generation_and_completion() {
    let database = database().await;
    let owner = seed_movie(&database, 5).await;
    let video = seed_storage_object(&database, "stale-video").await;
    let subtitle = seed_storage_object(&database, "stale-subtitle").await;
    authorize_storage_object(&database, owner, video).await;
    authorize_storage_object(&database, owner, subtitle).await;
    let (jobs, claimed) = claimed_source_index(&database, owner, 5).await;
    let fixture = source_fixture(video, subtitle);
    let manifest = SourcePublicationManifest::from_rows(
        &fixture.sources,
        &fixture.locations,
        &fixture.subtitles,
    )
    .unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_sources(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(
            &claimed,
            publication,
            &fixture.sources,
            &fixture.locations,
            &fixture.subtitles,
        )
        .await
        .unwrap();
    publications
        .seal_sources(&claimed, publication)
        .await
        .unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("source_index_revision"), 6)
                    .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid())),
            ),
        )
        .await
        .unwrap();

    let error = publications
        .publish_sources(&jobs, &claimed, publication)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CatalogPublicationError::StaleExpectedRevision
    ));
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
    assert!(publications.active_sources(owner).await.unwrap().is_empty());
    let backend = database.get_database_backend();
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

async fn probe_job_count(database: &DatabaseConnection) -> i64 {
    database
        .query_one(
            database.get_database_backend().build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("work_jobs"))
                    .and_where(Expr::col(Alias::new("task_kind")).eq("ProbeMedia")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap()
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps both publications and the preserved Probe state together.
async fn reindex_preserves_stable_source_key_and_existing_probe_state() {
    let database = database().await;
    let owner = seed_movie(&database, 2).await;
    let video = seed_storage_object(&database, "stable-video").await;
    let subtitle = seed_storage_object(&database, "stable-subtitle").await;
    authorize_storage_object(&database, owner, video).await;
    authorize_storage_object(&database, owner, subtitle).await;
    let fixture = source_fixture(video, subtitle);
    let manifest = SourcePublicationManifest::from_rows(
        &fixture.sources,
        &fixture.locations,
        &fixture.subtitles,
    )
    .unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let (first_jobs, first_claim) = claimed_source_index(&database, owner, 2).await;
    let first = publications
        .begin_sources(&first_claim, &manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(
            &first_claim,
            first,
            &fixture.sources,
            &fixture.locations,
            &fixture.subtitles,
        )
        .await
        .unwrap();
    publications
        .seal_sources(&first_claim, first)
        .await
        .unwrap();
    publications
        .publish_sources(&first_jobs, &first_claim, first)
        .await
        .unwrap();

    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("media_sources"))
                    .value(Alias::new("probe_state"), "Probed")
                    .value(Alias::new("probe_revision"), 7_i64)
                    .value(Alias::new("container"), "matroska")
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.sources[0].id().as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("source_index_revision"), 3_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid())),
            ),
        )
        .await
        .unwrap();

    let (second_jobs, second_claim) = claimed_source_index(&database, owner, 3).await;
    let second = publications
        .begin_sources(&second_claim, &manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(
            &second_claim,
            second,
            &fixture.sources,
            &fixture.locations,
            &fixture.subtitles,
        )
        .await
        .unwrap();
    publications
        .seal_sources(&second_claim, second)
        .await
        .unwrap();
    publications
        .publish_sources(&second_jobs, &second_claim, second)
        .await
        .unwrap();

    let active = publications.active_sources(owner).await.unwrap();
    assert_eq!(active[0].id(), fixture.sources[0].id());
    assert_eq!(
        active[0].presentation_key(),
        fixture.sources[0].presentation_key()
    );
    assert_eq!(active[0].probe_state(), "Probed");
    assert_eq!(active[0].probe_revision(), 7);
    assert_eq!(active[0].container(), Some("matroska"));
    let first_state: String = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("state"))
                    .from(Alias::new("catalog_publications"))
                    .and_where(Expr::col(Alias::new("id")).eq(first.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "state")
        .unwrap();
    assert_eq!(first_state, "Retired");
}

#[tokio::test]
async fn source_seal_rejects_storage_outside_the_owners_libraries() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let unauthorized = seed_storage_object(&database, "foreign").await;
    let (jobs, claimed) = claimed_source_index(&database, owner, 1).await;
    let source = MediaSourcePublicationRow::new(
        MediaSourceId::new(),
        PresentationKey::new(),
        None,
        Some("mkv".to_owned()),
    )
    .unwrap();
    let location = MediaLocationPublicationRow::new(
        MediaLocationId::new(),
        source.id(),
        unauthorized,
        None,
        None,
        0,
    )
    .unwrap();
    let manifest = SourcePublicationManifest::from_rows(
        std::slice::from_ref(&source),
        std::slice::from_ref(&location),
        &[],
    )
    .unwrap();
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_sources(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(&claimed, publication, &[source], &[location], &[])
        .await
        .unwrap();

    let error = publications
        .seal_sources(&claimed, publication)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CatalogPublicationError::UnauthorizedStorageObject
    ));
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
}

#[tokio::test]
async fn source_publish_rechecks_authorization_after_seal() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let video = seed_storage_object(&database, "revoked-video").await;
    let subtitle = seed_storage_object(&database, "revoked-subtitle").await;
    authorize_storage_object(&database, owner, video).await;
    authorize_storage_object(&database, owner, subtitle).await;
    let fixture = source_fixture(video, subtitle);
    let manifest = SourcePublicationManifest::from_rows(
        &fixture.sources,
        &fixture.locations,
        &fixture.subtitles,
    )
    .unwrap();
    let (jobs, claimed) = claimed_source_index(&database, owner, 1).await;
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_sources(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(
            &claimed,
            publication,
            &fixture.sources,
            &fixture.locations,
            &fixture.subtitles,
        )
        .await
        .unwrap();
    publications
        .seal_sources(&claimed, publication)
        .await
        .unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                &Query::delete()
                    .from_table(Alias::new("library_storage_roots"))
                    .to_owned(),
            ),
        )
        .await
        .unwrap();

    let error = publications
        .publish_sources(&jobs, &claimed, publication)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CatalogPublicationError::UnauthorizedStorageObject
    ));
    assert!(publications.active_sources(owner).await.unwrap().is_empty());
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
}

#[tokio::test]
async fn source_publish_rechecks_storage_facts_after_seal() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let video = seed_storage_object(&database, "pending-video").await;
    let subtitle = seed_storage_object(&database, "pending-subtitle").await;
    let root = authorize_storage_object(&database, owner, video).await;
    authorize_storage_object(&database, owner, subtitle).await;
    let fixture = source_fixture(video, subtitle);
    let manifest = SourcePublicationManifest::from_rows(
        &fixture.sources,
        &fixture.locations,
        &fixture.subtitles,
    )
    .unwrap();
    let (jobs, claimed) = claimed_source_index(&database, owner, 1).await;
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_sources(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(
            &claimed,
            publication,
            &fixture.sources,
            &fixture.locations,
            &fixture.subtitles,
        )
        .await
        .unwrap();
    publications
        .seal_sources(&claimed, publication)
        .await
        .unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .value(Alias::new("sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(root.as_uuid())),
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
                    .value(Alias::new("facts_observed_storage_root_id"), root.as_uuid())
                    .and_where(Expr::col(Alias::new("id")).eq(video.as_uuid())),
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
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap();

    let error = publications
        .publish_sources(&jobs, &claimed, publication)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CatalogPublicationError::StorageInputPending
    ));
    assert!(publications.active_sources(owner).await.unwrap().is_empty());
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
}

#[tokio::test]
async fn source_publish_requires_authorization_and_reconciliation_on_the_same_root() {
    let database = database().await;
    let owner = seed_movie(&database, 1).await;
    let other_owner = seed_movie(&database, 0).await;
    let video = seed_storage_object(&database, "cross-root-video").await;
    let subtitle = seed_storage_object(&database, "cross-root-subtitle").await;
    let authorized_root = authorize_storage_object(&database, owner, video).await;
    let other_root = authorize_storage_object(&database, other_owner, video).await;
    authorize_storage_object(&database, owner, subtitle).await;
    let fixture = source_fixture(video, subtitle);
    let manifest = SourcePublicationManifest::from_rows(
        &fixture.sources,
        &fixture.locations,
        &fixture.subtitles,
    )
    .unwrap();
    let (jobs, claimed) = claimed_source_index(&database, owner, 1).await;
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_sources(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(
            &claimed,
            publication,
            &fixture.sources,
            &fixture.locations,
            &fixture.subtitles,
        )
        .await
        .unwrap();
    publications
        .seal_sources(&claimed, publication)
        .await
        .unwrap();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("observed_sync_revision"), 1_i64)
                    .value(
                        Alias::new("facts_observed_storage_root_id"),
                        other_root.as_uuid(),
                    )
                    .and_where(Expr::col(Alias::new("id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .value(Alias::new("sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(authorized_root.as_uuid())),
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
                    .and_where(
                        Expr::col(Alias::new("storage_root_id")).eq(authorized_root.as_uuid()),
                    )
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap();

    let error = publications
        .publish_sources(&jobs, &claimed, publication)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        CatalogPublicationError::StorageInputPending
    ));
    assert!(publications.active_sources(owner).await.unwrap().is_empty());
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Covers aggregate visibility, projected state, and pointer precedence together.
async fn series_structure_pointer_atomically_publishes_episode_sources() {
    let database = database().await;
    let owner = seed_movie(&database, 0).await;
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("item_type"), "Series")
                    .value(Alias::new("structure_state"), "Unexpanded")
                    .value(Alias::new("structure_expansion_revision"), 3_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(owner.as_uuid())),
            ),
        )
        .await
        .unwrap();
    let scope = seed_storage_object(&database, "series-scope").await;
    let video = seed_storage_object(&database, "series-video").await;
    let subtitle = seed_storage_object(&database, "series-subtitle").await;
    let storage_root = authorize_storage_object(&database, owner, scope).await;
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("object_type"), "Directory")
                    .and_where(Expr::col(Alias::new("id")).eq(scope.as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("children_indexed"), true)
                    .value(Alias::new("children_index_revision"), 1_i64)
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(storage_root.as_uuid()))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(scope.as_uuid())),
            ),
        )
        .await
        .unwrap();
    attach_storage_object_to_root(&database, storage_root, video, Some(scope)).await;
    attach_storage_object_to_root(&database, storage_root, subtitle, Some(scope)).await;
    let season = CatalogItemId::new();
    let episode = CatalogItemId::new();
    let rows = vec![
        StructurePublicationRow::new(
            season,
            owner,
            storage_root,
            scope,
            "Season",
            "Season 1",
            "season 1",
            None,
            None,
        )
        .unwrap(),
        StructurePublicationRow::new(
            episode,
            season,
            storage_root,
            scope,
            "Episode",
            "Episode 1",
            "episode 1",
            None,
            None,
        )
        .unwrap(),
    ];
    let fixture = source_fixture(video, subtitle);
    let aggregate_key = fixture.sources[0].presentation_key();
    let group = SeriesSourcePublication::new(
        episode,
        fixture.sources,
        fixture.locations,
        fixture.subtitles,
    )
    .unwrap();
    let manifest =
        StructurePublicationManifest::from_series(&rows, std::slice::from_ref(&group)).unwrap();
    let (jobs, claimed) = claimed_expand(&database, owner, 3).await;
    let publications = CatalogPublicationRepository::new(&database);
    let publication = publications
        .begin_structure(&claimed, &manifest)
        .await
        .unwrap();
    publications
        .stage_structure_batch(&claimed, publication, &rows)
        .await
        .unwrap();
    assert!(matches!(
        publications
            .seal_structure(&claimed, publication)
            .await
            .unwrap_err(),
        CatalogPublicationError::ManifestMismatch
    ));
    publications
        .stage_structure_source_batch(&claimed, publication, &[group])
        .await
        .unwrap();
    assert!(
        publications
            .active_sources(episode)
            .await
            .unwrap()
            .is_empty()
    );

    publications
        .seal_structure(&claimed, publication)
        .await
        .unwrap();
    publications
        .publish_structure(&jobs, &claimed, publication)
        .await
        .unwrap();

    assert_eq!(publications.active_sources(episode).await.unwrap().len(), 1);
    let episode_row = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("active_source_publication_id"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(episode.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        episode_row
            .try_get::<Option<Uuid>>("", "active_source_publication_id")
            .unwrap()
            .is_none()
    );
    let projected_episode = database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("source_state"),
                        Alias::new("source_index_revision"),
                    ])
                    .from(Alias::new("publication_catalog_items"))
                    .and_where(Expr::col(Alias::new("publication_id")).eq(publication.as_uuid()))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(episode.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        projected_episode
            .try_get::<String>("", "source_state")
            .unwrap(),
        "Indexed"
    );
    assert_eq!(
        projected_episode
            .try_get::<i64>("", "source_index_revision")
            .unwrap(),
        0
    );

    let older_video = seed_storage_object(&database, "older-direct-video").await;
    let older_subtitle = seed_storage_object(&database, "older-direct-subtitle").await;
    authorize_storage_object(&database, owner, older_video).await;
    authorize_storage_object(&database, owner, older_subtitle).await;
    let older = source_fixture(older_video, older_subtitle);
    let older_manifest =
        SourcePublicationManifest::from_rows(&older.sources, &older.locations, &older.subtitles)
            .unwrap();
    let (_source_jobs, source_claim) = claimed_source_index(&database, episode, 0).await;
    let older_publication = publications
        .begin_sources(&source_claim, &older_manifest)
        .await
        .unwrap();
    publications
        .stage_source_batch(
            &source_claim,
            older_publication,
            &older.sources,
            &older.locations,
            &older.subtitles,
        )
        .await
        .unwrap();
    publications
        .seal_sources(&source_claim, older_publication)
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_publications"))
                    .value(Alias::new("state"), "Active")
                    .value(Alias::new("activated_generation"), 0_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(older_publication.as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(
                        Alias::new("active_source_publication_id"),
                        older_publication.as_uuid(),
                    )
                    .and_where(Expr::col(Alias::new("id")).eq(episode.as_uuid())),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        publications.active_sources(episode).await.unwrap()[0].presentation_key(),
        aggregate_key
    );
}
