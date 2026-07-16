//! Application use cases coordinating domain rules and persistence boundaries.

mod auth;

pub use auth::{
    AuthClock, AuthError, AuthService, ClientIdentity, IssuedAuthentication, SecretSessionToken,
    SystemClock,
};
pub use tjxy_db::AuthenticatedPrincipal;
