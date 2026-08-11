use std::{collections::HashSet, env, path::PathBuf};

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
    MetadataError, MetadataImageReference, MetadataProviderError, RichCatalogItem,
    RichRemoteImageKind, RichSeries, TmdbCatalogClient,
};
use tjxy_server::{CredentialKeyringError, parse_credential_keyring};
use zeroize::Zeroizing;

const TMDB_PROVIDER_KEY: &str = "tmdb";
const METADATA_FETCH_CONCURRENCY: usize = 8;
const ASSET_FETCH_CONCURRENCY: usize = 12;
const MOVIES_PER_LIST: usize = 1000;
const PAGES_PER_LIST: u16 = 50; // 20 items per page * 50 pages = 1000
const BATCH_SIZE: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum ContentType {
    Movie,
    Series,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TmdbList {
    Popular,
    TopRated,
}

impl TmdbList {
    fn name(self) -> &'static str {
        match self {
            TmdbList::Popular => "Popular",
            TmdbList::TopRated => "Top Rated",
        }
    }

    fn library_name(self, content_type: ContentType) -> &'static str {
        match (self, content_type) {
            (TmdbList::Popular, ContentType::Movie) => "热门电影",
            (TmdbList::TopRated, ContentType::Movie) => "经典电影",
            (TmdbList::Popular, ContentType::Series) => "热门剧集",
            (TmdbList::TopRated, ContentType::Series) => "经典剧集",
        }
    }
}

