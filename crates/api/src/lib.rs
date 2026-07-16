//! Jellyfin-compatible HTTP DTO contracts.

mod playback;

pub use playback::{
    DeliveryMethod, MediaProtocol, MediaSourceInfo, MediaStream, MediaStreamType,
    PlaybackInfoError, PlaybackInfoResponse,
};
