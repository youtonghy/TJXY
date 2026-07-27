use sea_orm_migration::{
    prelude::*,
    schema::{json, string_len, timestamp_with_time_zone, uuid},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("display_preferences"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("user_id")))
                    .col(uuid(Alias::new("display_preferences_id")))
                    .col(string_len(Alias::new("client"), 256))
                    .col(json(Alias::new("document")))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_display_preferences_user")
                            .from(Alias::new("display_preferences"), Alias::new("user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_display_preferences_scope")
                    .table(Alias::new("display_preferences"))
                    .col(Alias::new("user_id"))
                    .col(Alias::new("display_preferences_id"))
                    .col(Alias::new("client"))
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("display_preferences"))
                    .to_owned(),
            )
            .await
    }
}
