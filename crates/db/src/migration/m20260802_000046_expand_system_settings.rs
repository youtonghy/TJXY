use sea_orm_migration::{
    prelude::*,
    schema::{integer, string_len, string_len_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .add_column(string_len(Alias::new("site_title"), 120).default("TJXY"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .add_column(
                        string_len(Alias::new("site_subtitle"), 240).default("Your media library"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .add_column(
                        string_len(Alias::new("logo_url"), 2048).default("/brand/tjxy-mark.webp"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .add_column(
                        string_len(Alias::new("icon_url"), 2048).default("/brand/favicon.svg"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .add_column(string_len_null(Alias::new("public_url"), 2048))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .add_column(string_len(Alias::new("listen_host"), 64).default("127.0.0.1"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("system_settings"))
                    .add_column(integer(Alias::new("port")).default(8096))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            "port",
            "listen_host",
            "public_url",
            "icon_url",
            "logo_url",
            "site_subtitle",
            "site_title",
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("system_settings"))
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
