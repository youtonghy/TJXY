use sea_orm_migration::{prelude::*, schema::timestamp_with_time_zone_null};

/// Superseded-metadata purging re-read every pending metadata job on each five-second
/// maintenance pass. This records when it is next due, so an idle backlog is checked about once
/// a minute while a draining one still runs every pass.
const TABLE: &str = "work_maintenance_state";
const COLUMN: &str = "purge_available_at";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .add_column(timestamp_with_time_zone_null(Alias::new(COLUMN)))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .drop_column(Alias::new(COLUMN))
                    .to_owned(),
            )
            .await
    }
}
