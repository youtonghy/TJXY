//! Provider-neutral storage object and byte-range contracts.

mod backend;
mod types;

pub use backend::{BackendError, ByteStream, StorageBackend};
pub use types::{
    ByteRange, ChangeCursor, ChangePage, IdentityQuality, ObjectPage, ObjectType, PageToken,
    StorageCapabilities, StorageChange, StorageObject, StorageObjectId,
};
