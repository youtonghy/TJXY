use sea_orm_migration::{
    prelude::{Alias, DbErr, DeriveMigrationName, Index, MigrationTrait, SchemaManager, Table},
    schema::uuid_null,
};

const TABLE: &str = "storage_objects";
const COLUMN: &str = "facts_observed_storage_root_id";
const INDEX: &str = "ix_storage_objects_facts_observed_root_revision";
const ROOT_OBJECTS: &str = "storage_root_objects";
const ROOT_OBJECT_FK_INDEX: &str = "ix_storage_root_objects_object";
const ROOT_OBJECT_LOOKUP_INDEX: &str = "ix_storage_root_objects_object_root";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_index(ROOT_OBJECTS, ROOT_OBJECT_FK_INDEX)
            .await?
        {
            manager
                .create_index(
                    Index::create()
                        .name(ROOT_OBJECT_FK_INDEX)
                        .table(Alias::new(ROOT_OBJECTS))
                        .col(Alias::new("storage_object_id"))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .add_column(uuid_null(Alias::new(COLUMN)))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(INDEX)
                    .table(Alias::new(TABLE))
                    .col(Alias::new(COLUMN))
                    .col(Alias::new("observed_sync_revision"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(ROOT_OBJECT_LOOKUP_INDEX)
                    .table(Alias::new(ROOT_OBJECTS))
                    .col(Alias::new("storage_object_id"))
                    .col(Alias::new("storage_root_id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name(ROOT_OBJECT_LOOKUP_INDEX)
                    .table(Alias::new(ROOT_OBJECTS))
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name(INDEX)
                    .table(Alias::new(TABLE))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .drop_column(Alias::new(COLUMN))
                    .to_owned(),
            )
            .await
    }
}
