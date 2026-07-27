use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_application::{SourceIndexService, StorageChangeProjector};
use tjxy_common::{CatalogItemId, SortKey, StorageObjectRecordId, StorageRootId};
use tjxy_db::{
    CatalogPublicationRepository, OutboxRepository, WorkJobRepository, WorkJobSpec, WorkJobState,
    WorkScope, WorkTaskKind,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

#[tokio::test]
#[allow(clippy::too_many_lines)] // Mirrors the matched directory and its synchronized children.
async fn source_index_publishes_video_and_sidecar_from_sql_inventory() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let sql = database.get_database_backend();
    let library = Uuid::new_v4();
    let item = CatalogItemId::new();
    let account = Uuid::new_v4();
    let root = StorageRootId::new();
    let parent = StorageObjectRecordId::new();
    let video = StorageObjectRecordId::new();
    let subtitle = StorageObjectRecordId::new();
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
                        item.as_uuid().into(),
                        "Movie".into(),
                        "Arrival".into(),
                        "arrival".into(),
                        SortKey::from_text("Arrival").into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        "NotApplicable".into(),
                        "NotIndexed".into(),
                        0_i64.into(),
                        1_i64.into(),
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
                    .values_panic([Uuid::new_v4().into(), library.into(), item.as_uuid().into()]),
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
    for (id, name, object_type) in [
        (parent, "Arrival", "Directory"),
        (video, "Arrival.mkv", "File"),
        (subtitle, "Arrival.eng.srt", "File"),
    ] {
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
                            1_i64.into(),
                            (id == parent).into(),
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
                            if id == parent {
                                None::<Uuid>
                            } else {
                                Some(parent.as_uuid())
                            }
                            .into(),
                            1_i64.into(),
                            (id == parent).into(),
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
                        parent.as_uuid().into(),
                        item.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        serde_json::json!({}).into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let jobs = WorkJobRepository::new(&database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::IndexMediaSources,
            WorkScope::CatalogItem(item),
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
            &[WorkTaskKind::IndexMediaSources],
            "source-index",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    let generation = SourceIndexService::new(database.clone())
        .execute(&claimed)
        .await
        .unwrap();

    assert_eq!(generation, 1);
    assert_eq!(
        jobs.get(claimed.id()).await.unwrap().unwrap().state(),
        WorkJobState::Completed
    );
    let sources = CatalogPublicationRepository::new(&database)
        .active_sources(item)
        .await
        .unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].container(), Some("mkv"));
    assert_eq!(sources[0].locations()[0].storage_object_id(), video);
    assert_eq!(sources[0].subtitles()[0].storage_object_id(), subtitle);
    assert_eq!(sources[0].subtitles()[0].format(), "srt");
    assert_eq!(sources[0].subtitles()[0].language(), Some("eng"));
    let stable = (sources[0].id(), sources[0].presentation_key());
    let previous_location_id = sources[0].locations()[0].id();
    let replacement = StorageObjectRecordId::new();
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
                        Alias::new("observed_sync_revision"),
                        Alias::new("children_indexed"),
                        Alias::new("children_index_revision"),
                        Alias::new("identity_quality"),
                        Alias::new("presence_state"),
                    ])
                    .values_panic([
                        replacement.as_uuid().into(),
                        account.into(),
                        "local".into(),
                        "renamed-path".into(),
                        "Arrival Renamed.mkv".into(),
                        "arrival renamed.mkv".into(),
                        "File".into(),
                        2_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "PathWeak".into(),
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
                        replacement.as_uuid().into(),
                        parent.as_uuid().into(),
                        2_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("presence_state"), "ConfirmedAbsent")
                    .and_where(Expr::col(Alias::new("storage_object_id")).eq(video.as_uuid())),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
                Query::insert()
                    .into_table(Alias::new("storage_relink_candidates"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_root_id"),
                        Alias::new("previous_storage_object_id"),
                        Alias::new("replacement_storage_object_id"),
                        Alias::new("confidence"),
                        Alias::new("evidence"),
                        Alias::new("state"),
                        Alias::new("created_at"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        root.as_uuid().into(),
                        video.as_uuid().into(),
                        replacement.as_uuid().into(),
                        0.6.into(),
                        serde_json::json!({"same_modified_at":true}).into(),
                        "Confirmed".into(),
                        chrono::Utc::now().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            sql.build(
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
            sql.build(
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
                        Alias::new("created_at"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        root.as_uuid().into(),
                        2_i64.into(),
                        "Upserted".into(),
                        replacement.as_uuid().into(),
                        1.into(),
                        serde_json::json!({
                            "version": 1,
                            "kind": "Upserted",
                            "relation": {
                                "storage_root_id": root,
                                "storage_object_id": replacement,
                                "parent_storage_object_id": parent,
                            },
                        })
                        .into(),
                        format!("{root}:2:{replacement}:Upserted").into(),
                        "Pending".into(),
                        0.into(),
                        chrono::Utc::now().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let outbox = OutboxRepository::new(&database);
    let change = outbox
        .claim_next(root, "projector", chrono::Duration::minutes(5))
        .await
        .unwrap()
        .unwrap();
    let completion = StorageChangeProjector::new(database.clone())
        .apply(&change)
        .await
        .unwrap();
    assert_eq!(completion.reconciled_sync_revision, 2);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::IndexMediaSources,
            WorkScope::CatalogItem(item),
            2,
            100,
        )
        .unwrap()
        .with_input_sync_revision(1)
        .unwrap(),
    )
    .await
    .unwrap();
    let reindex = jobs
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "source-reindex",
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    SourceIndexService::new(database.clone())
        .execute(&reindex)
        .await
        .unwrap();
    let sources = CatalogPublicationRepository::new(&database)
        .active_sources(item)
        .await
        .unwrap();
    assert_eq!((sources[0].id(), sources[0].presentation_key(),), stable);
    assert_ne!(sources[0].locations()[0].id(), previous_location_id);
    assert_eq!(sources[0].locations()[0].storage_object_id(), replacement);
}
