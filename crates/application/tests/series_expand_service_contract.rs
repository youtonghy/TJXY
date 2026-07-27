use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_application::{SeriesExpandService, SourceIndexService, TaskService};
use tjxy_common::{CatalogItemId, SortKey, StorageObjectRecordId, StorageRootId, UserId};
use tjxy_db::{
    BrowseParent, CatalogPageRequest, CatalogPublicationRepository, CatalogQueryRepository,
    ManualProbeRepository, MetadataRequirement, MetadataWorkRepository, OutboxRepository,
    ProbeRepository, SeriesExpandRepository, SeriesExpandRepositoryError, SourceIndexRepository,
    SourceIndexRepositoryError, StorageChangeProjectionRepository, WorkJobRepository, WorkJobSpec,
    WorkJobState, WorkScope, WorkTaskKind,
};
use tjxy_metadata::{MetadataCandidate, MetadataResolution, MetadataSource};
use tjxy_test_support::test_database;
use uuid::Uuid;

struct Fixture {
    database: DatabaseConnection,
    series: CatalogItemId,
    account: Uuid,
    root: StorageRootId,
    season: StorageObjectRecordId,
    season_nfo: StorageObjectRecordId,
    episode_nfos: Vec<StorageObjectRecordId>,
    video: StorageObjectRecordId,
    subtitle: StorageObjectRecordId,
}

