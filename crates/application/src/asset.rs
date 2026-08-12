use std::{
    io::{self, Cursor, Write},
    path::{Component, Path},
};

#[cfg(not(unix))]
use std::path::PathBuf;

use image::{GenericImageView, ImageFormat, ImageReader, Limits};
use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::{CatalogItemId, ImageType};
use tjxy_db::{
    AssetPublication, AssetRepository, AssetRepositoryError, AssetStorageError,
    AssetStorageRepository, CatalogQueryError, CatalogQueryRepository,
};
use tokio::fs::File;

pub struct AssetReadService {
    database: DatabaseConnection,
    fallback_root: std::path::PathBuf,
    #[cfg(not(unix))]
    root: PathBuf,
    #[cfg(unix)]
    root_directory: std::fs::File,
}

const MAX_ENCODED_BYTES: usize = 20 * 1024 * 1024;
const MAX_DIMENSION: u32 = 16_384;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_DECODE_ALLOCATION: u64 = 256 * 1024 * 1024;

pub struct AssetWriteService {
    database: DatabaseConnection,
    storage_root_id: uuid::Uuid,
    #[cfg(not(unix))]
    root: PathBuf,
    #[cfg(unix)]
    root_directory: std::fs::File,
}

impl AssetWriteService {
    /// Creates and pins the content-addressed asset root.
    ///
    /// # Errors
    ///
    /// Returns [`AssetWriteError::Root`] when the root cannot be securely initialized.
    pub async fn new(
        database: DatabaseConnection,
        root: impl AsRef<Path>,
    ) -> Result<Self, AssetWriteError> {
        Self::new_with_override(database, root, false).await
    }

    pub async fn new_environment_override(
        database: DatabaseConnection,
        root: impl AsRef<Path>,
    ) -> Result<Self, AssetWriteError> {
        Self::new_with_override(database, root, true).await
    }

    async fn new_with_override(
        database: DatabaseConnection,
        root: impl AsRef<Path>,
        environment_override: bool,
    ) -> Result<Self, AssetWriteError> {
        tokio::fs::create_dir_all(root.as_ref())
            .await
            .map_err(AssetWriteError::Root)?;
        let root = tokio::fs::canonicalize(root.as_ref())
            .await
            .map_err(AssetWriteError::Root)?;
        let repository = AssetStorageRepository::new(&database);
        let storage_root = if environment_override {
            repository.register_history(&root.to_string_lossy()).await?
        } else {
            repository.activate(&root.to_string_lossy()).await?
        };
        #[cfg(unix)]
        let root_directory = open_root(&root).map_err(AssetWriteError::Root)?;
        Ok(Self {
            database,
            storage_root_id: storage_root.id(),
            #[cfg(not(unix))]
            root,
            #[cfg(unix)]
            root_directory,
        })
    }

    /// Validates, content-addresses, atomically stores, and publishes one original image.
    ///
    /// A database failure can leave only an unreferenced content-addressed file, which is safe for
    /// later GC. The SQL transaction never references a file before its atomic rename completes.
    ///
    /// # Errors
    ///
    /// Returns bounds, format, filesystem, task, or repository failures.
    #[allow(clippy::too_many_arguments)]
    pub async fn store_original(
        &self,
        item_id: CatalogItemId,
        image_type: ImageType,
        priority: u32,
        source_provider: &str,
        source_reference: Option<&str>,
        mime_type: &str,
        bytes: &[u8],
    ) -> Result<AssetWriteResult, AssetWriteError> {
        let prepared = self
            .prepare_original(
                item_id,
                image_type,
                priority,
                source_provider,
                source_reference,
                mime_type,
                bytes,
            )
            .await?;
        let report = AssetRepository::new(&self.database)
            .publish(prepared.publication())
            .await?;
        Ok(AssetWriteResult {
            sha256: prepared.sha256,
            width: prepared.width,
            height: prepared.height,
            reused_blob: report.reused_blob(),
            reference_changed: report.reference_changed(),
        })
    }

