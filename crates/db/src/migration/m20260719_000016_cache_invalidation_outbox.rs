use sea_orm_migration::{
    prelude::*,
    schema::{
        big_integer, integer, string_len, string_len_null, timestamp_with_time_zone_null, uuid,
    },
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("cache_invalidation_outbox"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(big_integer(Alias::new("generation")))
                    .col(string_len(Alias::new("state"), 32))
                    .col(integer(Alias::new("attempt_count")))
                    .col(string_len_null(Alias::new("lease_owner"), 192))
                    .col(timestamp_with_time_zone_null(Alias::new(
                        "lease_expires_at",
                    )))
                    .col(timestamp_with_time_zone_null(Alias::new("available_at")))
                    .col(timestamp_with_time_zone_null(Alias::new("created_at")))
                    .col(timestamp_with_time_zone_null(Alias::new("processed_at")))
                    .col(string_len_null(Alias::new("last_error"), 256))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_cache_invalidation_generation")
                    .table(Alias::new("cache_invalidation_outbox"))
                    .col(Alias::new("generation"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_cache_invalidation_claim")
                    .table(Alias::new("cache_invalidation_outbox"))
                    .col(Alias::new("state"))
                    .col(Alias::new("available_at"))
                    .col(Alias::new("lease_expires_at"))
                    .col(Alias::new("generation"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("cache_invalidation_outbox"))
                    .to_owned(),
            )
            .await
    }
}
