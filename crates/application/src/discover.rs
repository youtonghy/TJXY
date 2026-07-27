use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_db::{ClaimedWorkJob, DiscoverTitlesError, DiscoverTitlesRepository};

pub struct DiscoverTitlesService {
    database: DatabaseConnection,
}

impl DiscoverTitlesService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Publishes title-layer catalog items using only reconciled SQL inventory.
    ///
    /// # Errors
    ///
    /// Returns a repository error for stale work, invalid classification, or SQL failure.
    pub async fn execute(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<DiscoverTitlesReport, DiscoverTitlesServiceError> {
        let repository = DiscoverTitlesRepository::new(&self.database);
        let snapshot = repository.snapshot(claimed).await?;
        let discovered = snapshot.title_count();
        let generation = repository.publish(claimed, &snapshot).await?;
        Ok(DiscoverTitlesReport {
            discovered,
            generation,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoverTitlesReport {
    discovered: usize,
    generation: i64,
}

impl DiscoverTitlesReport {
    #[must_use]
    pub const fn discovered(self) -> usize {
        self.discovered
    }

    #[must_use]
    pub const fn generation(self) -> i64 {
        self.generation
    }
}

#[derive(Debug, Error)]
pub enum DiscoverTitlesServiceError {
    #[error("title discovery failed: {0}")]
    Repository(#[from] DiscoverTitlesError),
}
