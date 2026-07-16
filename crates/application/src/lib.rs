//! Application use cases coordinating domain rules and persistence boundaries.

mod auth;
mod catalog;

pub use auth::{
    AuthClock, AuthError, AuthService, ClientIdentity, IssuedAuthentication, SecretSessionToken,
    SessionCapabilities, SystemClock,
};
pub use catalog::{CatalogQueryService, CatalogServiceError};
pub use tjxy_db::{AuthenticatedPrincipal, CatalogItemType, CatalogPageRequest};
