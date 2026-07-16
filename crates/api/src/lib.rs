//! Jellyfin-compatible HTTP DTO contracts.

mod auth;
mod browse;
mod playback;
mod system;

pub use auth::{
    AuthenticateUserByName, AuthenticationResult, SessionInfoDto, UserConfiguration, UserDto,
    UserPolicy,
};
pub use browse::{
    BaseItemDto, BaseItemDtoQueryResult, BaseItemKind, ClientCapabilitiesDto, CollectionType,
    MediaType, UserItemDataDto,
};
pub use playback::{
    DeliveryMethod, MediaProtocol, MediaSourceInfo, MediaStream, MediaStreamType,
    PlaybackInfoError, PlaybackInfoResponse,
};
pub use system::PublicSystemInfo;
