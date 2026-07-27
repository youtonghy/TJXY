use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, TimeZone, Utc};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
    TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{UserId, Username};
use tjxy_credentials::CredentialEnvelope;
use tjxy_db::{ApiKeyDraft, ApiKeyRepository, ApiKeyRepositoryError, AuthRepository, AuthUser};
use tjxy_test_support::{reconnectable_test_database, test_database};
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn reconnect_lookup_database(database_url: &str) -> DatabaseConnection {
    let mut options = ConnectOptions::new(database_url);
    options.max_connections(1).min_connections(1);
    let database = Database::connect(options).await.unwrap();
    if database.get_database_backend() == DbBackend::Sqlite {
        database
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA busy_timeout = 5000",
            ))
            .await
            .unwrap();
    }
    database
}

fn envelope(marker: u8) -> CredentialEnvelope {
    CredentialEnvelope::from_parts(7, vec![marker; 28]).unwrap()
}

fn draft(
    actor: &AuthUser,
    envelope_id: Uuid,
    token_digest: [u8; 32],
    app_name: &str,
    created_at: DateTime<Utc>,
) -> ApiKeyDraft {
    ApiKeyDraft {
        envelope_id,
        creator_user_id: actor.id(),
        creator_auth_revision: actor.auth_revision(),
        token_digest,
        envelope: envelope(token_digest[0]),
        app_name: app_name.to_owned(),
        created_at,
    }
}

async fn user(
    database: &DatabaseConnection,
    name: &str,
    is_admin: bool,
    now: DateTime<Utc>,
) -> AuthUser {
    AuthRepository::new(database)
        .create_user(
            &Username::parse(name).unwrap(),
            "$argon2id$test-only",
            true,
            is_admin,
            now,
        )
        .await
        .unwrap()
}

async fn api_key_count_for_user(database: &DatabaseConnection, user_id: UserId) -> i64 {
    let query = Query::select()
        .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
        .from(Alias::new("api_keys"))
        .and_where(Expr::col(Alias::new("creator_user_id")).eq(user_id.as_uuid()))
        .to_owned();
    database
        .query_one(database.get_database_backend().build(&query))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap()
}

async fn last_used_at(database: &DatabaseConnection, digest: [u8; 32]) -> Option<DateTime<Utc>> {
    let query = Query::select()
        .column(Alias::new("last_used_at"))
        .from(Alias::new("api_keys"))
        .and_where(Expr::col(Alias::new("token_digest")).eq(digest.to_vec()))
        .to_owned();
    database
        .query_one(database.get_database_backend().build(&query))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "last_used_at")
        .unwrap()
}

async fn overwrite_app_name(database: &DatabaseConnection, digest: [u8; 32], app_name: &str) {
    let update = Query::update()
        .table(Alias::new("api_keys"))
        .value(Alias::new("app_name"), app_name)
        .and_where(Expr::col(Alias::new("token_digest")).eq(digest.to_vec()))
        .to_owned();
    database
        .execute(database.get_database_backend().build(&update))
        .await
        .unwrap();
}

#[tokio::test]
async fn create_rejects_missing_capacity_lock_state_without_inserting() {
    let database = database().await;
    let repository = ApiKeyRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let admin = user(&database, "Admin", true, now).await;
    let delete = Query::delete()
        .from_table(Alias::new("auth_state"))
        .and_where(Expr::col(Alias::new("id")).eq(1_i32))
        .to_owned();
    database
        .execute(database.get_database_backend().build(&delete))
        .await
        .unwrap();

    let error = repository
        .create(
            &admin,
            draft(&admin, Uuid::from_u128(1), [7; 32], "Rejected", now),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ApiKeyRepositoryError::MissingCapacityState));
    assert_eq!(api_key_count_for_user(&database, admin.id()).await, 0);
}

#[tokio::test]
async fn list_rejects_a_stored_blank_app_name() {
    let database = database().await;
    let repository = ApiKeyRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let admin = user(&database, "Admin", true, now).await;
    repository
        .create(
            &admin,
            draft(&admin, Uuid::from_u128(1), [7; 32], "Valid", now),
        )
        .await
        .unwrap();
    overwrite_app_name(&database, [7; 32], "   ").await;

    let error = repository.list(&admin).await.unwrap_err();

    assert!(matches!(error, ApiKeyRepositoryError::InvalidStoredAppName));
}

