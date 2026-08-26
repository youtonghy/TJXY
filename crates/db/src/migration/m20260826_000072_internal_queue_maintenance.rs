use sea_orm_migration::{
    prelude::*,
    schema::{big_integer_null, string_len_null, timestamp_with_time_zone_null},
};

const STORAGE_OUTBOX: &str = "storage_change_outbox";
const STORAGE_ROOTS: &str = "storage_roots";
const CLEANUP_INDEX: &str = "ix_storage_change_outbox_cleanup";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(STORAGE_OUTBOX))
                    .add_column(timestamp_with_time_zone_null(Alias::new(
                        "dead_lettered_at",
                    )))
                    .to_owned(),
            )
            .await?;
        for column in [
            timestamp_with_time_zone_null(Alias::new("outbox_degraded_at")),
            big_integer_null(Alias::new("outbox_degraded_revision")),
            string_len_null(Alias::new("outbox_degraded_reason"), 64),
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(STORAGE_ROOTS))
                        .add_column(column)
                        .to_owned(),
                )
                .await?;
        }
        manager
            .create_index(
                Index::create()
                    .name(CLEANUP_INDEX)
                    .table(Alias::new(STORAGE_OUTBOX))
                    .col(Alias::new("state"))
                    .col(Alias::new("dead_lettered_at"))
                    .col(Alias::new("processed_at"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name(CLEANUP_INDEX)
                    .table(Alias::new(STORAGE_OUTBOX))
                    .to_owned(),
            )
            .await?;
        for column in [
            "outbox_degraded_reason",
            "outbox_degraded_revision",
            "outbox_degraded_at",
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(STORAGE_ROOTS))
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(STORAGE_OUTBOX))
                    .drop_column(Alias::new("dead_lettered_at"))
                    .to_owned(),
            )
            .await
    }
}