    /// Validates and atomically stores one original image without publishing its item reference.
    ///
    /// This is used when the caller must publish the image reference in a larger SQL transaction.
    /// An unreferenced content-addressed file is safe for later GC if that transaction fails.
    ///
    /// # Errors
    ///
    /// Returns bounds, format, filesystem, task, or publication-validation failures.
    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_original(
        &self,
        item_id: CatalogItemId,
        image_type: ImageType,
        priority: u32,
        source_provider: &str,
        source_reference: Option<&str>,
        mime_type: &str,
        bytes: &[u8],
    ) -> Result<PreparedAssetPublication, AssetWriteError> {
        if bytes.is_empty() {
            return Err(AssetWriteError::InvalidBytes);
        }
        if bytes.len() > MAX_ENCODED_BYTES {
            return Err(AssetWriteError::EncodedTooLarge);
        }
        let bytes = bytes.to_vec();
        let mime_type = mime_type.to_owned();
        #[cfg(unix)]
        let root = self
            .root_directory
            .try_clone()
            .map_err(AssetWriteError::File)?;
        #[cfg(not(unix))]
        let root = self.root.clone();
        let prepared =
            tokio::task::spawn_blocking(move || validate_and_store(&root, &mime_type, &bytes))
                .await
                .map_err(|_| AssetWriteError::WriteTask)??;
        let publication = AssetPublication::new(
            item_id,
            image_type,
            priority,
            prepared.sha256.clone(),
            prepared.mime_type.clone(),
            prepared.width,
            prepared.height,
            prepared.byte_size,
            prepared.relative_path,
            source_provider,
            source_reference.map(str::to_owned),
        )?
        .with_storage_root(self.storage_root_id);
        Ok(PreparedAssetPublication {
            publication,
            sha256: prepared.sha256,
            width: prepared.width,
            height: prepared.height,
        })
    }
}

pub struct PreparedAssetPublication {
    publication: AssetPublication,
    sha256: String,
    width: u32,
    height: u32,
}

impl PreparedAssetPublication {
    #[must_use]
    pub const fn publication(&self) -> &AssetPublication {
        &self.publication
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }
}

fn inspect_image(
    mime_type: &str,
    bytes: &[u8],
) -> Result<(ImageFormat, u32, u32), AssetWriteError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| AssetWriteError::InvalidBytes)?;
    let format = reader.format().ok_or(AssetWriteError::InvalidBytes)?;
    if format_mime(format).ok_or(AssetWriteError::UnsupportedFormat)? != mime_type {
        return Err(AssetWriteError::FormatMismatch);
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOCATION);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| AssetWriteError::InvalidBytes)?;
    let (width, height) = decoded.dimensions();
    if width == 0
        || height == 0
        || u64::from(width)
            .checked_mul(u64::from(height))
            .is_none_or(|pixels| pixels > MAX_PIXELS)
    {
        return Err(AssetWriteError::DimensionsTooLarge);
    }
    Ok((format, width, height))
}

fn format_mime(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Jpeg => Some("image/jpeg"),
        ImageFormat::Png => Some("image/png"),
        ImageFormat::Gif => Some("image/gif"),
        ImageFormat::WebP => Some("image/webp"),
        ImageFormat::Bmp => Some("image/bmp"),
        _ => None,
    }
}

fn format_extension(format: ImageFormat) -> Result<&'static str, AssetWriteError> {
    match format {
        ImageFormat::Jpeg => Ok("jpg"),
        ImageFormat::Png => Ok("png"),
        ImageFormat::Gif => Ok("gif"),
        ImageFormat::WebP => Ok("webp"),
        ImageFormat::Bmp => Ok("bmp"),
        _ => Err(AssetWriteError::UnsupportedFormat),
    }
}

#[cfg(unix)]
fn validate_and_store(
    root: &std::fs::File,
    mime_type: &str,
    bytes: &[u8],
) -> Result<PreparedAsset, AssetWriteError> {
    let (format, width, height) = inspect_image(mime_type, bytes)?;
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let extension = format_extension(format)?;
    let prefix = sha256[..2].to_owned();
    let filename = format!("{sha256}.{extension}");
    write_content_addressed(root, &prefix, &filename, bytes)?;
    Ok(PreparedAsset {
        sha256,
        mime_type: mime_type.to_owned(),
        width,
        height,
        byte_size: u64::try_from(bytes.len()).map_err(|_| AssetWriteError::EncodedTooLarge)?,
        relative_path: format!("{prefix}/{filename}"),
    })
}

#[cfg(not(unix))]
fn validate_and_store(
    _root: &Path,
    mime_type: &str,
    bytes: &[u8],
) -> Result<PreparedAsset, AssetWriteError> {
    let _ = inspect_image(mime_type, bytes)?;
    Err(AssetWriteError::UnsupportedPlatform)
}

