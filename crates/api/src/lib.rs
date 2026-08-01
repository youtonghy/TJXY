//! Jellyfin-compatible HTTP DTO contracts.

mod api_key;
mod auth;
mod browse;
mod device;
mod display_preferences;
mod library;
mod playback;
mod playstate;
mod system;
mod task;

pub use api_key::{AuthenticationInfoDto, AuthenticationInfoQueryResult};
pub use auth::{
    AuthenticateUserByName, AuthenticationResult, CreateUserByName, SessionCapabilitiesDto,
    SessionInfoDto, UpdateUserName, UpdateUserPassword, UpdateUserPolicy, UserConfiguration,
    UserDto, UserPolicy,
};
pub use browse::{
    BaseItemDto, BaseItemDtoQueryResult, BaseItemKind, ClientCapabilitiesDto, CollectionType,
    ItemNamedCodeDto, ItemPersonDto, LocationType, MediaType, SearchHint, SearchHintResult,
    UpdateUserItemDataDto, UserItemDataDto,
};
pub use device::{DeviceInfoDto, DeviceInfoDtoQueryResult, DeviceOptionsDto};
pub use display_preferences::{DisplayPreferencesDto, ScrollDirection, SortOrder};
pub use library::{
    AddVirtualFolderDto, CreateLibraryOptions, LibraryOptionsDto, UpdateLibraryOptions,
    UpdateLibraryOptionsDto, VirtualFolderInfo,
};
pub use playback::{
    DeliveryMethod, MediaProtocol, MediaSourceInfo, MediaStream, MediaStreamType,
    PlaybackInfoError, PlaybackInfoResponse, PlaybackTicketRequest, PlaybackTicketResponse,
};
pub use playstate::PlaybackStateRequest;
pub use system::{BrandingConfiguration, EndpointInfo, PublicSystemInfo};
pub use task::{
    AdminHybridCandidateInfo, AdminHybridCandidatePage, AdminTaskJobInfo, AdminTaskJobStatus,
    ScheduledTaskInfo, ScheduledTaskState,
};
