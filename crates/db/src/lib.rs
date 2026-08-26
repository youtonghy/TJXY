//! TJXY's SQL schema and migration entry point.

mod ai;
mod ai_usage;
mod announcement;
mod api_key;
mod asset;
mod asset_storage;
mod auth;
mod cache_invalidation;
mod catalog_publication;
mod catalog_query;
mod catalog_storage_scope;
mod catalog_visibility;
mod dashboard;
mod demo_catalog;
mod device;
mod direct_metadata;
mod discover;
mod display_preferences;
mod full_scan;
mod hybrid_candidate;
mod import;
mod import_publication;
mod import_runtime;
mod installation;
mod library;
mod logging_settings;
mod manual_probe;
mod media_collection;
mod metadata;
mod metadata_provider_settings;
mod metadata_work;
mod migration;
mod natural_key;
mod outbox;
mod passkey;
mod playback_ticket;
mod playstate;
mod probe;
mod queue_maintenance;
mod series_expand;
mod site_theme_settings;
mod source_index;
mod source_publication;
mod storage_account;
mod storage_binding;
mod storage_change_feed;
mod storage_change_projection;
mod storage_credential;
mod storage_path_authorization;
mod storage_relink;
mod storage_sync;
mod system_settings;
mod title_year;
mod user_data;
mod work_job;
mod work_retention;

