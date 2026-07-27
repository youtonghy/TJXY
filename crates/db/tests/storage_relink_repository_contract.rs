use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, JoinType, Query},
};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tjxy_common::{CatalogItemId, StorageObjectRecordId, StorageRootId, UserId};
use tjxy_db::{StorageRelinkDecision, StorageRelinkRepository};
use tjxy_test_support::test_database;
use uuid::Uuid;

struct Fixture {
    database: DatabaseConnection,
    candidate_id: Uuid,
    item_id: CatalogItemId,
    old_object_id: StorageObjectRecordId,
    new_object_id: StorageObjectRecordId,
    presentation_key: Uuid,
    user_id: UserId,
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps identity, user-data, and source reuse in one atomic scenario.
async fn confirming_a_path_weak_candidate_preserves_user_data_and_stable_source_identity() {
    let fixture = fixture().await;
    let repository = StorageRelinkRepository::new(&fixture.database);

    let first = repository
        .decide(fixture.candidate_id, StorageRelinkDecision::Confirm)
        .await
        .unwrap();
    let repeated = repository
        .decide(fixture.candidate_id, StorageRelinkDecision::Confirm)
        .await
        .unwrap();

    assert!(first.changed());
    assert!(!repeated.changed());
    assert_eq!(first.state(), "Confirmed");
    let identities = Alias::new("identity_matches");
    let catalog = Alias::new("catalog_items");
    let user_data = Alias::new("user_data");
    let sources = Alias::new("media_sources");
    let backend = fixture.database.get_database_backend();
    let row = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(
                        Expr::col((identities.clone(), Alias::new("state"))),
                        Alias::new("state"),
                    )
                    .expr_as(
                        Expr::col((identities.clone(), Alias::new("candidate_catalog_item_id"))),
                        Alias::new("candidate_catalog_item_id"),
                    )
                    .expr_as(
                        Expr::col((catalog.clone(), Alias::new("source_index_revision"))),
                        Alias::new("source_index_revision"),
                    )
                    .expr_as(
                        Expr::col((user_data.clone(), Alias::new("is_favorite"))),
                        Alias::new("is_favorite"),
                    )
                    .expr_as(
                        Expr::col((user_data.clone(), Alias::new("playback_position_ticks"))),
                        Alias::new("playback_position_ticks"),
                    )
                    .expr_as(
                        Expr::col((sources.clone(), Alias::new("presentation_key"))),
                        Alias::new("presentation_key"),
                    )
                    .from(identities.clone())
                    .join(
                        JoinType::InnerJoin,
                        catalog.clone(),
                        Expr::col((catalog.clone(), Alias::new("id")))
                            .equals((identities.clone(), Alias::new("candidate_catalog_item_id"))),
                    )
                    .join(
                        JoinType::InnerJoin,
                        user_data.clone(),
                        Expr::col((user_data.clone(), Alias::new("catalog_item_id")))
                            .equals((catalog.clone(), Alias::new("id")))
                            .and(
                                Expr::col((user_data.clone(), Alias::new("user_id")))
                                    .eq(fixture.user_id.as_uuid()),
                            ),
                    )
                    .join(
                        JoinType::InnerJoin,
                        sources.clone(),
                        Expr::col((sources, Alias::new("catalog_item_id")))
                            .equals((catalog, Alias::new("id"))),
                    )
                    .and_where(
                        Expr::col((identities, Alias::new("storage_object_id")))
                            .eq(fixture.new_object_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "Matched");
    assert_eq!(
        row.try_get::<Uuid>("", "candidate_catalog_item_id")
            .unwrap(),
        fixture.item_id.as_uuid()
    );
    assert_eq!(row.try_get::<i64>("", "source_index_revision").unwrap(), 1);
    assert!(row.try_get::<bool>("", "is_favorite").unwrap());
    assert_eq!(
        row.try_get::<i64>("", "playback_position_ticks").unwrap(),
        42
    );
    assert_eq!(
        row.try_get::<Uuid>("", "presentation_key").unwrap(),
        fixture.presentation_key
    );
    let old_presence = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("presence_state"))
                    .from(Alias::new("storage_root_objects"))
                    .and_where(
                        Expr::col(Alias::new("storage_object_id"))
                            .eq(fixture.old_object_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "presence_state")
        .unwrap();
    assert_eq!(old_presence, "ConfirmedAbsent");
}

#[tokio::test]
async fn rejecting_a_candidate_is_idempotent_and_does_not_create_identity_matches() {
    let fixture = fixture().await;
    let repository = StorageRelinkRepository::new(&fixture.database);

    let first = repository
        .decide(fixture.candidate_id, StorageRelinkDecision::Reject)
        .await
        .unwrap();
    let repeated = repository
        .decide(fixture.candidate_id, StorageRelinkDecision::Reject)
        .await
        .unwrap();

    assert!(first.changed());
    assert!(!repeated.changed());
    assert_eq!(first.state(), "Rejected");
    let backend = fixture.database.get_database_backend();
    let count = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                    .from(Alias::new("identity_matches"))
                    .and_where(
                        Expr::col(Alias::new("storage_object_id"))
                            .eq(fixture.new_object_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_eq!(count, 0);
    assert!(matches!(
        repository
            .decide(fixture.candidate_id, StorageRelinkDecision::Confirm)
            .await,
        Err(tjxy_db::StorageRelinkRepositoryError::DecisionConflict)
    ));
}

#[tokio::test]
async fn pending_queue_is_bounded_and_redacts_provider_path_identities() {
    let fixture = fixture().await;
    let candidates = StorageRelinkRepository::new(&fixture.database)
        .pending(50)
        .await
        .unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id(), fixture.candidate_id);
    assert_eq!(candidates[0].previous_name(), "Old.mkv");
    assert_eq!(candidates[0].replacement_name(), "Renamed.mkv");
    assert_eq!(candidates[0].state(), "Pending");
    assert!(
        candidates[0].evidence()["same_modified_at"]
            .as_bool()
            .unwrap()
    );
    assert!(
        StorageRelinkRepository::new(&fixture.database)
            .pending(0)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn stale_candidate_cannot_be_confirmed_and_remains_pending() {
    let fixture = fixture().await;
    let backend = fixture.database.get_database_backend();
    fixture
        .database
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_root_objects"))
                    .value(Alias::new("presence_state"), "ConfirmedAbsent")
                    .and_where(
                        Expr::col(Alias::new("storage_object_id"))
                            .eq(fixture.new_object_id.as_uuid()),
                    ),
            ),
        )
        .await
        .unwrap();
    let repository = StorageRelinkRepository::new(&fixture.database);

    assert!(matches!(
        repository
            .decide(fixture.candidate_id, StorageRelinkDecision::Confirm)
            .await,
        Err(tjxy_db::StorageRelinkRepositoryError::StaleCandidate)
    ));
    let state = fixture
        .database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("state"))
                    .from(Alias::new("storage_relink_candidates"))
                    .and_where(Expr::col(Alias::new("id")).eq(fixture.candidate_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "state")
        .unwrap();
    assert_eq!(state, "Pending");
}

#[allow(clippy::too_many_lines)]
async fn fixture() -> Fixture {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let account_id = Uuid::new_v4();
    let root_id = StorageRootId::new();
    let old_object_id = StorageObjectRecordId::new();
    let new_object_id = StorageObjectRecordId::new();
    let item_id = CatalogItemId::new();
    let user_id = UserId::new();
    let source_id = Uuid::new_v4();
    let presentation_key = Uuid::new_v4();
    let candidate_id = Uuid::new_v4();
    let backend = database.get_database_backend();
    for statement in [
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
                account_id.into(),
                "filesystem".into(),
                "Local".into(),
                "local".into(),
                "none".into(),
                "Active".into(),
            ])
            .to_owned(),
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
                root_id.as_uuid().into(),
                account_id.into(),
                "root".into(),
                1_i64.into(),
                1_i64.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("catalog_items"))
            .columns([
                Alias::new("id"),
                Alias::new("item_type"),
                Alias::new("name"),
                Alias::new("sort_name"),
                Alias::new("metadata_state"),
                Alias::new("classification_state"),
                Alias::new("structure_state"),
                Alias::new("source_state"),
                Alias::new("structure_expansion_revision"),
                Alias::new("source_index_revision"),
                Alias::new("metadata_revision"),
                Alias::new("is_present"),
            ])
            .values_panic([
                item_id.as_uuid().into(),
                "Movie".into(),
                "Movie".into(),
                "movie".into(),
                "Partial".into(),
                "Matched".into(),
                "NotExpanded".into(),
                "Indexed".into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                true.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("users"))
            .columns([
                Alias::new("id"),
                Alias::new("username"),
                Alias::new("password_hash"),
                Alias::new("is_admin"),
            ])
            .values_panic([
                user_id.as_uuid().into(),
                "user".into(),
                "hash".into(),
                false.into(),
            ])
            .to_owned(),
    ] {
        database.execute(backend.build(&statement)).await.unwrap();
    }
    for (id, provider_id, name, presence) in [
        (old_object_id, "old-path", "Old.mkv", "ConfirmedAbsent"),
        (new_object_id, "new-path", "Renamed.mkv", "Present"),
    ] {
        let object = Query::insert()
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
                account_id.into(),
                "local".into(),
                provider_id.into(),
                name.into(),
                name.to_lowercase().into(),
                "File".into(),
                8_i64.into(),
                1_i64.into(),
                false.into(),
                0_i64.into(),
                "PathWeak".into(),
                presence.into(),
            ])
            .to_owned();
        database.execute(backend.build(&object)).await.unwrap();
        let relation = Query::insert()
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
                root_id.as_uuid().into(),
                id.as_uuid().into(),
                1_i64.into(),
                false.into(),
                0_i64.into(),
                presence.into(),
            ])
            .to_owned();
        database.execute(backend.build(&relation)).await.unwrap();
    }
    for statement in [
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
                old_object_id.as_uuid().into(),
                item_id.as_uuid().into(),
                1.0.into(),
                "Matched".into(),
                json!({"kind":"fixture"}).into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("media_sources"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("presentation_key"),
                Alias::new("probe_state"),
                Alias::new("probe_revision"),
            ])
            .values_panic([
                source_id.into(),
                item_id.as_uuid().into(),
                presentation_key.into(),
                "Probed".into(),
                0_i64.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("media_locations"))
            .columns([
                Alias::new("id"),
                Alias::new("media_source_id"),
                Alias::new("storage_object_id"),
                Alias::new("availability_state"),
                Alias::new("priority"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                source_id.into(),
                old_object_id.as_uuid().into(),
                "Missing".into(),
                0_i32.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("user_data"))
            .columns([
                Alias::new("id"),
                Alias::new("user_id"),
                Alias::new("catalog_item_id"),
                Alias::new("is_favorite"),
                Alias::new("is_played"),
                Alias::new("play_count"),
                Alias::new("playback_position_ticks"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                user_id.as_uuid().into(),
                item_id.as_uuid().into(),
                true.into(),
                false.into(),
                0_i32.into(),
                42_i64.into(),
            ])
            .to_owned(),
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
                candidate_id.into(),
                root_id.as_uuid().into(),
                old_object_id.as_uuid().into(),
                new_object_id.as_uuid().into(),
                0.6.into(),
                json!({"same_modified_at":true}).into(),
                "Pending".into(),
                chrono::Utc::now().into(),
            ])
            .to_owned(),
    ] {
        database.execute(backend.build(&statement)).await.unwrap();
    }
    Fixture {
        database,
        candidate_id,
        item_id,
        old_object_id,
        new_object_id,
        presentation_key,
        user_id,
    }
}
