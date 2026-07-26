use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::{
    prelude::*,
    schema::{
        big_integer, blob, integer, string_len, timestamp_with_time_zone,
        timestamp_with_time_zone_null, uuid,
    },
};

const TOKEN_DIGEST_BYTES: u32 = 32;
const APP_NAME_MAX_BYTES: u32 = 256;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mysql = manager.get_connection().get_database_backend() == DbBackend::MySql;
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("api_keys"))
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .big_integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(uuid(Alias::new("envelope_id")))
                    .col(uuid(Alias::new("creator_user_id")))
                    .col(big_integer(Alias::new("creator_auth_revision")))
                    .col(if mysql {
                        ColumnDef::new(Alias::new("token_digest"))
                            .var_binary(TOKEN_DIGEST_BYTES)
                            .not_null()
                            .take()
                    } else {
                        blob(Alias::new("token_digest"))
                    })
                    .col(blob(Alias::new("encrypted_payload")))
                    .col(integer(Alias::new("key_version")))
                    .col(string_len(Alias::new("app_name"), APP_NAME_MAX_BYTES))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone_null(Alias::new("last_used_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_api_keys_creator")
                            .from(Alias::new("api_keys"), Alias::new("creator_user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        for index in api_key_indexes() {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("api_keys"))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

fn api_key_indexes() -> Vec<IndexCreateStatement> {
    vec![
        Index::create()
            .name("uq_api_keys_envelope_id")
            .table(Alias::new("api_keys"))
            .col(Alias::new("envelope_id"))
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_api_keys_token_digest")
            .table(Alias::new("api_keys"))
            .col(Alias::new("token_digest"))
            .unique()
            .to_owned(),
        Index::create()
            .name("ix_api_keys_creator")
            .table(Alias::new("api_keys"))
            .col(Alias::new("creator_user_id"))
            .to_owned(),
    ]
}