#[allow(clippy::too_many_lines)] // Mirrors one complete reconciled Series storage tree.
async fn fixture(
    season_indexed: bool,
    nested_episode: bool,
    additional_flat_episode: bool,
) -> Fixture {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let sql = database.get_database_backend();
    let library = Uuid::new_v4();
    let series = CatalogItemId::new();
    let account = Uuid::new_v4();
    let root = StorageRootId::new();
    let title = StorageObjectRecordId::new();
    let season = StorageObjectRecordId::new();
    let season_nfo = StorageObjectRecordId::new();
    let episode_directory = StorageObjectRecordId::new();
    let video = StorageObjectRecordId::new();
    let subtitle = StorageObjectRecordId::new();
    let episode_nfos = if additional_flat_episode {
        vec![StorageObjectRecordId::new(), StorageObjectRecordId::new()]
    } else {
        Vec::new()
    };
    database
        .execute(
            sql.build(
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
                        SortKey::from_text("TV").into_bytes().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        series.as_uuid().into(),
                        "Series".into(),
                        "Dark".into(),
                        "dark".into(),
                        SortKey::from_text("Dark").into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        "Unexpanded".into(),
                        "Unknown".into(),
                        1_i64.into(),
                        0_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        library.into(),
                        series.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        "Local".into(),
                        "local-account".into(),
                        "local".into(),
                        "Active".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
                        root.as_uuid().into(),
                        account.into(),
                        "root".into(),
                        1_i64.into(),
                        1_i64.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("library_storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("storage_root_id"),
                    ])
                    .values_panic([Uuid::new_v4().into(), library.into(), root.as_uuid().into()]),
            ),
        )
        .await
        .unwrap();
    let mut objects = vec![
        (title, None, "Dark", "Directory", true),
        (
            season,
            Some(title),
            "Season 01",
            "Directory",
            season_indexed,
        ),
        (season_nfo, Some(season), "season.nfo", "File", false),
    ];
    let media_parent = if nested_episode {
        objects.push((
            episode_directory,
            Some(season),
            "Dark S01E01",
            "Directory",
            true,
        ));
        episode_directory
    } else {
        season
    };
    objects.extend([
        (video, Some(media_parent), "Dark.S01E01.mkv", "File", false),
        (
            subtitle,
            Some(media_parent),
            "Dark.S01E01.eng.srt",
            "File",
            false,
        ),
    ]);
    if additional_flat_episode {
        objects.push((
            StorageObjectRecordId::new(),
            Some(media_parent),
            "Dark.S01E02.mkv",
            "File",
            false,
        ));
        objects.extend([
            (
                episode_nfos[0],
                Some(media_parent),
                "Dark.S01E01.nfo",
                "File",
                false,
            ),
            (
                episode_nfos[1],
                Some(media_parent),
                "Dark.S01E02.nfo",
                "File",
                false,
            ),
        ]);
    }
    for (id, parent, name, object_type, indexed) in objects {
        database
            .execute(
                sql.build(
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
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("identity_quality"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            id.as_uuid().into(),
                            account.into(),
                            "local".into(),
                            id.to_string().into(),
                            name.into(),
                            name.to_lowercase().into(),
                            object_type.into(),
                            (object_type == "File").then_some(1024_i64).into(),
                            1_i64.into(),
                            indexed.into(),
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
                sql.build(
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
                            id.as_uuid().into(),
                            parent.map(StorageObjectRecordId::as_uuid).into(),
                            1_i64.into(),
                            indexed.into(),
                            1_i64.into(),
                            "Present".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
    }
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("identity_matches"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_object_id"),
                        Alias::new("candidate_catalog_item_id"),
                        Alias::new("confidence"),
                        Alias::new("state"),
                        Alias::new("evidence"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        title.as_uuid().into(),
                        series.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        serde_json::json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    Fixture {
        database,
        series,
        account,
        root,
        season,
        season_nfo,
        episode_nfos,
        video,
        subtitle,
    }
}

async fn expand_series(fixture: &Fixture) {
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(fixture.series),
            1,
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
            "series-expand",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    SeriesExpandService::new(fixture.database.clone())
        .execute(&claimed)
        .await
        .unwrap();
}

async fn mark_object_fact_pending(fixture: &Fixture, object: StorageObjectRecordId) {
    let sql = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .value(Alias::new("sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.root.as_uuid())),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("storage_objects"))
                    .value(Alias::new("observed_sync_revision"), 2_i64)
                    .value(
                        Alias::new("facts_observed_storage_root_id"),
                        fixture.root.as_uuid(),
                    )
                    .and_where(Expr::col(Alias::new("id")).eq(object.as_uuid())),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("observed_sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(fixture.root.as_uuid()))
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(object.as_uuid())),
            ),
        )
        .await
        .unwrap();
}

#[allow(clippy::too_many_lines)] // Builds one complete root-local outbox change fixture.
async fn project_scope_change(
    fixture: &Fixture,
    object_id: StorageObjectRecordId,
    normalized_name: &str,
) {
    seed_scope_change(fixture, object_id, normalized_name).await;
    apply_next_scope_change(fixture).await;
}

#[allow(clippy::too_many_lines)] // Builds one complete root-local outbox change fixture.
async fn seed_scope_change(
    fixture: &Fixture,
    object_id: StorageObjectRecordId,
    normalized_name: &str,
) {
    let backend = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .value(Alias::new("sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.root.as_uuid())),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
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
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("identity_quality"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        object_id.as_uuid().into(),
                        fixture.account.into(),
                        "local".into(),
                        object_id.to_string().into(),
                        normalized_name.into(),
                        normalized_name.into(),
                        "File".into(),
                        1024_i64.into(),
                        2_i64.into(),
                        false.into(),
                        2_i64.into(),
                        "StableFileId".into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
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
                        fixture.root.as_uuid().into(),
                        object_id.as_uuid().into(),
                        fixture.season.as_uuid().into(),
                        2_i64.into(),
                        false.into(),
                        2_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_change_outbox"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("event_type"),
                        Alias::new("storage_object_id"),
                        Alias::new("payload_version"),
                        Alias::new("payload"),
                        Alias::new("dedupe_key"),
                        Alias::new("state"),
                        Alias::new("attempt_count"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        fixture.root.as_uuid().into(),
                        2_i64.into(),
                        "Upserted".into(),
                        object_id.as_uuid().into(),
                        1_i32.into(),
                        serde_json::json!({
                            "version": 1,
                            "kind": "Upserted",
                            "relation": {
                                "parent_storage_object_id": fixture.season,
                            },
                        })
                        .into(),
                        format!("{}:2:{object_id}:Upserted", fixture.root).into(),
                        "Pending".into(),
                        0_i32.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

async fn apply_next_scope_change(fixture: &Fixture) {
    let claimed = OutboxRepository::new(&fixture.database)
        .claim_next(
            fixture.root,
            "structure-scope-projector",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    StorageChangeProjectionRepository::new(&fixture.database)
        .apply(&claimed)
        .await
        .unwrap();
}

async fn projected_children(fixture: &Fixture) -> (CatalogItemId, CatalogItemId) {
    let query = CatalogQueryRepository::new(&fixture.database);
    let season = query
        .items(
            UserId::new(),
            BrowseParent::Item(fixture.series),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap()
        .items()[0]
        .id();
    let episode = query
        .items(
            UserId::new(),
            BrowseParent::Item(season),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap()
        .items()[0]
        .id();
    (season, episode)
}

async fn projected_episode_metadata_sidecars(
    fixture: &Fixture,
) -> Vec<Option<StorageObjectRecordId>> {
    let query = CatalogQueryRepository::new(&fixture.database);
    let season = query
        .items(
            UserId::new(),
            BrowseParent::Item(fixture.series),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap()
        .items()[0]
        .id();
    let episodes = query
        .items(
            UserId::new(),
            BrowseParent::Item(season),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap()
        .items()
        .iter()
        .map(tjxy_db::CatalogItemRecord::id)
        .collect::<Vec<_>>();
    let jobs = WorkJobRepository::new(&fixture.database);
    for episode in &episodes {
        let target = query
            .lazy_work_target(UserId::new(), *episode)
            .await
            .unwrap()
            .unwrap();
        jobs.enqueue_or_join(
            &WorkJobSpec::new(
                WorkTaskKind::ResolveMetadata,
                WorkScope::CatalogItem(*episode),
                target.metadata_revision(),
                100,
            )
            .unwrap()
            .with_metadata_requirement(MetadataRequirement::Full)
            .unwrap()
            .with_input_sync_revision(1)
            .unwrap(),
        )
        .await
        .unwrap();
    }
    let mut sidecars = Vec::new();
    for _ in episodes {
        let claimed = jobs
            .claim_next(
                &[WorkTaskKind::ResolveMetadata],
                "flat-episode-metadata",
                chrono::Duration::minutes(5),
            )
            .await
            .unwrap()
            .unwrap();
        sidecars.push(
            MetadataWorkRepository::new(&fixture.database)
                .snapshot(&claimed)
                .await
                .unwrap()
                .sidecar()
                .map(tjxy_db::MetadataSidecarCandidate::record_id),
        );
    }
    sidecars
}

#[tokio::test]
async fn storage_change_in_projected_scope_invalidates_structure_and_episode_sources() {
    let fixture = fixture(true, false, false).await;
    expand_series(&fixture).await;
    let (_, episode) = projected_children(&fixture).await;

    project_scope_change(&fixture, StorageObjectRecordId::new(), "dark.s01e02.mkv").await;

    let backend = fixture.database.get_database_backend();
    let series = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("structure_expansion_revision"),
                        Alias::new("structure_state"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.series.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        series
            .try_get::<i64>("", "structure_expansion_revision")
            .unwrap(),
        2
    );
    assert_eq!(
        series.try_get::<String>("", "structure_state").unwrap(),
        "NotExpanded"
    );
    let episode_row = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("source_index_revision"),
                        Alias::new("source_state"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(episode.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        episode_row
            .try_get::<i64>("", "source_index_revision")
            .unwrap(),
        2
    );
    assert_eq!(
        episode_row.try_get::<String>("", "source_state").unwrap(),
        "NotIndexed"
    );
    assert!(
        CatalogPublicationRepository::new(&fixture.database)
            .active_sources(episode)
            .await
            .unwrap()
            .is_empty(),
        "the stale Structure publication must stop serving sources"
    );
}

#[tokio::test]
async fn nfo_change_in_projected_scope_invalidates_projected_metadata() {
    let fixture = fixture(true, false, false).await;
    expand_series(&fixture).await;
    let (season, _) = projected_children(&fixture).await;

    project_scope_change(&fixture, StorageObjectRecordId::new(), "season-updated.nfo").await;

    let row = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("metadata_revision"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(season.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "metadata_revision").unwrap(), 1);
}

#[tokio::test]
async fn consecutive_changes_keep_invalidating_the_active_structure_projection() {
    let fixture = fixture(true, false, false).await;
    expand_series(&fixture).await;
    let (_, episode) = projected_children(&fixture).await;

    seed_scope_change(&fixture, StorageObjectRecordId::new(), "dark.s01e02.mkv").await;
    seed_scope_change(&fixture, StorageObjectRecordId::new(), "dark.s01e03.mkv").await;
    apply_next_scope_change(&fixture).await;
    apply_next_scope_change(&fixture).await;

    let row = fixture
        .database
        .query_one(
            fixture.database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("source_index_revision"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(episode.as_uuid())),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "source_index_revision").unwrap(), 3);
}

#[tokio::test]
async fn expand_publishes_seasons_episodes_and_episode_sources_atomically() {
    let fixture = fixture(true, false, false).await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(fixture.series),
            1,
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
            "series-expand",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let query = CatalogQueryRepository::new(&fixture.database);
    assert!(
        query
            .items(
                UserId::new(),
                BrowseParent::Item(fixture.series),
                CatalogPageRequest::new(0, 20).unwrap()
            )
            .await
            .unwrap()
            .items()
            .is_empty()
    );

    let generation = SeriesExpandService::new(fixture.database.clone())
        .execute(&claimed)
        .await
        .unwrap();

    assert_eq!(generation, 1);
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Completed
    );
    let seasons = query
        .items(
            UserId::new(),
            BrowseParent::Item(fixture.series),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(seasons.items().len(), 1);
    assert_eq!(seasons.items()[0].item_type(), "Season");
    let season = seasons.items()[0].id();
    let episodes = query
        .items(
            UserId::new(),
            BrowseParent::Item(season),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(episodes.items().len(), 1);
    assert_eq!(episodes.items()[0].item_type(), "Episode");
    let sources = CatalogPublicationRepository::new(&fixture.database)
        .active_sources(episodes.items()[0].id())
        .await
        .unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].locations()[0].storage_object_id(), fixture.video);
    assert_eq!(
        sources[0].subtitles()[0].storage_object_id(),
        fixture.subtitle
    );
    assert!(
        query
            .lazy_work_target(UserId::new(), episodes.items()[0].id())
            .await
            .unwrap()
            .unwrap()
            .has_current_sources(),
        "the active Structure source projection must satisfy the Episode source revision"
    );
}

#[tokio::test]
async fn expanded_items_retain_their_structure_directory_scope() {
    let fixture = fixture(true, false, false).await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(fixture.series),
            1,
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
            "series-expand",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    SeriesExpandService::new(fixture.database.clone())
        .execute(&claimed)
        .await
        .unwrap();

    let query = CatalogQueryRepository::new(&fixture.database);
    let season = query
        .items(
            UserId::new(),
            BrowseParent::Item(fixture.series),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap()
        .items()[0]
        .id();
    let episode = query
        .items(
            UserId::new(),
            BrowseParent::Item(season),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap()
        .items()[0]
        .id();

    let season_scope = query
        .lazy_work_target(UserId::new(), season)
        .await
        .unwrap()
        .unwrap()
        .storage_scope()
        .expect("the Structure projection must retain the Season directory scope");
    let episode_scope = query
        .lazy_work_target(UserId::new(), episode)
        .await
        .unwrap()
        .unwrap()
        .storage_scope()
        .expect("the Structure projection must retain the Episode directory scope");

    assert_eq!(season_scope.storage_object_id(), fixture.season);
    assert_eq!(episode_scope.storage_object_id(), fixture.season);
}

#[tokio::test]
async fn projected_season_metadata_reads_nfo_from_its_structure_scope() {
    let fixture = fixture(true, false, false).await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(fixture.series),
            1,
            100,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let expand = jobs
        .claim_next(
            &[WorkTaskKind::ExpandItem],
            "series-expand",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    SeriesExpandService::new(fixture.database.clone())
        .execute(&expand)
        .await
        .unwrap();

    let query = CatalogQueryRepository::new(&fixture.database);
    let season = query
        .items(
            UserId::new(),
            BrowseParent::Item(fixture.series),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap()
        .items()[0]
        .id();
    let target = query
        .lazy_work_target(UserId::new(), season)
        .await
        .unwrap()
        .unwrap();
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(season),
            target.metadata_revision(),
            100,
        )
        .unwrap()
        .with_metadata_requirement(MetadataRequirement::Full)
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let metadata = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "metadata",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    let snapshot = MetadataWorkRepository::new(&fixture.database)
        .snapshot(&metadata)
        .await
        .unwrap();

    assert_eq!(
        snapshot
            .sidecar()
            .map(tjxy_db::MetadataSidecarCandidate::record_id),
        Some(fixture.season_nfo)
    );
}

#[tokio::test]
async fn projected_episode_ignores_directory_level_season_nfo() {
    let fixture = fixture(true, false, false).await;
    expand_series(&fixture).await;
    let (_, episode) = projected_children(&fixture).await;
    let target = CatalogQueryRepository::new(&fixture.database)
        .lazy_work_target(UserId::new(), episode)
        .await
        .unwrap()
        .unwrap();
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(episode),
            target.metadata_revision(),
            100,
        )
        .unwrap()
        .with_metadata_requirement(MetadataRequirement::Full)
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "episode-metadata",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    let snapshot = MetadataWorkRepository::new(&fixture.database)
        .snapshot(&claimed)
        .await
        .unwrap();

    assert!(snapshot.sidecar().is_none());
}

#[tokio::test]
async fn flat_episodes_select_their_same_stem_nfo() {
    let fixture = fixture(true, false, true).await;
    expand_series(&fixture).await;

    let sidecars = projected_episode_metadata_sidecars(&fixture).await;

    assert_eq!(sidecars.len(), 2);
    for nfo in &fixture.episode_nfos {
        assert!(sidecars.contains(&Some(*nfo)));
    }
}

#[tokio::test]
async fn flat_episode_does_not_borrow_a_sibling_nfo() {
    let fixture = fixture(true, false, true).await;
    expand_series(&fixture).await;
    let removed = fixture.episode_nfos[1];
    let sql = fixture.database.get_database_backend();
    for (table, column) in [
        ("storage_root_objects", "storage_object_id"),
        ("storage_objects", "id"),
    ] {
        fixture
            .database
            .execute(
                sql.build(
                    Query::update()
                        .table(Alias::new(table))
                        .value(Alias::new("presence_state"), "ConfirmedAbsent")
                        .and_where(Expr::col(Alias::new(column)).eq(removed.as_uuid())),
                ),
            )
            .await
            .unwrap();
    }

    let sidecars = projected_episode_metadata_sidecars(&fixture).await;

    assert_eq!(sidecars.len(), 2);
    assert_eq!(
        sidecars
            .iter()
            .filter(|sidecar| **sidecar == Some(fixture.episode_nfos[0]))
            .count(),
        1
    );
    assert_eq!(
        sidecars.iter().filter(|sidecar| sidecar.is_none()).count(),
        1
    );
}

#[tokio::test]
async fn metadata_commit_rejects_a_structure_scope_invalidated_after_snapshot() {
    let fixture = fixture(true, false, false).await;
    expand_series(&fixture).await;
    let (season, _) = projected_children(&fixture).await;
    let jobs = WorkJobRepository::new(&fixture.database);
    let target = CatalogQueryRepository::new(&fixture.database)
        .lazy_work_target(UserId::new(), season)
        .await
        .unwrap()
        .unwrap();
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ResolveMetadata,
            WorkScope::CatalogItem(season),
            target.metadata_revision(),
            100,
        )
        .unwrap()
        .with_metadata_requirement(MetadataRequirement::Full)
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::ResolveMetadata],
            "season-metadata-race",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let repository = MetadataWorkRepository::new(&fixture.database);
    let snapshot = repository.snapshot(&claimed).await.unwrap();
    let source = MetadataSource::new("Fixture", None::<String>, 1_000).unwrap();
    let resolution = MetadataResolution::from_candidate(
        snapshot.lookup(),
        MetadataCandidate::new(source).with_title("Season 1"),
    )
    .unwrap();
    fixture
        .database
        .execute(
            fixture.database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("catalog_items"))
                    .value(Alias::new("structure_expansion_revision"), 2_i64)
                    .value(Alias::new("structure_state"), "NotExpanded")
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.series.as_uuid())),
            ),
        )
        .await
        .unwrap();

    let error = repository
        .commit(&claimed, &snapshot, &resolution, None, false, Vec::new())
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        tjxy_db::MetadataWorkError::StaleOrUnavailable
    ));
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Running
    );
}

#[tokio::test]
async fn expand_schedules_unindexed_directories_before_publication() {
    let fixture = fixture(false, false, false).await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(fixture.series),
            1,
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
            "series-expand",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    let error = SeriesExpandService::new(fixture.database.clone())
        .execute(&claimed)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        tjxy_application::SeriesExpandError::InventoryPending { scheduled: 1 }
    ));
    let sync = jobs
        .claim_next(
            &[WorkTaskKind::ScopedStorageSync],
            "inventory",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sync.job().scope(), WorkScope::StorageObject(fixture.season));
}

#[tokio::test]
async fn pending_descendant_fact_rejects_series_expand_snapshot() {
    let fixture = fixture(true, false, false).await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(fixture.series),
            1,
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
            "series-pending-root-snapshot",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    mark_object_fact_pending(&fixture, fixture.video).await;

    let error = SeriesExpandRepository::new(&fixture.database)
        .snapshot(&claimed)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        SeriesExpandRepositoryError::StorageInputPending
    ));
}

#[tokio::test]
async fn pending_video_fact_rejects_projected_episode_source_snapshot() {
    let fixture = fixture(true, false, false).await;
    expand_series(&fixture).await;
    let (_, episode) = projected_children(&fixture).await;
    let jobs = WorkJobRepository::new(&fixture.database);
    let submission = TaskService::new(fixture.database.clone())
        .index_media_sources(UserId::new(), episode)
        .await
        .unwrap();
    assert_eq!(submission.job().storage_root_affinity(), Some(fixture.root));
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "source-pending-root-snapshot",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id(), submission.job().id());
    mark_object_fact_pending(&fixture, fixture.video).await;

    let error = SourceIndexRepository::new(&fixture.database)
        .snapshot(&claimed)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        SourceIndexRepositoryError::StorageInputPending
    ));
}

