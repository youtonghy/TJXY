use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_credentials::{CredentialCipher, CredentialKey, SealedCredential};
use tjxy_db::{
    MetadataProviderSettingsRepository, MetadataProviderSettingsRepositoryError, Migrator,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

const PROVIDER: &str = "tmdb";
const TOKEN_FIXTURE: &[u8] = b"tmdb-read-access-token-fixture";

fn cipher(active_version: i32, historical_versions: &[i32]) -> CredentialCipher {
    let key_byte = u8::try_from(active_version).unwrap();
    CredentialCipher::new(
        CredentialKey::new(active_version, [key_byte; 32]).unwrap(),
        historical_versions
            .iter()
            .map(|version| {
                CredentialKey::new(*version, [u8::try_from(*version).unwrap(); 32]).unwrap()
            })
            .collect(),
    )
    .unwrap()
}

fn sealed(
    cipher: &CredentialCipher,
    credential_id: Uuid,
    provider: &str,
    token: &[u8],
) -> SealedCredential {
    cipher.seal_bound(credential_id, provider, token).unwrap()
}

#[tokio::test]
async fn missing_provider_setting_returns_none() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();

    let stored = MetadataProviderSettingsRepository::new(&database)
        .get(PROVIDER)
        .await
        .unwrap();

    assert!(stored.is_none());
}

#[tokio::test]
async fn put_creates_and_rotates_one_encrypted_setting_with_revision_fencing() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let credential_id = Uuid::new_v4();
    let first_cipher = cipher(1, &[]);
    let first_sealed = sealed(&first_cipher, credential_id, PROVIDER, TOKEN_FIXTURE);
    let repository = MetadataProviderSettingsRepository::new(&database);

    let created = repository
        .put(&first_sealed, true, "zh-CN", None)
        .await
        .unwrap();
    assert_eq!(created.provider(), PROVIDER);
    assert!(created.enabled());
    assert_eq!(created.language(), "zh-CN");
    assert_eq!(created.credential_id(), credential_id);
    assert_eq!(created.envelope(), first_sealed.envelope());
    assert_eq!(created.revision(), 1);
    assert_eq!(
        first_cipher
            .open(
                created.credential_id(),
                created.provider(),
                created.envelope(),
            )
            .unwrap()
            .as_slice(),
        TOKEN_FIXTURE
    );
    let backend = database.get_database_backend();
    let raw = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("encrypted_payload"))
                    .from(Alias::new("metadata_provider_settings"))
                    .and_where(Expr::col(Alias::new("provider")).eq(PROVIDER)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<Vec<u8>>("", "encrypted_payload")
        .unwrap();
    assert!(
        !raw.windows(TOKEN_FIXTURE.len())
            .any(|window| window == TOKEN_FIXTURE)
    );

    let rotated_cipher = cipher(2, &[1]);
    let replacement = sealed(
        &rotated_cipher,
        credential_id,
        PROVIDER,
        b"replacement-token",
    );
    let updated = repository
        .put(&replacement, false, "en-AU", Some(1))
        .await
        .unwrap();
    assert!(!updated.enabled());
    assert_eq!(updated.language(), "en-AU");
    assert_eq!(updated.credential_id(), credential_id);
    assert_eq!(updated.envelope(), replacement.envelope());
    assert_eq!(updated.revision(), 2);
    assert!(updated.updated_at() >= created.updated_at());
    assert_eq!(
        rotated_cipher
            .open(
                updated.credential_id(),
                updated.provider(),
                updated.envelope(),
            )
            .unwrap()
            .as_slice(),
        b"replacement-token"
    );

    let stale = repository.put(&first_sealed, true, "fr-FR", Some(1)).await;
    assert!(matches!(
        stale,
        Err(MetadataProviderSettingsRepositoryError::RevisionConflict)
    ));
    let after_stale = repository.get(PROVIDER).await.unwrap().unwrap();
    assert!(!after_stale.enabled());
    assert_eq!(after_stale.language(), "en-AU");
    assert_eq!(after_stale.credential_id(), credential_id);
    assert_eq!(after_stale.envelope(), replacement.envelope());
    assert_eq!(after_stale.revision(), 2);
}