#[cfg(unix)]
fn open_root(root: &Path) -> Result<std::fs::File, io::Error> {
    use rustix::fs::{Mode, OFlags, open};

    let descriptor = open(
        root,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    Ok(std::fs::File::from(descriptor))
}

#[cfg(unix)]
fn write_content_addressed(
    root: &std::fs::File,
    prefix: &str,
    filename: &str,
    bytes: &[u8],
) -> Result<(), AssetWriteError> {
    use rustix::fs::{AtFlags, Mode, OFlags, mkdirat, openat, renameat, unlinkat};

    if let Err(error) = mkdirat(root, prefix, Mode::from_bits_truncate(0o755))
        && error != rustix::io::Errno::EXIST
    {
        return Err(AssetWriteError::File(error.into()));
    }
    let directory = openat(
        root,
        prefix,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map(std::fs::File::from)
    .map_err(|error| AssetWriteError::File(error.into()))?;
    let temporary = format!(".{}.tmp", uuid::Uuid::new_v4());
    let result = (|| {
        let descriptor = openat(
            &directory,
            temporary.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(io::Error::from)?;
        let mut file = std::fs::File::from(descriptor);
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        renameat(&directory, temporary.as_str(), &directory, filename).map_err(io::Error::from)?;
        directory.sync_all()?;
        Ok::<(), io::Error>(())
    })();
    if result.is_err() {
        let _ = unlinkat(&directory, temporary.as_str(), AtFlags::empty());
    }
    result.map_err(AssetWriteError::File)
}

struct PreparedAsset {
    sha256: String,
    mime_type: String,
    width: u32,
    height: u32,
    byte_size: u64,
    relative_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetWriteResult {
    sha256: String,
    width: u32,
    height: u32,
    reused_blob: bool,
    reference_changed: bool,
}

impl AssetWriteResult {
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn reused_blob(&self) -> bool {
        self.reused_blob
    }

    #[must_use]
    pub const fn reference_changed(&self) -> bool {
        self.reference_changed
    }
}

impl AssetReadService {
    /// Creates the configured asset root and pins its canonical location.
    ///
    /// # Errors
    ///
    /// Returns [`AssetReadError::Root`] when the directory cannot be created or resolved.
    pub async fn new(
        database: DatabaseConnection,
        root: impl AsRef<Path>,
    ) -> Result<Self, AssetReadError> {
        tokio::fs::create_dir_all(root.as_ref())
            .await
            .map_err(AssetReadError::Root)?;
        let root = tokio::fs::canonicalize(root.as_ref())
            .await
            .map_err(AssetReadError::Root)?;
        #[cfg(unix)]
        let root_directory = {
            use rustix::fs::{Mode, OFlags, open};

            let descriptor = open(
                &root,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| AssetReadError::Root(error.into()))?;
            std::fs::File::from(descriptor)
        };
        Ok(Self {
            database,
            fallback_root: root.clone(),
            #[cfg(not(unix))]
            root,
            #[cfg(unix)]
            root_directory,
        })
    }

    /// Opens an original asset after catalog visibility and filesystem confinement checks.
    ///
    /// # Errors
    ///
    /// Returns [`AssetReadError`] for query failures or stored-file integrity violations.
    pub async fn original(
        &self,
        item_id: CatalogItemId,
        image_type: ImageType,
        priority: u32,
    ) -> Result<Option<OpenedAsset>, AssetReadError> {
        let Some(asset) = CatalogQueryRepository::new(&self.database)
            .image(item_id, image_type, priority)
            .await?
        else {
            return Ok(None);
        };
        validate_metadata(asset.sha256(), asset.mime_type())?;
        let relative = Path::new(asset.local_relative_path());
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(AssetReadError::InvalidStoredPath);
        }
        let root = asset
            .storage_root_path()
            .map_or(self.fallback_root.as_path(), Path::new);
        let file = self.open_file(root, relative).await?;
        let metadata = file.metadata().await.map_err(AssetReadError::File)?;
        if !metadata.is_file() {
            return Err(AssetReadError::NotAFile);
        }
        if metadata.len() != asset.byte_size() {
            return Err(AssetReadError::SizeMismatch {
                expected: asset.byte_size(),
                actual: metadata.len(),
            });
        }
        Ok(Some(OpenedAsset {
            file,
            sha256: asset.sha256().to_owned(),
            mime_type: asset.mime_type().to_owned(),
            byte_size: asset.byte_size(),
        }))
    }

    #[cfg(unix)]
    async fn open_file(&self, root_path: &Path, relative: &Path) -> Result<File, AssetReadError> {
        let root = if root_path == self.fallback_root {
            self.root_directory
                .try_clone()
                .map_err(AssetReadError::File)?
        } else {
            open_root(root_path).map_err(AssetReadError::File)?
        };
        let relative = relative.to_owned();
        let file = tokio::task::spawn_blocking(move || open_relative_no_symlinks(&root, &relative))
            .await
            .map_err(|_| AssetReadError::OpenTask)?
            .map_err(map_open_error)?;
        Ok(File::from_std(file))
    }

    #[cfg(not(unix))]
    async fn open_file(&self, _root_path: &Path, _relative: &Path) -> Result<File, AssetReadError> {
        let _ = &self.root;
        Err(AssetReadError::UnsupportedPlatform)
    }
}

#[cfg(unix)]
fn open_relative_no_symlinks(
    root: &std::fs::File,
    relative: &Path,
) -> Result<std::fs::File, io::Error> {
    use rustix::fs::{Mode, OFlags, openat};

    let mut directory = openat(
        root,
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let mut components = relative.components().peekable();
    while let Some(Component::Normal(component)) = components.next() {
        let is_file = components.peek().is_none();
        let flags = if is_file {
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC
        } else {
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
        };
        let opened =
            openat(&directory, component, flags, Mode::empty()).map_err(io::Error::from)?;
        if is_file {
            return Ok(std::fs::File::from(opened));
        }
        directory = opened;
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "asset path has no file component",
    ))
}

#[cfg(unix)]
fn map_open_error(error: io::Error) -> AssetReadError {
    if error.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error()) {
        return AssetReadError::InvalidStoredPath;
    }
    match error.kind() {
        io::ErrorKind::NotFound => AssetReadError::MissingFile,
        io::ErrorKind::NotADirectory => AssetReadError::InvalidStoredPath,
        _ => AssetReadError::File(error),
    }
}

fn validate_metadata(sha256: &str, mime_type: &str) -> Result<(), AssetReadError> {
    if sha256.len() != 64
        || !sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AssetReadError::InvalidSha256);
    }
    if !matches!(
        mime_type,
        "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "image/avif" | "image/bmp"
    ) {
        return Err(AssetReadError::InvalidMimeType);
    }
    Ok(())
}

