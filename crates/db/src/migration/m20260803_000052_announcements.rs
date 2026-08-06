use sea_orm_migration::{
    prelude::*,
    schema::{
        big_integer, string_len, text, timestamp_with_time_zone, timestamp_with_time_zone_null,
        uuid,
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
                    .table(Alias::new("announcements"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(string_len(Alias::new("title"), 200))
                    .col(text(Alias::new("body_markdown")))
                    .col(string_len(Alias::new("kind"), 16))
                    .col(string_len(Alias::new("status"), 16))
                    .col(big_integer(Alias::new("content_version")))
                    .col(big_integer(Alias::new("revision")))
                    .col(timestamp_with_time_zone_null(Alias::new("published_at")))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_announcements_status_published")
                    .table(Alias::new("announcements"))
                    .col(Alias::new("status"))
                    .col(Alias::new("published_at"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("user_announcement_receipts"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("announcement_id")))
                    .col(uuid(Alias::new("user_id")))
                    .col(big_integer(Alias::new("acknowledged_version")))
                    .col(timestamp_with_time_zone(Alias::new("acknowledged_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_announcement_receipts_announcement")
                            .from(
                                Alias::new("user_announcement_receipts"),
                                Alias::new("announcement_id"),
                            )
                            .to(Alias::new("announcements"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_announcement_receipts_user")
                            .from(
                                Alias::new("user_announcement_receipts"),
                                Alias::new("user_id"),
                            )
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_announcement_receipt_pair")
                    .table(Alias::new("user_announcement_receipts"))
                    .col(Alias::new("announcement_id"))
                    .col(Alias::new("user_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_announcement_receipts_user")
                    .table(Alias::new("user_announcement_receipts"))
                    .col(Alias::new("user_id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("user_announcement_receipts"))
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("announcements"))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
