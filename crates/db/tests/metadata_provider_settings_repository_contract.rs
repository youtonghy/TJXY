use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_credentials::{CredentialCipher, CredentialEnvelope, CredentialKey};
use tjxy_db::{
    MetadataProviderSettingsRepository, MetadataProviderSettingsRepositoryError, Migrator,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

const PROVIDER: &str = "tmdb";
const TOKEN_FIXTURE: &[u8] = b"tmdb-read-access-token-fixture";

fn envelope(credential_id: Uuid, key_version: i32, token: &[u8]) -> CredentialEnvelope {
    let key_byte = u8::try_from(key_version).unwrap();
    CredentialCipher::new(
        CredentialKey::new(key_version, [key_byte; 32]).unwrap(),
        Vec::new(),
    )
    .unwrap()
    .seal(credential_id, PROVIDER, token)
    .unwrap()
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
    let first_envelope = envelope(credential_id, 1, TOKEN_FIXTURE);
    let repository = MetadataProviderSettingsRepository::new(&database);

    let created = repository
        .put(
            PROVIDER,
            true,
            "zh-CN",
            credential_id,
            &first_envelope,
            None,
        )
        .await
        .unwrap();
    assert_eq!(created.provider(), PROVIDER);
    assert!(created.enabled());
    assert_eq!(created.language(), "zh-CN");
    assert_eq!(created.credential_id(), credential_id);
    assert_eq!(created.envelope(), &first_envelope);
    assert_eq!(created.revision(), 1);
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

    let replacement_id = Uuid::new_v4();
    let replacement_envelope = envelope(replacement_id, 2, b"replacement-token");
    let updated = repository
        .put(
            PROVIDER,
            false,
            "en-AU",
            replacement_id,
            &replacement_envelope,
            Some(1),
        )
        .await
        .unwrap();
    assert!(!updated.enabled());
    assert_eq!(updated.language(), "en-AU");
    assert_eq!(updated.credential_id(), replacement_id);
    assert_eq!(updated.envelope(), &replacement_envelope);
    assert_eq!(updated.revision(), 2);
    assert!(updated.updated_at() >= created.updated_at());

    let stale = repository
        .put(
            PROVIDER,
            true,
            "fr-FR",
            credential_id,
            &first_envelope,
            Some(1),
        )
        .await;
    assert!(matches!(
        stale,
        Err(MetadataProviderSettingsRepositoryError::RevisionConflict)
    ));
    let after_stale = repository.get(PROVIDER).await.unwrap().unwrap();
    assert!(!after_stale.enabled());
    assert_eq!(after_stale.language(), "en-AU");
    assert_eq!(after_stale.credential_id(), replacement_id);
    assert_eq!(after_stale.envelope(), &replacement_envelope);
    assert_eq!(after_stale.revision(), 2);
}

#[tokio::test]
async fn delete_is_idempotent_and_honors_revision_fences() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let credential_id = Uuid::new_v4();
    let encrypted = envelope(credential_id, 3, b"delete-token");
    let repository = MetadataProviderSettingsRepository::new(&database);
    repository
        .put(PROVIDER, true, "en-US", credential_id, &encrypted, None)
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
    let encrypted = envelope(credential_id, 4, b"validation-token");
    let repository = MetadataProviderSettingsRepository::new(&database);

    assert!(matches!(
        repository.get("TMDB").await,
        Err(MetadataProviderSettingsRepositoryError::InvalidProvider)
    ));
    assert!(matches!(
        repository
            .put(
                PROVIDER,
                true,
                " language ",
                credential_id,
                &encrypted,
                None,
            )
            .await,
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
