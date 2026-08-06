use sea_orm_migration::{
    prelude::*,
    schema::{
        big_integer, integer, string_len, timestamp_with_time_zone, timestamp_with_time_zone_null,
        uuid, uuid_null,
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
                    .table(Alias::new("installation_records"))
                    .col(uuid(Alias::new("installation_id")).primary_key().take())
                    .col(uuid(Alias::new("server_id")))
                    .col(integer(Alias::new("singleton_key")))
                    .col(string_len(Alias::new("status"), 16))
                    .col(uuid_null(Alias::new("administrator_id")))
                    .col(big_integer(Alias::new("revision")))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone_null(Alias::new("completed_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_installation_administrator")
                            .from(
                                Alias::new("installation_records"),
                                Alias::new("administrator_id"),
                            )
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_installation_singleton")
                    .table(Alias::new("installation_records"))
                    .col(Alias::new("singleton_key"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_installation_server")
                    .table(Alias::new("installation_records"))
                    .col(Alias::new("server_id"))
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("installation_records"))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
