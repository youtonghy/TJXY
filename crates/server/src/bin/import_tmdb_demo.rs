use std::{env, path::PathBuf};

use chrono::Utc;
use sea_orm::{Database, DatabaseConnection, DbErr};
use sea_orm_migration::MigratorTrait;
use thiserror::Error;
use tjxy_application::{
    AssetWriteError, AssetWriteService, MetadataImageFetchError, MetadataImageFetcher,
    ReqwestMetadataImageFetcher,
};
use tjxy_common::{CatalogItemId, ImageType};
use tjxy_credentials::{CredentialCipher, CredentialCipherError};
use tjxy_db::{
    AssetPublication, DemoCatalogPublication, DemoCatalogPublicationError, DemoCatalogRepository,
    MetadataProviderSettingsRepository, MetadataProviderSettingsRepositoryError, Migrator,
    demo_catalog_item_id,
};
use tjxy_metadata::{
    MetadataError, MetadataImageReference, MetadataProviderError, RichCatalogItem,
    RichRemoteImageKind, RichSeries, TmdbCatalogClient,
};
use tjxy_server::{CredentialKeyringError, parse_credential_keyring};
use zeroize::Zeroizing;

const TMDB_PROVIDER_KEY: &str = "tmdb";
const MOVIE_IDS: [u64; 12] = [
    329_865, 496_243, 129, 238, 76_341, 324_857, 843, 146, 546_554, 545_611, 693_134, 155,
];
const SERIES_IDS: [u64; 6] = [87_108, 87_739, 115_004, 91_275, 81_355, 94_028];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DemoManifest {
    movie_ids: &'static [u64],
    series_ids: &'static [u64],
}

fn demo_manifest() -> DemoManifest {
    DemoManifest {
        movie_ids: &MOVIE_IDS,
        series_ids: &SERIES_IDS,
    }
}

