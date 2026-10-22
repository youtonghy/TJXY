use sea_orm_migration::{
    prelude::*,
    schema::{big_integer, string_len, text, timestamp_with_time_zone, uuid, uuid_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("client_releases"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(string_len(Alias::new("platform"), 16))
                    .col(string_len(Alias::new("arch"), 16))
                    .col(string_len(Alias::new("version"), 64))
                    .col(string_len(Alias::new("file_name"), 255))
                    .col(string_len(Alias::new("stored_name"), 255))
                    .col(big_integer(Alias::new("size_bytes")))
                    .col(string_len(Alias::new("sha256"), 64))
                    .col(text(Alias::new("notes")))
                    .col(
                        ColumnDef::new(Alias::new("is_latest"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(uuid_null(Alias::new("created_by")))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_client_releases_created_by")
                            .from(Alias::new("client_releases"), Alias::new("created_by"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_client_releases_target_version")
                    .table(Alias::new("client_releases"))
                    .col(Alias::new("platform"))
                    .col(Alias::new("arch"))
                    .col(Alias::new("version"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_client_releases_latest")
                    .table(Alias::new("client_releases"))
                    .col(Alias::new("platform"))
                    .col(Alias::new("is_latest"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("client_releases"))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
