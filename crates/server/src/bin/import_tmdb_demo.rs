use std::{env, path::PathBuf};

use chrono::Utc;
use futures_util::{StreamExt, TryStreamExt, stream};
use sea_orm::{Database, DatabaseConnection, DbErr};
use thiserror::Error;
use tjxy_application::{
    AssetWriteError, AssetWriteService, MetadataImageFetchError, MetadataImageFetcher,
    ReqwestMetadataImageFetcher,
};
use tjxy_common::{CatalogItemId, ImageType};
use tjxy_credentials::{CredentialCipher, CredentialCipherError};
use tjxy_db::{
    AssetPublication, DemoCatalogPublication, DemoCatalogPublicationError, DemoCatalogRepository,
    MetadataProviderSettingsRepository, MetadataProviderSettingsRepositoryError,
    demo_catalog_item_id, migrate_database,
};
use tjxy_metadata::{
    MetadataError, MetadataImageReference, MetadataProviderError, RichCatalogItem, RichEpisode,
    RichRemoteImageKind, RichSeries, TmdbCatalogClient,
};
use tjxy_server::{CredentialKeyringError, parse_credential_keyring};
use zeroize::Zeroizing;

const TMDB_PROVIDER_KEY: &str = "tmdb";
const METADATA_FETCH_CONCURRENCY: usize = 8;
const ASSET_FETCH_CONCURRENCY: usize = 12;
const MAX_SEASONS_PER_SERIES: usize = 3;
const MAX_EPISODES_PER_SEASON: usize = 12;
const MOVIE_IDS: [u64; 100] = [
    329_865, 496_243, 129, 238, 76_341, 324_857, 843, 146, 546_554, 545_611, 693_134, 155, 120,
    122, 278, 550, 557, 603, 652, 671, 680, 862, 1_930, 5_721, 19_995, 24_428, 27_205, 44_980,
    47_612, 64_439, 82_976, 83_533, 89_657, 98_894, 102_382, 151_024, 157_336, 299_536, 299_710,
    315_635, 329_505, 393_392, 424_711, 429_617, 454_639, 532_794, 533_535, 569_094, 599_335,
    617_126, 634_649, 687_163, 755_898, 822_119, 840_464, 872_585, 931_285, 936_075, 949_838,
    969_681, 976_912, 980_431, 1_003_596, 1_007_757, 1_061_474, 1_081_003, 1_083_381, 1_084_242,
    1_084_244, 1_108_427, 1_110_034, 1_122_573, 1_127_384, 1_212_763, 1_226_863, 1_228_710,
    1_234_821, 1_235_877, 1_273_221, 1_275_779, 1_280_738, 1_284_465, 1_285_366, 1_290_821,
    1_297_842, 1_301_310, 1_301_421, 1_304_313, 1_305_672, 1_305_781, 1_307_247, 1_311_031,
    1_314_481, 1_315_772, 1_318_447, 1_318_621, 1_321_008, 1_327_819, 1_337_148, 1_339_713,
];
const SERIES_IDS: [u64; 100] = [
    87_108, 87_739, 115_004, 91_275, 81_355, 94_028, 40, 456, 498, 502, 549, 688, 693, 764, 841,
    1_396, 1_398, 1_399, 1_402, 1_405, 1_408, 1_412, 1_416, 1_419, 1_421, 1_431, 1_433, 1_434,
    1_620, 1_622, 1_636, 1_668, 1_911, 2_122, 2_224, 2_261, 2_288, 2_316, 95_396, 2_734, 3_022,
    3_034, 4_057, 4_177, 4_601, 4_604, 4_614, 4_656, 5_920, 6_489, 8_590, 13_945, 14_424, 17_887,
    18_165, 22_980, 30_979, 30_983, 30_984, 31_132, 32_692, 32_798, 33_238, 34_307, 36_109, 37_680,
    39_351, 44_006, 44_217, 45_140, 46_271, 46_952, 51_025, 57_243, 59_941, 60_625, 60_735, 60_802,
    61_818, 63_174, 63_770, 65_334, 65_682, 65_701, 66_732, 67_707, 70_998, 71_712, 71_715, 71_790,
    73_586, 75_219, 75_384, 76_479, 77_826, 79_744, 80_748, 82_452, 82_873, 85_552,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DemoManifest {
    movie_ids: &'static [u64],
    series_ids: &'static [u64],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ImportMode {
    Import,
    PrintPopularManifest,
}

fn demo_manifest() -> DemoManifest {
    DemoManifest {
        movie_ids: &MOVIE_IDS,
        series_ids: &SERIES_IDS,
    }
}

#[derive(Debug, Error)]
enum ImportDemoError {
    #[error("usage: import_tmdb_demo [--print-popular-manifest]")]
    InvalidArguments,
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
    #[error("database schema is incompatible: {0}")]
    DatabaseSchema(#[from] tjxy_db::SchemaMigrationError),
    #[error("metadata settings could not be read: {0}")]
    Settings(#[from] MetadataProviderSettingsRepositoryError),
    #[error("configured TMDB credential could not be decrypted: {0}")]
    Cipher(#[from] CredentialCipherError),
    #[error("TMDB client configuration is invalid: {0}")]
    MetadataConfiguration(#[from] MetadataError),
    #[error("TMDB catalog request failed: {0}")]
    MetadataProvider(#[from] MetadataProviderError),
    #[error("TMDB {kind} {id} request failed: {source}")]
    MetadataRecord {
        kind: &'static str,
        id: u64,
        #[source]
        source: MetadataProviderError,
    },
    #[error("image client configuration failed: {0}")]
    ImageFetch(#[from] MetadataImageFetchError),
    #[error("asset storage failed: {0}")]
    Asset(#[from] AssetWriteError),
    #[error("demo publication failed: {0}")]
    Publication(#[from] DemoCatalogPublicationError),
}

#[tokio::main]
async fn main() -> Result<(), ImportDemoError> {
    let mode = import_mode(env::args())?;
    let database_url =
        env::var("TJXY_DATABASE_URL").unwrap_or_else(|_| "sqlite://tjxy.db?mode=rwc".to_owned());
    let assets_dir =
        env::var("TJXY_ASSETS_DIR").map_or_else(|_| PathBuf::from("./data/assets"), PathBuf::from);
    let encoded_keyring = Zeroizing::new(
        env::var("TJXY_CREDENTIAL_KEYRING").map_err(|_| ImportDemoError::MissingKeyring)?,
    );
    let cipher = parse_credential_keyring(&encoded_keyring)?;
    let database = Database::connect(database_url).await?;
    migrate_database(&database).await?;
    let (client, language) = configured_client(&database, &cipher, |access_token, language| {
        TmdbCatalogClient::new(access_token.to_owned(), language.to_owned())
    })
    .await?;
    if mode == ImportMode::PrintPopularManifest {
        print_popular_manifest(&client).await?;
        return Ok(());
    }
    let manifest = demo_manifest();
    println!(
        "Fetching {} movies and {} series from TMDB...",
        manifest.movie_ids.len(),
        manifest.series_ids.len()
    );
    let movies = stream::iter(manifest.movie_ids.iter().copied())
        .map(|id| {
            let client = &client;
            async move {
                client
                    .movie(id)
                    .await
                    .map_err(|source| ImportDemoError::MetadataRecord {
                        kind: "movie",
                        id,
                        source,
                    })
            }
        })
        .buffered(METADATA_FETCH_CONCURRENCY)
        .try_collect::<Vec<_>>()
        .await?;
    let series = stream::iter(manifest.series_ids.iter().copied())
        .map(|id| {
            let client = &client;
            async move {
                client
                    .series(id)
                    .await
                    .map(|series| {
                        series
                            .with_structure_limits(MAX_SEASONS_PER_SERIES, MAX_EPISODES_PER_SEASON)
                    })
                    .map_err(|source| ImportDemoError::MetadataRecord {
                        kind: "series",
                        id,
                        source,
                    })
            }
        })
        .buffered(METADATA_FETCH_CONCURRENCY)
        .try_collect::<Vec<_>>()
        .await?;
    println!("TMDB metadata fetch complete.");

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

fn import_mode<I, S>(args: I) -> Result<ImportMode, ImportDemoError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();
    let _binary = args.next();
    let argument = args.next().map(|value| value.as_ref().to_owned());
    if args.next().is_some() {
        return Err(ImportDemoError::InvalidArguments);
    }
    match argument.as_deref() {
        None => Ok(ImportMode::Import),
        Some("--print-popular-manifest") => Ok(ImportMode::PrintPopularManifest),
        Some(_) => Err(ImportDemoError::InvalidArguments),
    }
}

async fn print_popular_manifest(client: &TmdbCatalogClient) -> Result<(), ImportDemoError> {
    let mut movies = Vec::with_capacity(120);
    let mut series = Vec::with_capacity(120);
    for page in 1..=6 {
        movies.extend(client.popular_movie_ids(page).await?);
        series.extend(client.popular_series_ids(page).await?);
    }
    movies.sort_unstable();
    movies.dedup();
    series.sort_unstable();
    series.dedup();
    println!("MOVIE_IDS={movies:?}");
    println!("SERIES_IDS={series:?}");
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
    let items = catalog_items(movies, series);
    println!("Preparing artwork for {} catalog items...", items.len());
    let prepared = stream::iter(items)
        .map(|item| prepare_item_assets(writer, fetcher, item))
        .buffered(ASSET_FETCH_CONCURRENCY)
        .try_collect::<Vec<_>>()
        .await?;
    let mut publications = Vec::new();
    let mut warnings = 0;
    for (mut item_publications, item_warnings) in prepared {
        publications.append(&mut item_publications);
        warnings += item_warnings;
    }
    println!(
        "Artwork preparation complete: {} assets, {} warnings.",
        publications.len(),
        warnings
    );
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
            items.extend(season.episodes().iter().map(RichEpisode::item));
        }
    }
    items
}

async fn prepare_item_assets(
    writer: &AssetWriteService,
    fetcher: &dyn MetadataImageFetcher,
    item: &RichCatalogItem,
) -> Result<(Vec<AssetPublication>, usize), ImportDemoError> {
    let mut publications = Vec::new();
    let mut warnings = 0;
    for image in item.images() {
        let Some(reference) = MetadataImageReference::tmdb(image.path()) else {
            warnings += 1;
            continue;
        };
        let Ok(bytes) = fetcher.fetch(&reference).await else {
            warnings += 1;
            continue;
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
                warnings += 1;
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        publications.push(prepared.publication().clone());
    }
    Ok((publications, warnings))
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

    use super::{ImportDemoError, ImportMode, configured_client, demo_manifest, import_mode};

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

        assert!(movies.len() >= 100);
        assert!(series.len() >= 100);
        assert!(movies.iter().all(|id| *id > 0));
        assert!(series.iter().all(|id| *id > 0));
        assert_eq!(movies.len(), manifest.movie_ids.len());
        assert_eq!(series.len(), manifest.series_ids.len());
        assert!(manifest.movie_ids.contains(&329_865));
        assert!(manifest.series_ids.contains(&87_108));
    }

    #[test]
    fn print_manifest_mode_is_explicit_and_rejects_unknown_arguments() {
        assert_eq!(
            import_mode(["import_tmdb_demo", "--print-popular-manifest"]).unwrap(),
            ImportMode::PrintPopularManifest
        );
        assert_eq!(
            import_mode(["import_tmdb_demo"]).unwrap(),
            ImportMode::Import
        );
        assert!(import_mode(["import_tmdb_demo", "--unknown"]).is_err());
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
