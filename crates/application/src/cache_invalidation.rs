use std::sync::Arc;

use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_cache::{CacheInvalidationFailureKind, CacheInvalidationOutcome, CacheInvalidator};
use tjxy_db::{CacheInvalidationRepository, CacheInvalidationRepositoryError};
use uuid::Uuid;

pub struct CacheInvalidationService {
    database: DatabaseConnection,
    invalidator: Arc<dyn CacheInvalidator>,
    owner: String,
}

impl CacheInvalidationService {
    #[must_use]
    pub fn new(database: DatabaseConnection, invalidator: Arc<dyn CacheInvalidator>) -> Self {
        Self {
            database,
            invalidator,
            owner: format!("cache-invalidation-{}", Uuid::new_v4()),
        }
    }

    /// Claims and processes at most one durable cache invalidation.
    ///
    /// Redis failures are durably deferred and returned as [`CacheInvalidationRun::Deferred`].
    ///
    /// # Errors
    ///
    /// Returns a repository error when claim, completion, or retry persistence fails.
    pub async fn run_once(&self) -> Result<CacheInvalidationRun, CacheInvalidationServiceError> {
        let repository = CacheInvalidationRepository::new(&self.database);
        let Some(claimed) = repository
            .claim_next(&self.owner, chrono::Duration::seconds(30))
            .await?
        else {
            return Ok(CacheInvalidationRun::Idle);
        };
        match self
            .invalidator
            .invalidate_generation(claimed.stale_generation())
            .await
        {
            Ok(CacheInvalidationOutcome::Deleted { count, remaining }) if remaining > 0 => {
                repository.release(&claimed).await?;
                Ok(CacheInvalidationRun::Progressed {
                    generation: claimed.generation(),
                    deleted: count,
                    remaining,
                })
            }
            Ok(outcome) => {
                repository.complete(&claimed).await?;
                Ok(CacheInvalidationRun::Completed {
                    generation: claimed.generation(),
                    outcome,
                })
            }
            Err(error) => {
                let failure = error.kind();
                repository
                    .fail(
                        &claimed,
                        retry_delay(claimed.attempt_count()),
                        &failure.to_string(),
                    )
                    .await?;
                Ok(CacheInvalidationRun::Deferred {
                    generation: claimed.generation(),
                    failure,
                })
            }
        }
    }
}

fn retry_delay(attempt_count: i32) -> chrono::Duration {
    let exponent = u32::try_from(attempt_count).unwrap_or_default().min(6);
    chrono::Duration::seconds((5_i64 * (1_i64 << exponent)).min(300))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheInvalidationRun {
    Idle,
    Completed {
        generation: i64,
        outcome: CacheInvalidationOutcome,
    },
    Progressed {
        generation: i64,
        deleted: usize,
        remaining: usize,
    },
    Deferred {
        generation: i64,
        failure: CacheInvalidationFailureKind,
    },
}

#[derive(Debug, Error)]
pub enum CacheInvalidationServiceError {
    #[error("cache invalidation repository failed: {0}")]
    Repository(#[from] CacheInvalidationRepositoryError),
}