#[tokio::test]
async fn rotation_rejects_sealed_values_bound_to_a_different_identity() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let encryption = cipher(5, &[]);
    let credential_id = Uuid::new_v4();
    let initial = sealed(&encryption, credential_id, PROVIDER, b"initial-token");
    let repository = MetadataProviderSettingsRepository::new(&database);
    repository.put(&initial, true, "en-US", None).await.unwrap();
    let mismatched = sealed(&encryption, Uuid::new_v4(), PROVIDER, b"mismatched-token");

    assert!(matches!(
        repository.put(&mismatched, false, "fr-FR", Some(1)).await,
        Err(MetadataProviderSettingsRepositoryError::CredentialIdentityConflict)
    ));
    let wrong_provider = sealed(
        &encryption,
        credential_id,
        "other-provider",
        b"wrong-provider-token",
    );
    assert!(matches!(
        repository
            .put(&wrong_provider, false, "fr-FR", Some(1))
            .await,
        Err(MetadataProviderSettingsRepositoryError::RevisionConflict)
    ));
    assert!(repository.get("other-provider").await.unwrap().is_none());

    let stored = repository.get(PROVIDER).await.unwrap().unwrap();
    assert_eq!(stored.credential_id(), credential_id);
    assert_eq!(stored.envelope(), initial.envelope());
    assert_eq!(stored.revision(), 1);
    assert_eq!(
        encryption
            .open(stored.credential_id(), stored.provider(), stored.envelope(),)
            .unwrap()
            .as_slice(),
        b"initial-token"
    );
}

#[tokio::test]
async fn delete_is_idempotent_and_honors_revision_fences() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let credential_id = Uuid::new_v4();
    let encryption = cipher(3, &[]);
    let encrypted = sealed(&encryption, credential_id, PROVIDER, b"delete-token");
    let repository = MetadataProviderSettingsRepository::new(&database);
    repository
        .put(&encrypted, true, "en-US", None)
        .await
        .unwrap();

    assert!(matches!(
        repository.delete(PROVIDER, Some(2)).await,
        Err(MetadataProviderSettingsRepositoryError::RevisionConflict)
    ));
    assert!(repository.get(PROVIDER).await.unwrap().is_some());
    assert!(repository.delete(PROVIDER, Some(1)).await.unwrap());
    assert!(repository.get(PROVIDER).await.unwrap().is_none());
    assert!(!repository.delete(PROVIDER, None).await.unwrap());
}

#[tokio::test]
async fn invalid_keys_languages_and_stored_envelopes_are_rejected() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let credential_id = Uuid::new_v4();
    let encryption = cipher(4, &[]);
    let encrypted = sealed(&encryption, credential_id, PROVIDER, b"validation-token");
    let repository = MetadataProviderSettingsRepository::new(&database);

    assert!(matches!(
        repository.get("TMDB").await,
        Err(MetadataProviderSettingsRepositoryError::InvalidProvider)
    ));
    assert!(matches!(
        repository.put(&encrypted, true, " language ", None).await,
        Err(MetadataProviderSettingsRepositoryError::InvalidLanguage)
    ));

    let now = chrono::Utc::now();
    let backend = database.get_database_backend();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("metadata_provider_settings"))
                    .columns([
                        Alias::new("provider"),
                        Alias::new("enabled"),
                        Alias::new("language"),
                        Alias::new("credential_id"),
                        Alias::new("encrypted_payload"),
                        Alias::new("key_version"),
                        Alias::new("revision"),
                        Alias::new("created_at"),
                        Alias::new("updated_at"),
                    ])
                    .values_panic([
                        PROVIDER.into(),
                        true.into(),
                        "en-US".into(),
                        credential_id.into(),
                        vec![1_u8, 2, 3].into(),
                        1_i32.into(),
                        1_i64.into(),
                        now.into(),
                        now.into(),
                    ]),
            ),
        )
        .await
        .unwrap();

    let error = repository.get(PROVIDER).await.unwrap_err();
    assert!(matches!(
        error,
        MetadataProviderSettingsRepositoryError::InvalidStoredEnvelope
    ));
    assert!(!error.to_string().contains("1, 2, 3"));
}
