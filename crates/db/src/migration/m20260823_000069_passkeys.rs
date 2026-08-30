use sea_orm::DbErr;
use sea_orm_migration::prelude::{
    Alias, ColumnDef, DeriveMigrationName, ForeignKey, ForeignKeyAction, MigrationTrait,
    SchemaManager, Table,
};
use sea_orm_migration::schema::{blob, string_len, timestamp_with_time_zone, uuid};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .add_column(
                        ColumnDef::new(Alias::new("passkey_enabled"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Alias::new("passkey_credentials"))
                    .if_not_exists()
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("user_id")).not_null())
                    .col(string_len(Alias::new("credential_id"), 1024))
                    .col(blob(Alias::new("public_key")).not_null())
                    .col(
                        ColumnDef::new(Alias::new("counter"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(string_len(Alias::new("name"), 128))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("last_used_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .from(Alias::new("passkey_credentials"), Alias::new("user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                sea_orm_migration::prelude::Index::create()
                    .name("ux_passkey_credentials_credential_id")
                    .table(Alias::new("passkey_credentials"))
                    .col(Alias::new("credential_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Alias::new("passkey_challenges"))
                    .if_not_exists()
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(ColumnDef::new(Alias::new("user_id")).uuid().null())
                    .col(string_len(Alias::new("kind"), 16))
                    .col(blob(Alias::new("state")).not_null())
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("expires_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .from(Alias::new("passkey_challenges"), Alias::new("user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                sea_orm_migration::prelude::Index::create()
                    .name("idx_passkey_challenges_expires_at")
                    .table(Alias::new("passkey_challenges"))
                    .col(Alias::new("expires_at"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("passkey_challenges"))
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("passkey_credentials"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .drop_column(Alias::new("passkey_enabled"))
                    .to_owned(),
            )
            .await
    }
}
