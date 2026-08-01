//! Application use cases coordinating domain rules and persistence boundaries.

mod api_key;
mod asset;
mod auth;
mod cache_invalidation;
mod catalog;
mod device_profile;
mod discover;
mod display_preferences;
mod filesystem_browser;
mod full_scan;
mod library;
mod media;
mod media_collection;
mod metadata;
mod playback_ticket;
mod playstate;
mod probe;
mod series_expand;
mod source_index;
mod storage_backend_registry;
mod storage_change;
mod storage_change_feed;
mod storage_read;
mod storage_sync;
mod storage_validate;
mod task;
mod user_data;

pub use api_key::{ApiKeyInfo, SecretApiKey, valid_api_key_app_name};
pub use asset::{
    AssetReadError, AssetReadService, AssetWriteError, AssetWriteResult, AssetWriteService,
    OpenedAsset, PreparedAssetPublication,
};
pub use auth::{
    AuthClock, AuthError, AuthService, ClientIdentity, IssuedAuthentication, SecretSessionToken,
    SessionCapabilities, SessionListFilter, SystemClock,
};
pub use cache_invalidation::{
    CacheInvalidationRun, CacheInvalidationService, CacheInvalidationServiceError,
};
pub use catalog::{
    CatalogQueryService, CatalogServiceError, PlaybackSource, PlaybackStream, PlaybackSubtitle,
};
pub use device_profile::DeviceProfile;
pub use discover::{DiscoverTitlesReport, DiscoverTitlesService, DiscoverTitlesServiceError};
pub use display_preferences::{DisplayPreferencesService, DisplayPreferencesServiceError};
pub use filesystem_browser::{
    FilesystemBrowser, FilesystemBrowserError, FilesystemBrowserRoot, FilesystemDirectoryEntry,
    FilesystemDirectoryPage, ResolvedFilesystemDirectory,
};
pub use full_scan::{FullScanError, FullScanResult, FullScanService};
pub use library::{LibraryPolicyOverrides, LibraryService, LibraryServiceError};
pub use media::{
    MediaReadError, MediaReadService, OpenedMediaRange, ResolvedMedia, ResolvedSubtitle,
};
pub use media_collection::{MediaCollectionService, MediaCollectionServiceError};
pub use metadata::{
    MetadataImageBytes, MetadataImageFetchError, MetadataImageFetcher, MetadataImportError,
    MetadataImportReport, MetadataImportService, MetadataResolveError, MetadataResolveReport,
    MetadataResolveService, ReqwestMetadataImageFetcher,
};
pub use playback_ticket::{
    IssuedPlaybackTicket, PlaybackTicketService, PlaybackTicketServiceError, SecretPlaybackTicket,
};
pub use playstate::{PlaybackEvent, PlaystateService, PlaystateServiceError};
pub use probe::{
    DefaultMediaInspector, MatroskaInspector, MediaInspector, ProbeInput, ProbeService,
    ProbeServiceError,
};
pub use series_expand::{SeriesExpandError, SeriesExpandService};
pub use source_index::{SourceIndexError, SourceIndexService};
pub use storage_backend_registry::{StorageBackendRegistry, StorageBackendRegistryError};
pub use storage_change::{
    StorageChangeProjector, StorageChangeProjectorError, StorageChangeReconcileFailure,
    StorageChangeReconcileReport, StorageChangeReconciler, StorageChangeReconcilerError,
};
pub use storage_change_feed::{
    StorageChangeFeedError, StorageChangeFeedResult, StorageChangeFeedService,
};
pub use storage_sync::{ScopedInventoryError, ScopedInventoryResult, ScopedInventoryService};
pub use storage_validate::{
    FullValidateStorageError, FullValidateStorageResult, FullValidateStorageService,
};
pub use task::{TaskService, TaskServiceError};
pub use tjxy_db::{AuthSessionRecord, DeviceOptionsRecord, DeviceRecord};
pub use tjxy_db::{
    AuthenticatedPrincipal, CatalogItemType, CatalogItemsQuery, CatalogItemsScope,
    CatalogPageRequest, CatalogSort, CatalogSortField, CatalogSortOrder,
};
pub use user_data::{UserDataService, UserDataServiceError};
