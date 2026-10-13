use sea_orm_migration::prelude::{DbErr, DeriveMigrationName, MigrationTrait, SchemaManager};

/// The 000085 re-resolution selected episode and season art but published none of it:
/// direct image references were written only for items that are library members
/// themselves, and seasons and episodes are members only through their series. Resolve the
/// same items again now that the membership check covers them.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        super::m20261012_000085_reresolve_episode_and_season_art::reresolve_items_with_art(manager)
            .await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // A revision bump only schedules another resolution; there is nothing to restore.
        Ok(())
    }
}
