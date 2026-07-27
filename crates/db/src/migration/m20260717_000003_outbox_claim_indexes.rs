use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::{
    Alias, DbErr, DeriveMigrationName, Index, MigrationTrait, SchemaManager,
};

const CLAIM_INDEX: &str = "idx_outbox_root_claim";
const REVISION_INDEX: &str = "idx_outbox_root_revision_state";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_connection().get_database_backend() == sea_orm::DbBackend::MySql {
            manager
                .create_index(
                    Index::create()
                        .name("ix_storage_change_outbox_root")
                        .table(Alias::new("storage_change_outbox"))
                        .col(Alias::new("storage_root_id"))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .create_index(
                Index::create()
                    .name(CLAIM_INDEX)
                    .table(Alias::new("storage_change_outbox"))
                    .col(Alias::new("storage_root_id"))
                    .col(Alias::new("sync_revision"))
                    .col(Alias::new("state"))
                    .col(Alias::new("available_at"))
                    .col(Alias::new("lease_expires_at"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(REVISION_INDEX)
                    .table(Alias::new("storage_change_outbox"))
                    .col(Alias::new("storage_root_id"))
                    .col(Alias::new("sync_revision"))
                    .col(Alias::new("state"))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for index in [REVISION_INDEX, CLAIM_INDEX] {
            manager
                .drop_index(
                    Index::drop()
                        .name(index)
                        .table(Alias::new("storage_change_outbox"))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
