use sea_orm_migration::{
    prelude::*,
    schema::{big_integer, blob, boolean, integer, string_len, timestamp_with_time_zone, uuid},
};

const PROVIDER_MAX_LENGTH: u32 = 64;
const LANGUAGE_MAX_LENGTH: u32 = 32;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("metadata_provider_settings"))
                    .col(
                        string_len(Alias::new("provider"), PROVIDER_MAX_LENGTH)
                            .primary_key()
                            .take(),
                    )
                    .col(boolean(Alias::new("enabled")))
                    .col(string_len(Alias::new("language"), LANGUAGE_MAX_LENGTH))
                    .col(uuid(Alias::new("credential_id")))
                    .col(blob(Alias::new("encrypted_payload")))
                    .col(integer(Alias::new("key_version")))
                    .col(
                        big_integer(Alias::new("revision"))
                            .check(Expr::col(Alias::new("revision")).gt(0_i64))
                            .take(),
                    )
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("metadata_provider_settings"))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
