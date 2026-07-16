//! TJXY's SQL schema and migration entry point.

mod migration;
mod outbox;
mod user_data;

pub use migration::Migrator;
pub use outbox::{
    ClaimedOutboxEvent, OutboxClock, OutboxCompletion, OutboxFailureReason, OutboxRepository,
    OutboxRepositoryError, SystemClock,
};
pub use user_data::{
    UserDataCommit, UserDataPatch, UserDataRecord, UserDataRepository, UserDataRepositoryError,
};