pub use ai::{
    AI_PROVIDER_KEY, AiConversationRecord, AiConversationRepository, AiConversationRepositoryError,
    AiMessageRecord, AiModelInput, AiModelRecord, AiReasoningEffort, AiSettingsRecord,
    AiSettingsRepository, AiSettingsRepositoryError,
};
pub use ai_usage::{
    AiExecutionInput, AiExecutionOutcome, AiUsageAnalytics, AiUsageDaily, AiUsageFailure,
    AiUsageModel, AiUsageRepository, AiUsageRepositoryError, AiUsageSummary, AiUsageUser,
};
pub use announcement::{
    AnnouncementDraftInput, AnnouncementKind, AnnouncementList, AnnouncementRecord,
    AnnouncementRepository, AnnouncementRepositoryError, AnnouncementStatus, AnnouncementView,
};
pub use api_key::{ApiKeyDraft, ApiKeyRepository, ApiKeyRepositoryError, StoredApiKey};
pub use asset::{AssetPublication, AssetPublicationReport, AssetRepository, AssetRepositoryError};
pub use asset_storage::{AssetStorageError, AssetStorageRepository, AssetStorageRoot};
pub use auth::{
    AuthRepository, AuthRepositoryError, AuthSessionQuery, AuthSessionRecord, AuthUser,
    AuthenticatedPrincipal, AuthenticationOrigin, CredentialSnapshot, IssuedSession,
    SessionCapabilitiesDraft, SessionDraft,
};
pub use cache_invalidation::{
    CacheInvalidationClock, CacheInvalidationRepository, CacheInvalidationRepositoryError,
    CacheInvalidationSystemClock, ClaimedCacheInvalidation, advance_catalog_generation,
};
pub use catalog_publication::{
    CatalogPublicationError, CatalogPublicationRepository, StructurePublicationManifest,
    StructurePublicationRow,
};
pub use catalog_query::{
    AssetRecord, BrowseParent, CacheRevisions, CatalogCreditRecord, CatalogFilterFacets,
    CatalogItemDetailRecord, CatalogItemRecord, CatalogItemType, CatalogItemsQuery,
    CatalogItemsScope, CatalogNamedCodeRecord, CatalogPage, CatalogPageRequest, CatalogQueryError,
    CatalogQueryRepository, CatalogSort, CatalogSortField, CatalogSortOrder, LatestItemRecord,
    LazyCatalogWorkTarget, LazyStorageScope, LibraryViewRecord,
};
pub use catalog_visibility::catalog_item_visibility_condition;
pub use dashboard::{
    DashboardLoginRecord, DashboardNowPlaying, DashboardPage, DashboardPlaybackEvent,
    DashboardRepository, DashboardSnapshot, DashboardTopItem, DashboardWatchRecord,
};
pub use demo_catalog::{
    DemoCatalogPublication, DemoCatalogPublicationError, DemoCatalogPublicationReport,
    DemoCatalogRepository, demo_catalog_item_id,
};
pub use device::{DeviceOptionsRecord, DeviceRecord, DeviceRepository, DeviceRepositoryError};
pub use direct_metadata::{DirectMetadataObjectRecord, DirectMetadataRepository};
#[doc(hidden)]
pub use discover::enqueue_after_root_sync as enqueue_discovery_after_root_sync;
pub use discover::{
    DiscoverTitlesError, DiscoverTitlesRepository, DiscoverTitlesSnapshot, DiscoveredTitle,
};
pub use display_preferences::{DisplayPreferencesRepository, DisplayPreferencesRepositoryError};
pub use full_scan::{FullScanPolicy, FullScanRepository, FullScanRepositoryError, FullScanRoot};
pub use hybrid_candidate::{
    HybridCandidateError, HybridCandidateMutation, HybridCandidatePage, HybridCandidateRecord,
    HybridCandidateRepository,
};
pub use import::{
    ClaimedImportJob, ImportJobRecord, ImportJobRepository, ImportJobState, ImportStagingCommit,
    ImportStagingItem, ImportStagingRepositoryError,
};
pub use import_publication::{
    ImportPublicationError, ImportPublicationReport, ImportPublicationRepository,
    ImportPublicationTarget,
};
pub use import_runtime::{
    CreatedImportRuntime, ImportRuntimeDraft, ImportRuntimeRepository,
    ImportRuntimeRepositoryError, ImportSourceRecord,
};
pub use installation::{
    InstallationRecord, InstallationRepository, InstallationRepositoryError, InstallationStatus,
};
pub use library::{
    CreatedFilesystemLibrary, DisabledStorageRuntime, FilesystemRootConfiguration,
    FilesystemRootDraft, LibraryPolicyUpdate, LibraryRepository, LibraryRepositoryError,
    VirtualFolderRecord, VirtualFolderRoot,
};
pub use logging_settings::{
    DEFAULT_LOG_RETENTION_DAYS, LogMode, LoggingSettingsInput, LoggingSettingsRecord,
    LoggingSettingsRepository, LoggingSettingsRepositoryError,
};
pub use manual_probe::{ManualProbeError, ManualProbeRepository, ManualProbeSubmission};
pub use media_collection::{
    MediaCollectionCatalogItem, MediaCollectionEntry, MediaCollectionKind, MediaCollectionRecord,
    MediaCollectionRepository, MediaCollectionRepositoryError,
};
pub use metadata::{
    MetadataPublicationError, MetadataPublicationReport, MetadataPublicationRepository,
};
pub use metadata_provider_settings::{
    MetadataProviderSettingRecord, MetadataProviderSettingsRepository,
    MetadataProviderSettingsRepositoryError,
};
pub use metadata_work::{
    MetadataImageCandidate, MetadataSidecarCandidate, MetadataWorkError, MetadataWorkRepository,
    MetadataWorkSnapshot,
};
pub use migration::{Migrator, SchemaMigrationError, migrate_database};
pub use outbox::{
    BackloggedStorageRoot, ClaimedOutboxEvent, OutboxClock, OutboxCompletion,
    OutboxFailureDisposition, OutboxFailureReason, OutboxRepository, OutboxRepositoryError,
    SystemClock,
};
pub use passkey::{PasskeyChallenge, PasskeyCredential, PasskeyRepository, PasskeyRepositoryError};
pub use playback_ticket::{
    PlaybackTicketDraft, PlaybackTicketGrant, PlaybackTicketRepository,
    PlaybackTicketRepositoryError,
};
pub use playstate::{PlaybackSessionCommit, PlaystateRepository, PlaystateRepositoryError};
pub use probe::{ProbeCandidate, ProbeRepository, ProbeRepositoryError, ProbeResult, ProbedStream};
pub use queue_maintenance::{
    QueueMaintenanceError, QueueMaintenanceRepository, QueueMaintenanceRun,
};
pub use series_expand::{
    SeriesExpandRepository, SeriesExpandRepositoryError, SeriesExpandSnapshot, SeriesStorageObject,
};
pub use site_theme_settings::{
    DEFAULT_SITE_THEME_ID, DEFAULT_SITE_THEME_SCHEMA_VERSION, SiteThemeConfiguration,
    SiteThemeSelectionInput, SiteThemeSettingsRecord, SiteThemeSettingsRepository,
    SiteThemeSettingsRepositoryError,
};
pub use source_index::{
    SourceIndexObject, SourceIndexRepository, SourceIndexRepositoryError, SourceIndexSnapshot,
};
pub use source_publication::{
    MediaLocationPublicationRow, MediaSourcePublicationRow, PlaybackLocation,
    PlaybackSubtitleLocation, PublishedMediaLocation, PublishedMediaSource, PublishedMediaStream,
    PublishedSubtitle, SeriesSourcePublication, SourcePlaybackPolicy, SourcePlaybackPolicyError,
    SourcePublicationManifest, SubtitlePublicationRow,
};
pub use storage_account::{
    StorageAccountBinding, StorageAccountRepository, StorageAccountRepositoryError,
};
pub use storage_binding::{
    CreatedStorageBinding, StorageBindingDraft, StorageBindingRepository,
    StorageBindingRepositoryError,
};
pub use storage_change_feed::{
    CommittedChangePage, StorageChangeFeedRepository, StorageChangeFeedRepositoryError,
    activate_storage_cursor_recovery, fail_storage_cursor_recovery,
};
pub use storage_change_projection::{
    StorageChangeProjectionError, StorageChangeProjectionRepository,
};
pub use storage_credential::{
    CredentialRefreshState, StorageCredentialRecord, StorageCredentialRepository,
    StorageCredentialRepositoryError,
};
pub use storage_relink::{
    StorageRelinkCandidate, StorageRelinkDecision, StorageRelinkDecisionReport,
    StorageRelinkRepository, StorageRelinkRepositoryError,
};
pub use storage_sync::{
    CommittedStoragePage, ObjectAvailabilityUpdate, ScopedInventoryTarget, StorageSyncPage,
    StorageSyncRepository, StorageSyncRepositoryError, TemporaryAvailabilityReason,
};
pub use system_settings::{
    DEFAULT_ICON_URL, DEFAULT_LISTEN_HOST, DEFAULT_LOGO_URL, DEFAULT_PORT, DEFAULT_SITE_SUBTITLE,
    DEFAULT_SITE_TITLE, DEFAULT_SYSTEM_LOCALE, SystemSettingsInput, SystemSettingsRecord,
    SystemSettingsRepository, SystemSettingsRepositoryError,
};
pub use user_data::{
    UserDataCommit, UserDataPatch, UserDataRecord, UserDataRepository, UserDataRepositoryError,
};
pub use work_job::{
    ADMIN_CANCELLED_ERROR, ClaimedWorkJob, FullScanChildSubmission, MetadataRequirement,
    WorkJobAdminOutcome, WorkJobAdminRecord, WorkJobAdminStatus, WorkJobClock, WorkJobRecord,
    WorkJobRepository, WorkJobRepositoryError, WorkJobResult, WorkJobSpec, WorkJobState,
    WorkJobSubmission, WorkJobSystemClock, WorkScope, WorkStagingRow, WorkTaskKind,
};
pub use work_retention::{WorkRetentionError, WorkRetentionRepository, WorkRetentionRun};