#[tokio::test]
async fn unrelated_pending_root_revision_does_not_reject_series_snapshot() {
    let fixture = fixture(true, false, false).await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(fixture.series),
            1,
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
            "series-unrelated-pending-root",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    fixture
        .database
        .execute(
            fixture.database.get_database_backend().build(
                Query::update()
                    .table(Alias::new("storage_roots"))
                    .value(Alias::new("sync_revision"), 2_i64)
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.root.as_uuid())),
            ),
        )
        .await
        .unwrap();

    SeriesExpandRepository::new(&fixture.database)
        .snapshot(&claimed)
        .await
        .unwrap();
}

#[tokio::test]
async fn expand_recursively_classifies_episode_directories() {
    let fixture = fixture(true, true, false).await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(fixture.series),
            1,
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
            "series-expand",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    SeriesExpandService::new(fixture.database.clone())
        .execute(&claimed)
        .await
        .unwrap();

    let backend = fixture.database.get_database_backend();
    let source_count = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("media_sources")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_eq!(source_count, 1);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Covers Structure publication, isolated re-index, and Probe handoff.
async fn projected_episode_reindex_keeps_flat_season_sources_isolated() {
    let fixture = fixture(true, false, true).await;
    let jobs = WorkJobRepository::new(&fixture.database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::ExpandItem,
            WorkScope::CatalogItem(fixture.series),
            1,
            100,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let expand = jobs
        .claim_next(
            &[WorkTaskKind::ExpandItem],
            "series-expand",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    SeriesExpandService::new(fixture.database.clone())
        .execute(&expand)
        .await
        .unwrap();

    let query = CatalogQueryRepository::new(&fixture.database);
    let season = query
        .items(
            UserId::new(),
            BrowseParent::Item(fixture.series),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap()
        .items()[0]
        .id();
    let episodes = query
        .items(
            UserId::new(),
            BrowseParent::Item(season),
            CatalogPageRequest::new(0, 20).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(episodes.items().len(), 2);
    let publications = CatalogPublicationRepository::new(&fixture.database);
    let mut target = None;
    for episode in episodes.items() {
        let sources = publications.active_sources(episode.id()).await.unwrap();
        if sources[0].locations()[0].storage_object_id() == fixture.video {
            target = Some(episode.id());
        }
    }
    let target = target.unwrap();

    let submission = TaskService::new(fixture.database.clone())
        .index_media_sources(UserId::new(), target)
        .await
        .unwrap();
    assert_eq!(
        submission.job().task_kind(),
        WorkTaskKind::IndexMediaSources
    );
    let reindex = jobs
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "source-reindex",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    SourceIndexService::new(fixture.database.clone())
        .execute(&reindex)
        .await
        .unwrap();

    let sources = publications.active_sources(target).await.unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].locations().len(), 1);
    assert_eq!(sources[0].locations()[0].storage_object_id(), fixture.video);
    assert_eq!(sources[0].subtitles().len(), 1);
    assert_eq!(
        sources[0].subtitles()[0].storage_object_id(),
        fixture.subtitle
    );

    let probe_jobs = ManualProbeRepository::new(&fixture.database)
        .enqueue_item(target, 100, 10)
        .await
        .unwrap();
    assert_eq!(probe_jobs.len(), 1);
    let probe = jobs
        .claim_next(
            &[WorkTaskKind::ProbeMedia],
            "probe",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let candidate = ProbeRepository::new(&fixture.database)
        .candidate(&probe)
        .await
        .unwrap()
        .expect("the newer direct source publication must win over the structure publication");
    assert_eq!(candidate.storage_object_id(), fixture.video);
}
