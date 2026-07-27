use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_db::{CredentialRefreshState, StorageAccountRepository, StorageCredentialRepository};
use tjxy_test_support::test_database;
use uuid::Uuid;

#[tokio::test]
async fn encrypted_credentials_round_trip_without_persisting_plaintext() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let credential_id = Uuid::new_v4();
    let cipher =
        CredentialCipher::new(CredentialKey::new(4, [4_u8; 32]).unwrap(), Vec::new()).unwrap();
    let envelope = cipher
        .seal(credential_id, "google-drive", b"refresh-token-secret")
        .unwrap();

    StorageCredentialRepository::new(&database)
        .put(credential_id, &envelope, CredentialRefreshState::Ready)
        .await
        .unwrap();

    let stored = StorageCredentialRepository::new(&database)
        .get(credential_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.refresh_state(), CredentialRefreshState::Ready);
    assert_eq!(stored.envelope(), &envelope);
    assert_eq!(
        cipher
            .open(credential_id, "google-drive", stored.envelope())
            .unwrap()
            .as_slice(),
        b"refresh-token-secret"
    );
    let backend = database.get_database_backend();
    let raw = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("encrypted_payload"))
                    .from(Alias::new("storage_credentials"))
                    .and_where(Expr::col(Alias::new("id")).eq(credential_id)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<Vec<u8>>("", "encrypted_payload")
        .unwrap();
    assert!(
        !raw.windows(b"refresh-token-secret".len())
            .any(|window| window == b"refresh-token-secret")
    );
}

#[tokio::test]
async fn credential_update_replaces_envelope_and_refresh_state_atomically() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let credential_id = Uuid::new_v4();
    let first = CredentialCipher::new(CredentialKey::new(1, [1_u8; 32]).unwrap(), Vec::new())
        .unwrap()
        .seal(credential_id, "google-drive", b"first")
        .unwrap();
    let second = CredentialCipher::new(CredentialKey::new(2, [2_u8; 32]).unwrap(), Vec::new())
        .unwrap()
        .seal(credential_id, "google-drive", b"second")
        .unwrap();
    let repository = StorageCredentialRepository::new(&database);
    repository
        .put(credential_id, &first, CredentialRefreshState::Ready)
        .await
        .unwrap();

    repository
        .put(
            credential_id,
            &second,
            CredentialRefreshState::ReauthenticationRequired,
        )
        .await
        .unwrap();

    let stored = repository.get(credential_id).await.unwrap().unwrap();
    assert_eq!(stored.envelope(), &second);
    assert_eq!(
        stored.refresh_state(),
        CredentialRefreshState::ReauthenticationRequired
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Builds one normalized provider binding fixture.
async fn active_provider_bindings_return_distinct_drives_and_parsed_credential_references() {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let account_id = Uuid::new_v4();
    let credential_id = Uuid::new_v4();
    let root_id = Uuid::new_v4();
    let object_id = Uuid::new_v4();
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
                        account_id.into(),
                        "google-drive".into(),
                        "Drive".into(),
                        "account".into(),
                        credential_id.to_string().into(),
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
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                    ])
                    .values_panic([
                        root_id.into(),
                        account_id.into(),
                        "root".into(),
                        0_i64.into(),
                        0_i64.into(),
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
                        object_id.into(),
                        account_id.into(),
                        "shared-drive".into(),
                        "root".into(),
                        "Root".into(),
                        "root".into(),
                        "Directory".into(),
                        0_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "ProviderStableId".into(),
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
                        root_id.into(),
                        object_id.into(),
                        0_i64.into(),
                        false.into(),
                        0_i64.into(),
                        "Present".into(),
                    ]),
            ),
        )
        .await
        .unwrap();

    let bindings = StorageAccountRepository::new(&database)
        .active_provider_bindings("google-drive")
        .await
        .unwrap();

    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].account_id(), account_id);
    assert_eq!(bindings[0].credential_id(), credential_id);
    assert_eq!(bindings[0].provider_drive_id(), "shared-drive");
}
