//! TJXY's SQL schema and migration entry point.

mod auth;
mod catalog_query;
mod migration;
mod outbox;
mod user_data;

pub use auth::{
    AuthRepository, AuthRepositoryError, AuthUser, AuthenticatedPrincipal, CredentialSnapshot,
    IssuedSession, SessionDraft,
};
pub use catalog_query::{
    BrowseParent, CatalogItemRecord, CatalogPage, CatalogPageRequest, CatalogQueryError,
    CatalogQueryRepository, LibraryViewRecord,
};
pub use migration::Migrator;
pub use outbox::{
    ClaimedOutboxEvent, OutboxClock, OutboxCompletion, OutboxFailureReason, OutboxRepository,
    OutboxRepositoryError, SystemClock,
};
pub use user_data::{
    UserDataCommit, UserDataPatch, UserDataRecord, UserDataRepository, UserDataRepositoryError,
};
