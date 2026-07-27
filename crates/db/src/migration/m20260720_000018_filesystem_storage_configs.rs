use sea_orm_migration::{
    prelude::*,
    schema::{string_len, uuid},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mysql = manager.get_connection().get_database_backend() == sea_orm::DbBackend::MySql;
        let mut table = Table::create();
        table
            .table(Alias::new("filesystem_storage_configs"))
            .col(uuid(Alias::new("storage_account_id")).primary_key().take())
            .col(string_len(Alias::new("root_path"), 4096));
        if mysql {
            table.col(string_len(Alias::new("root_path_key"), 64));
        }
        table.foreign_key(
            ForeignKey::create()
                .name("fk_filesystem_config_account")
                .from(
                    Alias::new("filesystem_storage_configs"),
                    Alias::new("storage_account_id"),
                )
                .to(Alias::new("storage_accounts"), Alias::new("id")),
        );
        manager.create_table(table.clone()).await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_filesystem_config_root_path")
                    .table(Alias::new("filesystem_storage_configs"))
                    .col(Alias::new(if mysql {
                        "root_path_key"
                    } else {
                        "root_path"
                    }))
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("filesystem_storage_configs"))
                    .to_owned(),
            )
            .await
    }
}
