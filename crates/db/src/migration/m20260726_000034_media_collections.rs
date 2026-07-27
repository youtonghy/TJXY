use sea_orm_migration::{
    prelude::*,
    schema::{big_integer, string_len, timestamp_with_time_zone, uuid, uuid_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("media_collections"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(string_len(Alias::new("kind"), 16))
                    .col(uuid_null(Alias::new("owner_user_id")))
                    .col(string_len(Alias::new("name"), 256))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_media_collections_owner")
                            .from(Alias::new("media_collections"), Alias::new("owner_user_id"))
                            .to(Alias::new("users"), Alias::new("id")),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("media_collection_entries"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("media_collection_id")))
                    .col(uuid(Alias::new("catalog_item_id")))
                    .col(big_integer(Alias::new("position")))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_media_collection_entries_collection")
                            .from(
                                Alias::new("media_collection_entries"),
                                Alias::new("media_collection_id"),
                            )
                            .to(Alias::new("media_collections"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_media_collection_entries_item")
                            .from(
                                Alias::new("media_collection_entries"),
                                Alias::new("catalog_item_id"),
                            )
                            .to(Alias::new("catalog_items"), Alias::new("id")),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_media_collections_owner_kind_name")
                    .table(Alias::new("media_collections"))
                    .col(Alias::new("owner_user_id"))
                    .col(Alias::new("kind"))
                    .col(Alias::new("name"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_media_collection_entries_position")
                    .table(Alias::new("media_collection_entries"))
                    .col(Alias::new("media_collection_id"))
                    .col(Alias::new("position"))
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("media_collection_entries"))
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("media_collections"))
                    .to_owned(),
            )
            .await
    }
}