#[derive(Debug)]
pub struct OpenedAsset {
    file: File,
    sha256: String,
    mime_type: String,
    byte_size: u64,
}

impl OpenedAsset {
    #[must_use]
    pub fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    #[must_use]
    pub fn into_file(self) -> File {
        self.file
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    #[must_use]
    pub const fn byte_size(&self) -> u64 {
        self.byte_size
    }
}

#[derive(Debug, Error)]
pub enum AssetReadError {
    #[error("asset query failed: {0}")]
    Query(#[from] CatalogQueryError),
    #[error("asset root initialization failed: {0}")]
    Root(io::Error),
    #[error("asset metadata contains an invalid sha256")]
    InvalidSha256,
    #[error("asset metadata contains an unsupported MIME type")]
    InvalidMimeType,
    #[error("asset metadata contains an invalid stored path")]
    InvalidStoredPath,
    #[error("asset file is missing")]
    MissingFile,
    #[error("asset path does not reference a regular file")]
    NotAFile,
    #[error("asset size mismatch: expected {expected} bytes, found {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("asset file access failed: {0}")]
    File(io::Error),
    #[error("asset file open task failed")]
    OpenTask,
    #[error("secure asset file access is unsupported on this platform")]
    UnsupportedPlatform,
}

#[derive(Debug, Error)]
pub enum AssetWriteError {
    #[error("asset bytes are empty or not a valid image")]
    InvalidBytes,
    #[error("encoded asset exceeds the configured size bound")]
    EncodedTooLarge,
    #[error("asset format is not supported by the content store")]
    UnsupportedFormat,
    #[error("declared asset MIME type does not match its bytes")]
    FormatMismatch,
    #[error("decoded asset dimensions exceed the configured bounds")]
    DimensionsTooLarge,
    #[error("asset root initialization failed: {0}")]
    Root(io::Error),
    #[error("asset file write failed: {0}")]
    File(io::Error),
    #[error("asset write task failed")]
    WriteTask,
    #[error("secure asset writes are unsupported on this platform")]
    UnsupportedPlatform,
    #[error("asset publication failed: {0}")]
    Repository(#[from] AssetRepositoryError),
    #[error("asset storage configuration failed: {0}")]
    Storage(#[from] AssetStorageError),
}