#[tokio::test]
async fn startup_list_rejects_a_stored_control_character_app_name() {
    let database = database().await;
    let repository = ApiKeyRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let admin = user(&database, "Admin", true, now).await;
    repository
        .create(
            &admin,
            draft(&admin, Uuid::from_u128(1), [7; 32], "Valid", now),
        )
        .await
        .unwrap();
    overwrite_app_name(&database, [7; 32], "bad\nname").await;

    let error = repository.list_for_startup().await.unwrap_err();

    assert!(matches!(error, ApiKeyRepositoryError::InvalidStoredAppName));
}

#[tokio::test]
async fn lifecycle_lists_complete_rows_newest_first_and_allows_duplicate_app_names() {
    let database = database().await;
    let repository = ApiKeyRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let admin = user(&database, "Admin", true, now).await;
    let first_envelope_id = Uuid::from_u128(1);
    let second_envelope_id = Uuid::from_u128(2);

    repository
        .create(
            &admin,
            draft(&admin, first_envelope_id, [7; 32], "Kodi Sync", now),
        )
        .await
        .unwrap();
    repository
        .create(
            &admin,
            draft(&admin, second_envelope_id, [8; 32], "Kodi Sync", now),
        )
        .await
        .unwrap();

    let listed = repository.list(&admin).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].app_name(), "Kodi Sync");
    assert_eq!(listed[0].envelope_id(), second_envelope_id);
    assert_eq!(listed[0].creator_user_id(), admin.id());
    assert_eq!(listed[0].creator_auth_revision(), 0);
    assert_eq!(listed[0].token_digest(), &[8; 32]);
    assert_eq!(listed[0].envelope(), &envelope(8));
    assert_eq!(listed[0].created_at(), now);
    assert_eq!(listed[0].last_used_at(), None);
    assert!(listed[0].id() > listed[1].id());

    let startup = repository.list_for_startup().await.unwrap();
    assert_eq!(startup, listed);

    repository.delete_by_digest(&admin, &[8; 32]).await.unwrap();
    repository.delete_by_digest(&admin, &[8; 32]).await.unwrap();
    let remaining = repository.list(&admin).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].envelope_id(), first_envelope_id);
}

#[tokio::test]
async fn create_enforces_capacity_and_maps_both_unique_constraints() {
    let database = database().await;
    let repository = ApiKeyRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let admin = user(&database, "Admin", true, now).await;

    for index in 0_u16..256 {
        let mut digest = [0_u8; 32];
        digest[0..2].copy_from_slice(&index.to_be_bytes());
        repository
            .create(
                &admin,
                draft(
                    &admin,
                    Uuid::from_u128(u128::from(index) + 1),
                    digest,
                    "Capacity fixture",
                    now + Duration::seconds(i64::from(index)),
                ),
            )
            .await
            .unwrap();
    }

    let capacity_error = repository
        .create(
            &admin,
            draft(
                &admin,
                Uuid::from_u128(10_000),
                [250; 32],
                "One too many",
                now + Duration::hours(1),
            ),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        capacity_error,
        ApiKeyRepositoryError::CapacityReached
    ));
    assert_eq!(repository.list(&admin).await.unwrap().len(), 256);

    repository.delete_by_digest(&admin, &[0; 32]).await.unwrap();
    let duplicate_envelope = repository
        .create(
            &admin,
            draft(
                &admin,
                Uuid::from_u128(2),
                [251; 32],
                "Duplicate envelope",
                now,
            ),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        duplicate_envelope,
        ApiKeyRepositoryError::DuplicateCredential
    ));

    let duplicate_digest = repository
        .create(
            &admin,
            draft(
                &admin,
                Uuid::from_u128(20_000),
                {
                    let mut digest = [0_u8; 32];
                    digest[1] = 1;
                    digest
                },
                "Duplicate digest",
                now,
            ),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        duplicate_digest,
        ApiKeyRepositoryError::DuplicateCredential
    ));
    assert_eq!(repository.list(&admin).await.unwrap().len(), 255);
}