#[derive(Debug, Error)]
enum ImportError {
    #[error("usage: import_tmdb_lists")]
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
    #[error("image client configuration failed: {0}")]
    ImageFetch(#[from] MetadataImageFetchError),
    #[error("asset storage failed: {0}")]
    Asset(#[from] AssetWriteError),
    #[error("demo publication failed: {0}")]
    Publication(#[from] DemoCatalogPublicationError),
}

#[tokio::main]
#[allow(clippy::too_many_lines)] // This one-shot importer keeps its fetch and publication progress linear.
async fn main() -> Result<(), ImportError> {
    if env::args().len() > 1 {
        return Err(ImportError::InvalidArguments);
    }

    let database_url =
        env::var("TJXY_DATABASE_URL").unwrap_or_else(|_| "sqlite://tjxy.db?mode=rwc".to_owned());
    let assets_dir =
        env::var("TJXY_ASSETS_DIR").map_or_else(|_| PathBuf::from("./data/assets"), PathBuf::from);
    let encoded_keyring = Zeroizing::new(
        env::var("TJXY_CREDENTIAL_KEYRING").map_err(|_| ImportError::MissingKeyring)?,
    );
    let cipher = parse_credential_keyring(&encoded_keyring)?;
    let database = Database::connect(database_url).await?;
    migrate_database(&database).await?;
    let (client, language) = configured_client(&database, &cipher, |access_token, language| {
        TmdbCatalogClient::new(access_token.to_owned(), language.to_owned())
    })
    .await?;

    // Collect movie and series IDs from both lists (Popular and TopRated)
    let mut all_ids: Vec<(TmdbList, ContentType, Vec<u64>)> = Vec::new();
    let mut seen = HashSet::new();

    // Fetch movies from both lists
    for list in [TmdbList::Popular, TmdbList::TopRated] {
        println!(
            "Fetching {} movie IDs from {}...",
            PAGES_PER_LIST,
            list.name()
        );
        let mut ids = Vec::with_capacity(MOVIES_PER_LIST);
        for page in 1..=PAGES_PER_LIST {
            let page_result = match list {
                TmdbList::Popular => client.popular_movie_ids(page).await,
                TmdbList::TopRated => client.top_rated_movie_ids(page).await,
            };
            let Ok(page_ids) = page_result else {
                println!("  Page {page}: exhausted (API limit reached)");
                break;
            };
            if page_ids.is_empty() {
                println!("  Page {page}: empty (list exhausted)");
                break;
            }
            ids.extend(page_ids);
            if page % 10 == 0 {
                println!(
                    "  Page {}/{} ({} IDs so far)",
                    page,
                    PAGES_PER_LIST,
                    ids.len()
                );
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids.truncate(MOVIES_PER_LIST);
        let before = ids.len();
        ids.retain(|id| seen.insert((ContentType::Movie, *id)));
        println!(
            "  {} Movies: {} IDs ({} after dedup)",
            list.name(),
            before,
            ids.len()
        );
        all_ids.push((list, ContentType::Movie, ids));
    }

    // Fetch series from both lists
    for list in [TmdbList::Popular, TmdbList::TopRated] {
        println!(
            "Fetching {} series IDs from {}...",
            PAGES_PER_LIST,
            list.name()
        );
        let mut ids = Vec::with_capacity(MOVIES_PER_LIST);
        for page in 1..=PAGES_PER_LIST {
            let page_result = match list {
                TmdbList::Popular => client.popular_series_ids(page).await,
                TmdbList::TopRated => client.top_rated_series_ids(page).await,
            };
            let Ok(page_ids) = page_result else {
                println!("  Page {page}: exhausted (API limit reached)");
                break;
            };
            if page_ids.is_empty() {
                println!("  Page {page}: empty (list exhausted)");
                break;
            }
            ids.extend(page_ids);
            if page % 10 == 0 {
                println!(
                    "  Page {}/{} ({} IDs so far)",
                    page,
                    PAGES_PER_LIST,
                    ids.len()
                );
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids.truncate(MOVIES_PER_LIST);
        let before = ids.len();
        ids.retain(|id| seen.insert((ContentType::Series, *id)));
        println!(
            "  {} Series: {} IDs ({} after dedup)",
            list.name(),
            before,
            ids.len()
        );
        all_ids.push((list, ContentType::Series, ids));
    }

    let image_fetcher = ReqwestMetadataImageFetcher::new()?;
    let asset_writer = AssetWriteService::new(database.clone(), assets_dir).await?;

    // Import movies for each list, batched to stay within the per-publication limit
    for (list, content_type, ids) in &all_ids {
        if ids.is_empty() {
            println!(
                "Skipping {} {:?}: no items to import",
                list.name(),
                content_type
            );
            continue;
        }
        println!(
            "\nImporting {} {:?} into {}...",
            ids.len(),
            content_type,
            list.library_name(*content_type)
        );

        // Fetch and publish in batches
        let mut total_imported = 0;
        let mut total_warnings = 0;

        for (batch_idx, batch_ids) in ids.chunks(BATCH_SIZE).enumerate() {
            println!(
                "  Fetching batch {}/{} ({} items)...",
                batch_idx + 1,
                ids.len().div_ceil(BATCH_SIZE),
                batch_ids.len()
            );

            let (batch_movies, batch_series) = match content_type {
                ContentType::Movie => {
                    let mut batch_movies = Vec::with_capacity(batch_ids.len());
                    for chunk in batch_ids.chunks(METADATA_FETCH_CONCURRENCY) {
                        let results = stream::iter(chunk.iter().copied())
                            .map(|id| {
                                let client = &client;
                                async move { (id, client.movie(id).await) }
                            })
                            .buffered(METADATA_FETCH_CONCURRENCY)
                            .collect::<Vec<_>>()
                            .await;
                        for (id, result) in results {
                            match result {
                                Ok(movie) => batch_movies.push(movie),
                                Err(_) => eprintln!("    Warning: failed to fetch movie {id}"),
                            }
                        }
                    }
                    (batch_movies, Vec::new())
                }
                ContentType::Series => {
                    let mut batch_series = Vec::with_capacity(batch_ids.len());
                    for chunk in batch_ids.chunks(METADATA_FETCH_CONCURRENCY) {
                        let results = stream::iter(chunk.iter().copied())
                            .map(|id| {
                                let client = &client;
                                async move { (id, client.series(id).await) }
                            })
                            .buffered(METADATA_FETCH_CONCURRENCY)
                            .collect::<Vec<_>>()
                            .await;
                        for (id, result) in results {
                            match result {
                                Ok(series) => batch_series.push(series),
                                Err(_) => eprintln!("    Warning: failed to fetch series {id}"),
                            }
                        }
                    }
                    (Vec::new(), batch_series)
                }
            };

            if batch_movies.is_empty() && batch_series.is_empty() {
                println!("    Skipping batch: all items failed to fetch");
                continue;
            }

            let (batch_assets, batch_warnings) = if batch_movies.is_empty() {
                prepare_series_assets(&asset_writer, &image_fetcher, &batch_series).await?
            } else {
                prepare_assets(&asset_writer, &image_fetcher, &batch_movies).await?
            };
            total_warnings += batch_warnings;

            let publication = DemoCatalogPublication::new(
                batch_movies,
                batch_series,
                language.clone(),
                Utc::now(),
            )?
            .with_assets(batch_assets)?;
            let report = DemoCatalogRepository::new(&database)
                .publish(&publication)
                .await?;
            total_imported += report.movies() + report.series();
        }

        println!(
            "  Imported {} total items into {}; {} image warnings.",
            total_imported,
            list.library_name(*content_type),
            total_warnings,
        );
    }

    println!("\nAll imports complete!");
    Ok(())
}

async fn configured_client<Client>(
    database: &DatabaseConnection,
    cipher: &CredentialCipher,
    factory: impl FnOnce(&str, &str) -> Result<Client, MetadataError>,
) -> Result<(Client, String), ImportError> {
    let stored = MetadataProviderSettingsRepository::new(database)
        .get(TMDB_PROVIDER_KEY)
        .await?
        .ok_or(ImportError::MissingSettings)?;
    if !stored.enabled() {
        return Err(ImportError::DisabledSettings);
    }
    let plaintext = cipher.open(stored.credential_id(), stored.provider(), stored.envelope())?;
    let access_token =
        std::str::from_utf8(&plaintext).map_err(|_| ImportError::InvalidCredential)?;
    let client = factory(access_token, stored.language())?;
    Ok((client, stored.language().to_owned()))
}

async fn prepare_assets(
    writer: &AssetWriteService,
    fetcher: &dyn MetadataImageFetcher,
    movies: &[RichCatalogItem],
) -> Result<(Vec<AssetPublication>, usize), ImportError> {
    if movies.is_empty() {
        return Ok((Vec::new(), 0));
    }
    println!("  Preparing artwork for {} movies...", movies.len());
    let prepared = stream::iter(movies)
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
        "  Artwork complete: {} assets, {} warnings.",
        publications.len(),
        warnings
    );
    Ok((publications, warnings))
}

async fn prepare_series_assets(
    writer: &AssetWriteService,
    fetcher: &dyn MetadataImageFetcher,
    series: &[RichSeries],
) -> Result<(Vec<AssetPublication>, usize), ImportError> {
    if series.is_empty() {
        return Ok((Vec::new(), 0));
    }
    println!("  Preparing artwork for {} series...", series.len());
    let mut publications = Vec::new();
    let mut warnings = 0;

    for s in series {
        // Prepare assets for the series itself
        let (mut series_pubs, series_warnings) =
            prepare_item_assets(writer, fetcher, s.item()).await?;
        publications.append(&mut series_pubs);
        warnings += series_warnings;

        // Prepare assets for seasons
        for season in s.seasons() {
            let (mut season_pubs, season_warnings) =
                prepare_item_assets(writer, fetcher, season.item()).await?;
            publications.append(&mut season_pubs);
            warnings += season_warnings;
        }
    }

    println!(
        "  Artwork complete: {} assets, {} warnings.",
        publications.len(),
        warnings
    );
    Ok((publications, warnings))
}

async fn prepare_item_assets(
    writer: &AssetWriteService,
    fetcher: &dyn MetadataImageFetcher,
    item: &RichCatalogItem,
) -> Result<(Vec<AssetPublication>, usize), ImportError> {
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
