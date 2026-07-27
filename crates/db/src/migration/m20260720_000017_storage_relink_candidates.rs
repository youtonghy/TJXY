use sea_orm_migration::{
    prelude::*,
    schema::{double, json, string_len, timestamp_with_time_zone, uuid},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("storage_relink_candidates"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("storage_root_id")))
                    .col(uuid(Alias::new("previous_storage_object_id")))
                    .col(uuid(Alias::new("replacement_storage_object_id")))
                    .col(double(Alias::new("confidence")))
                    .col(json(Alias::new("evidence")))
                    .col(string_len(Alias::new("state"), 32))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_relink_candidate_root")
                            .from(
                                Alias::new("storage_relink_candidates"),
                                Alias::new("storage_root_id"),
                            )
                            .to(Alias::new("storage_roots"), Alias::new("id")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_relink_candidate_previous")
                            .from(
                                Alias::new("storage_relink_candidates"),
                                Alias::new("previous_storage_object_id"),
                            )
                            .to(Alias::new("storage_objects"), Alias::new("id")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_relink_candidate_replacement")
                            .from(
                                Alias::new("storage_relink_candidates"),
                                Alias::new("replacement_storage_object_id"),
                            )
                            .to(Alias::new("storage_objects"), Alias::new("id")),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_storage_relink_candidate_pair")
                    .table(Alias::new("storage_relink_candidates"))
                    .col(Alias::new("storage_root_id"))
                    .col(Alias::new("previous_storage_object_id"))
                    .col(Alias::new("replacement_storage_object_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_storage_relink_candidate_queue")
                    .table(Alias::new("storage_relink_candidates"))
                    .col(Alias::new("state"))
                    .col(Alias::new("created_at"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("storage_relink_candidates"))
                    .to_owned(),
            )
            .await
    }
}