#[tokio::test]
async fn create_list_and_delete_reject_non_admin_stale_and_disabled_actors() {
    let database = database().await;
    let repository = ApiKeyRepository::new(&database);
    let auth = AuthRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let admin = user(&database, "Admin", true, now).await;
    let other_admin = user(&database, "Other Admin", true, now).await;
    let non_admin = user(&database, "Viewer", false, now).await;
    repository
        .create(
            &admin,
            draft(&admin, Uuid::from_u128(1), [7; 32], "Kept", now),
        )
        .await
        .unwrap();

    let create_error = repository
        .create(
            &non_admin,
            draft(&non_admin, Uuid::from_u128(2), [8; 32], "Rejected", now),
        )
        .await
        .unwrap_err();
    assert!(matches!(create_error, ApiKeyRepositoryError::ActorRejected));

    auth.rename_user(
        admin.id(),
        &Username::parse("Renamed Admin").unwrap(),
        now + Duration::seconds(1),
    )
    .await
    .unwrap();
    let list_error = repository.list(&admin).await.unwrap_err();
    assert!(matches!(list_error, ApiKeyRepositoryError::ActorRejected));

    let disabled = auth
        .update_policy(other_admin.id(), true, true, now + Duration::seconds(2))
        .await
        .unwrap();
    let delete_error = repository
        .delete_by_digest(&disabled, &[7; 32])
        .await
        .unwrap_err();
    assert!(matches!(delete_error, ApiKeyRepositoryError::ActorRejected));
}