#[derive(Debug, Error)]
enum ImportDemoError {
    #[error("TJXY_CREDENTIAL_KEYRING is required for the encrypted TMDB setting")]
    MissingKeyring,
    #[error("TJXY_CREDENTIAL_KEYRING is invalid")]
    InvalidKeyring(#[from] CredentialKeyringError),
    #[error("configured TMDB settings are missing")]
    MissingSettings,
    #[error("configured TMDB settings are disabled")]
    DisabledSettings,
    #[error("configured TMDB credential is invalid")]
    InvalidCredential,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("metadata settings could not be read: {0}")]
    Settings(#[from] MetadataProviderSettingsRepositoryError),
    #[error("configured TMDB credential could not be decrypted: {0}")]
    Cipher(#[from] CredentialCipherError),
    #[error("TMDB client configuration is invalid: {0}")]
    MetadataConfiguration(#[from] MetadataError),
    #[error("TMDB catalog request failed: {0}")]
    MetadataProvider(#[from] MetadataProviderError),
    #[error("image client configuration failed: {0}")]
    ImageFetch(#[from] MetadataImageFetchError),
    #[error("asset storage failed: {0}")]
    Asset(#[from] AssetWriteError),
    #[error("demo publication failed: {0}")]
    Publication(#[from] DemoCatalogPublicationError),
}

#[tokio::main]
async fn main() -> Result<(), ImportDemoError> {
    let database_url =
        env::var("TJXY_DATABASE_URL").unwrap_or_else(|_| "sqlite://tjxy.db?mode=rwc".to_owned());
    let assets_dir =
        env::var("TJXY_ASSETS_DIR").map_or_else(|_| PathBuf::from("./data/assets"), PathBuf::from);
    let encoded_keyring = Zeroizing::new(
        env::var("TJXY_CREDENTIAL_KEYRING").map_err(|_| ImportDemoError::MissingKeyring)?,
    );
    let cipher = parse_credential_keyring(&encoded_keyring)?;
    let database = Database::connect(database_url).await?;
    Migrator::up(&database, None).await?;
    let (client, language) = configured_client(&database, &cipher, |access_token, language| {
        TmdbCatalogClient::new(access_token.to_owned(), language.to_owned())
    })
    .await?;

    let manifest = demo_manifest();
    let mut movies = Vec::with_capacity(manifest.movie_ids.len());
    for id in manifest.movie_ids {
        movies.push(client.movie(*id).await?);
    }
    let mut series = Vec::with_capacity(manifest.series_ids.len());
    for id in manifest.series_ids {
        series.push(client.series(*id).await?);
    }

    let image_fetcher = ReqwestMetadataImageFetcher::new()?;
    let asset_writer = AssetWriteService::new(database.clone(), assets_dir).await?;
    let (assets, warnings) =
        prepare_assets(&asset_writer, &image_fetcher, &movies, &series).await?;
    let publication =
        DemoCatalogPublication::new(movies, series, language, Utc::now())?.with_assets(assets)?;
    let report = DemoCatalogRepository::new(&database)
        .publish(&publication)
        .await?;

    println!(
        "Imported {} movies, {} series, {} seasons, {} episodes; {} image warnings.",
        report.movies(),
        report.series(),
        report.seasons(),
        report.episodes(),
        warnings
    );
    Ok(())
}

async fn configured_client<Client>(
    database: &DatabaseConnection,
    cipher: &CredentialCipher,
    factory: impl FnOnce(&str, &str) -> Result<Client, MetadataError>,
) -> Result<(Client, String), ImportDemoError> {
    let stored = MetadataProviderSettingsRepository::new(database)
        .get(TMDB_PROVIDER_KEY)
        .await?
        .ok_or(ImportDemoError::MissingSettings)?;
    if !stored.enabled() {
        return Err(ImportDemoError::DisabledSettings);
    }
    let plaintext = cipher.open(stored.credential_id(), stored.provider(), stored.envelope())?;
    let access_token =
        std::str::from_utf8(&plaintext).map_err(|_| ImportDemoError::InvalidCredential)?;
    let client = factory(access_token, stored.language())?;
    Ok((client, stored.language().to_owned()))
}

async fn prepare_assets(
    writer: &AssetWriteService,
    fetcher: &dyn MetadataImageFetcher,
    movies: &[RichCatalogItem],
    series: &[RichSeries],
) -> Result<(Vec<AssetPublication>, usize), ImportDemoError> {
    let mut publications = Vec::new();
    let mut warnings = 0;
    for item in catalog_items(movies, series) {
        prepare_item_assets(writer, fetcher, item, &mut publications, &mut warnings).await?;
    }
    Ok((publications, warnings))
}

fn catalog_items<'catalog>(
    movies: &'catalog [RichCatalogItem],
    series: &'catalog [RichSeries],
) -> Vec<&'catalog RichCatalogItem> {
    let mut items = movies.iter().collect::<Vec<_>>();
    for show in series {
        items.push(show.item());
        for season in show.seasons() {
            items.push(season.item());
            items.extend(season.episodes().iter().map(|episode| episode.item()));
        }
    }
    items
}

async fn prepare_item_assets(
    writer: &AssetWriteService,
    fetcher: &dyn MetadataImageFetcher,
    item: &RichCatalogItem,
    publications: &mut Vec<AssetPublication>,
    warnings: &mut usize,
) -> Result<(), ImportDemoError> {
    for image in item.images() {
        let Some(reference) = MetadataImageReference::tmdb(image.path()) else {
            *warnings += 1;
            continue;
        };
        let bytes = match fetcher.fetch(&reference).await {
            Ok(bytes) => bytes,
            Err(_) => {
                *warnings += 1;
                continue;
            }
        };
        let image_type = match image.kind() {
            RichRemoteImageKind::Primary => ImageType::Primary,
            RichRemoteImageKind::Backdrop => ImageType::Backdrop,
        };
        let prepared = match writer
            .prepare_original(
                CatalogItemId::from_uuid(demo_catalog_item_id(item.kind(), item.provider_id())),
                image_type,
                0,
                reference.provider(),
                Some(reference.reference()),
                bytes.mime_type(),
                bytes.bytes(),
            )
            .await
        {
            Ok(prepared) => prepared,
            Err(error) if invalid_remote_image(&error) => {
                *warnings += 1;
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        publications.push(prepared.publication().clone());
    }
    Ok(())
}

fn invalid_remote_image(error: &AssetWriteError) -> bool {
    matches!(
        error,
        AssetWriteError::InvalidBytes
            | AssetWriteError::EncodedTooLarge
            | AssetWriteError::UnsupportedFormat
            | AssetWriteError::FormatMismatch
            | AssetWriteError::DimensionsTooLarge
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use sea_orm_migration::MigratorTrait;
    use tjxy_credentials::{CredentialCipher, CredentialKey};
    use tjxy_db::{MetadataProviderSettingsRepository, Migrator};
    use tjxy_test_support::test_database;
    use uuid::Uuid;

    use super::{ImportDemoError, configured_client, demo_manifest};

    fn cipher(seed: u8) -> CredentialCipher {
        CredentialCipher::new(CredentialKey::new(1, [seed; 32]).unwrap(), Vec::new()).unwrap()
    }

    async fn database() -> sea_orm::DatabaseConnection {
        let database = test_database().await.unwrap();
        Migrator::up(&database, None).await.unwrap();
        database
    }

    #[test]
    fn manifest_contains_the_approved_unique_movie_and_series_ids() {
        let manifest = demo_manifest();
        let mut movies = manifest.movie_ids.to_vec();
        movies.sort_unstable();
        movies.dedup();
        let mut series = manifest.series_ids.to_vec();
        series.sort_unstable();
        series.dedup();

        assert_eq!(movies.len(), 12);
        assert_eq!(series.len(), 6);
        assert_eq!(
            manifest.movie_ids,
            [
                329_865, 496_243, 129, 238, 76_341, 324_857, 843, 146, 546_554, 545_611, 693_134,
                155,
            ]
        );
        assert_eq!(
            manifest.series_ids,
            [87_108, 87_739, 115_004, 91_275, 81_355, 94_028]
        );
    }

    #[tokio::test]
    async fn missing_disabled_and_undecryptable_settings_fail_before_client_creation() {
        let database = database().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let current_cipher = cipher(7);

        let error = configured_client(&database, &current_cipher, {
            let calls = Arc::clone(&calls);
            move |_, _| {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        })
        .await
        .unwrap_err();
        assert!(matches!(error, ImportDemoError::MissingSettings));

        let sealed = current_cipher
            .seal_bound(Uuid::new_v4(), "tmdb", b"not-a-real-token")
            .unwrap();
        MetadataProviderSettingsRepository::new(&database)
            .put(&sealed, false, "zh-CN", None)
            .await
            .unwrap();
        let error = configured_client(&database, &current_cipher, {
            let calls = Arc::clone(&calls);
            move |_, _| {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        })
        .await
        .unwrap_err();
        assert!(matches!(error, ImportDemoError::DisabledSettings));

        MetadataProviderSettingsRepository::new(&database)
            .put(&sealed, true, "zh-CN", Some(1))
            .await
            .unwrap();
        let error = configured_client(&database, &cipher(8), {
            let calls = Arc::clone(&calls);
            move |_, _| {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        })
        .await
        .unwrap_err();
        assert!(matches!(error, ImportDemoError::Cipher(_)));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }
}
