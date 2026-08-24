use chrono::Utc;
use sea_orm_migration::{
    prelude::*,
    schema::{
        big_integer, big_integer_null, integer, string_len_null, timestamp_with_time_zone,
        timestamp_with_time_zone_null, uuid,
    },
};

const CACHE_STATE: &str = "cache_invalidation_state";
const RETENTION_QUEUE: &str = "work_job_retention_queue";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new(CACHE_STATE))
                    .col(integer(Alias::new("id")).primary_key().take())
                    .col(big_integer(Alias::new("processed_generation")))
                    .col(big_integer_null(Alias::new("target_generation")))
                    .col(integer(Alias::new("attempt_count")))
                    .col(string_len_null(Alias::new("lease_owner"), 192))
                    .col(timestamp_with_time_zone_null(Alias::new(
                        "lease_expires_at",
                    )))
                    .col(timestamp_with_time_zone_null(Alias::new("available_at")))
                    .col(string_len_null(Alias::new("last_error"), 256))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .to_owned(),
            )
            .await?;
        let backend = manager.get_connection().get_database_backend();
        manager
            .get_connection()
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new(CACHE_STATE))
                        .columns([
                            Alias::new("id"),
                            Alias::new("processed_generation"),
                            Alias::new("attempt_count"),
                            Alias::new("updated_at"),
                        ])
                        .values_panic([
                            1_i32.into(),
                            0_i64.into(),
                            0_i32.into(),
                            Utc::now().into(),
                        ]),
                ),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Alias::new(RETENTION_QUEUE))
                    .col(uuid(Alias::new("job_id")).primary_key().take())
                    .col(timestamp_with_time_zone(Alias::new("terminal_at")))
                    .col(integer(Alias::new("attempt_count")))
                    .col(string_len_null(Alias::new("lease_owner"), 192))
                    .col(timestamp_with_time_zone_null(Alias::new(
                        "lease_expires_at",
                    )))
                    .col(timestamp_with_time_zone_null(Alias::new("available_at")))
                    .col(string_len_null(Alias::new("last_error"), 256))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_job_retention_job")
                            .from(Alias::new(RETENTION_QUEUE), Alias::new("job_id"))
                            .to(Alias::new("work_jobs"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_work_job_retention_claim")
                    .table(Alias::new(RETENTION_QUEUE))
                    .col(Alias::new("available_at"))
                    .col(Alias::new("lease_expires_at"))
                    .col(Alias::new("terminal_at"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new(RETENTION_QUEUE)).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Alias::new(CACHE_STATE)).to_owned())
            .await
    }
}