#[tokio::test]
async fn digest_lookup_returns_current_admin_principal_and_throttles_activity_writes() {
    let database = database().await;
    let repository = ApiKeyRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let admin = user(&database, "Admin", true, now).await;
    let digest = [7; 32];
    repository
        .create(
            &admin,
            draft(&admin, Uuid::from_u128(1), digest, "Infuse", now),
        )
        .await
        .unwrap();
    let id = repository.list(&admin).await.unwrap()[0].id();

    let principal = repository
        .find_principal_by_token_digest(&digest, now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(principal.user().id(), admin.id());
    assert_eq!(principal.user().name(), "Admin");
    assert!(principal.user().is_admin());
    assert_eq!(principal.api_key_id(), Some(id));
    assert_eq!(principal.session_id(), None);
    assert_eq!(last_used_at(&database, digest).await, Some(now));

    repository
        .find_principal_by_token_digest(&digest, now + Duration::minutes(3))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(last_used_at(&database, digest).await, Some(now));

    let after_threshold = now + Duration::minutes(3) + Duration::seconds(1);
    repository
        .find_principal_by_token_digest(&digest, after_threshold)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(last_used_at(&database, digest).await, Some(after_threshold));
    assert!(
        repository
            .find_principal_by_token_digest(&[8; 32], now)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn digest_lookup_cannot_complete_after_a_concurrent_delete_commits() {
    let fixture = reconnectable_test_database().await.unwrap();
    let database = fixture.connection();
    tjxy_db::Migrator::up(database, None).await.unwrap();
    let lookup_database = reconnect_lookup_database(fixture.database_url()).await;

    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let admin = user(database, "Admin", true, now).await;
    let digest = [7; 32];
    ApiKeyRepository::new(database)
        .create(
            &admin,
            draft(&admin, Uuid::from_u128(1), digest, "Race fence", now),
        )
        .await
        .unwrap();

    let delete = database.begin().await.unwrap();
    let statement = Query::delete()
        .from_table(Alias::new("api_keys"))
        .and_where(Expr::col(Alias::new("token_digest")).eq(digest.to_vec()))
        .to_owned();
    delete
        .execute(delete.get_database_backend().build(&statement))
        .await
        .unwrap();

    let mut lookup = tokio::spawn(async move {
        ApiKeyRepository::new(&lookup_database)
            .find_principal_by_token_digest(&digest, now)
            .await
    });
    assert!(
        tokio::time::timeout(StdDuration::from_millis(250), &mut lookup)
            .await
            .is_err(),
        "lookup must wait for the concurrent delete transaction"
    );

    delete.commit().await.unwrap();
    let principal = tokio::time::timeout(StdDuration::from_secs(5), lookup)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        principal.is_none(),
        "a delete that commits before lookup completion must win the race"
    );
}

#[tokio::test]
async fn digest_lookup_cannot_complete_after_a_creator_revision_mutation_commits() {
    let fixture = reconnectable_test_database().await.unwrap();
    let database = fixture.connection();
    tjxy_db::Migrator::up(database, None).await.unwrap();
    let lookup_database = reconnect_lookup_database(fixture.database_url()).await;
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let admin = user(database, "Admin", true, now).await;
    let digest = [7; 32];
    ApiKeyRepository::new(database)
        .create(
            &admin,
            draft(&admin, Uuid::from_u128(1), digest, "Revision fence", now),
        )
        .await
        .unwrap();

    let mutation = database.begin().await.unwrap();
    let update = Query::update()
        .table(Alias::new("users"))
        .value(
            Alias::new("auth_revision"),
            Expr::col(Alias::new("auth_revision")).add(1_i64),
        )
        .and_where(Expr::col(Alias::new("id")).eq(admin.id().as_uuid()))
        .to_owned();
    mutation
        .execute(mutation.get_database_backend().build(&update))
        .await
        .unwrap();
    let delete = Query::delete()
        .from_table(Alias::new("api_keys"))
        .and_where(Expr::col(Alias::new("creator_user_id")).eq(admin.id().as_uuid()))
        .to_owned();
    mutation
        .execute(mutation.get_database_backend().build(&delete))
        .await
        .unwrap();

    let mut lookup = tokio::spawn(async move {
        ApiKeyRepository::new(&lookup_database)
            .find_principal_by_token_digest(&digest, now)
            .await
    });
    assert!(
        tokio::time::timeout(StdDuration::from_millis(250), &mut lookup)
            .await
            .is_err(),
        "lookup must wait for the concurrent creator mutation"
    );

    mutation.commit().await.unwrap();
    let principal = tokio::time::timeout(StdDuration::from_secs(5), lookup)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        principal.is_none(),
        "a creator mutation that commits before lookup completion must win the race"
    );
}

#[tokio::test]
async fn every_user_revision_mutation_and_user_delete_physically_remove_api_keys() {
    let database = database().await;
    let keys = ApiKeyRepository::new(&database);
    let auth = AuthRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let _other_admin = user(&database, "Other Admin", true, now).await;
    let mut target = user(&database, "Target Admin", true, now).await;

    keys.create(
        &target,
        draft(&target, Uuid::from_u128(1), [1; 32], "Rename", now),
    )
    .await
    .unwrap();
    target = auth
        .rename_user(
            target.id(),
            &Username::parse("Renamed Admin").unwrap(),
            now + Duration::seconds(1),
        )
        .await
        .unwrap();
    assert_eq!(api_key_count_for_user(&database, target.id()).await, 0);

    keys.create(
        &target,
        draft(&target, Uuid::from_u128(2), [2; 32], "Password", now),
    )
    .await
    .unwrap();
    target = auth
        .update_password(
            target.id(),
            "$argon2id$changed",
            true,
            now + Duration::seconds(2),
        )
        .await
        .unwrap();
    assert_eq!(api_key_count_for_user(&database, target.id()).await, 0);

    keys.create(
        &target,
        draft(&target, Uuid::from_u128(3), [3; 32], "Disable", now),
    )
    .await
    .unwrap();
    target = auth
        .update_policy(target.id(), true, true, now + Duration::seconds(3))
        .await
        .unwrap();
    assert_eq!(api_key_count_for_user(&database, target.id()).await, 0);

    target = auth
        .update_policy(target.id(), true, false, now + Duration::seconds(4))
        .await
        .unwrap();
    keys.create(
        &target,
        draft(&target, Uuid::from_u128(4), [4; 32], "Demote", now),
    )
    .await
    .unwrap();
    auth.update_policy(target.id(), false, false, now + Duration::seconds(5))
        .await
        .unwrap();
    assert_eq!(api_key_count_for_user(&database, target.id()).await, 0);

    let deleted = user(&database, "Deleted Admin", true, now).await;
    keys.create(
        &deleted,
        draft(&deleted, Uuid::from_u128(5), [5; 32], "Delete", now),
    )
    .await
    .unwrap();
    auth.delete_user(deleted.id()).await.unwrap();
    assert_eq!(api_key_count_for_user(&database, deleted.id()).await, 0);
}

#[tokio::test]
async fn failed_final_admin_policy_mutation_rolls_back_api_key_deletion() {
    let database = database().await;
    let keys = ApiKeyRepository::new(&database);
    let auth = AuthRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let admin = user(&database, "Only Admin", true, now).await;
    keys.create(
        &admin,
        draft(&admin, Uuid::from_u128(1), [7; 32], "Must survive", now),
    )
    .await
    .unwrap();

    let error = auth
        .update_policy(admin.id(), false, false, now + Duration::seconds(1))
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        tjxy_db::AuthRepositoryError::LastEnabledAdmin
    ));
    assert_eq!(api_key_count_for_user(&database, admin.id()).await, 1);
}
