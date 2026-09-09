use sea_orm_migration::{
    prelude::*,
    schema::{
        big_integer, json, string_len, string_len_null, timestamp_with_time_zone, uuid, uuid_null,
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
                    .table(Alias::new("nfo_choices"))
                    .col(uuid(Alias::new("catalog_item_id")))
                    .col(uuid(Alias::new("storage_root_id")))
                    .col(string_len(Alias::new("fingerprint"), 64))
                    .col(big_integer(Alias::new("metadata_revision")))
                    .col(big_integer(Alias::new("input_sync_revision")))
                    .col(json(Alias::new("candidates")))
                    .col(json(Alias::new("conflict_fields")))
                    .col(uuid_null(Alias::new("selected_object_id")))
                    .col(string_len_null(Alias::new("selected_digest"), 64))
                    .col(string_len(Alias::new("status"), 32))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .primary_key(
                        Index::create()
                            .col(Alias::new("catalog_item_id"))
                            .col(Alias::new("storage_root_id")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Alias::new("nfo_choices"), Alias::new("catalog_item_id"))
                            .to(Alias::new("catalog_items"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Alias::new("nfo_choices"), Alias::new("storage_root_id"))
                            .to(Alias::new("storage_roots"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("nfo_choices")).to_owned())
            .await
    }
}
