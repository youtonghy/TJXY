use sea_orm_migration::{
    prelude::{
        Alias, DbErr, DeriveMigrationName, ForeignKey, Index, MigrationTrait, SchemaManager, Table,
    },
    schema::{blob, integer, timestamp_with_time_zone_null, uuid},
};

const TABLE: &str = "import_sources";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new(TABLE))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("import_job_id")))
                    .col(blob(Alias::new("encrypted_payload")))
                    .col(integer(Alias::new("key_version")))
                    .col(uuid(Alias::new("target_library_id")))
                    .col(uuid(Alias::new("target_user_id")))
                    .col(timestamp_with_time_zone_null(Alias::new("created_at")))
                    .col(timestamp_with_time_zone_null(Alias::new("updated_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_import_sources_job")
                            .from(Alias::new(TABLE), Alias::new("import_job_id"))
                            .to(Alias::new("import_jobs"), Alias::new("id")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_import_sources_library")
                            .from(Alias::new(TABLE), Alias::new("target_library_id"))
                            .to(Alias::new("libraries"), Alias::new("id")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_import_sources_user")
                            .from(Alias::new(TABLE), Alias::new("target_user_id"))
                            .to(Alias::new("users"), Alias::new("id")),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_import_sources_job")
                    .table(Alias::new(TABLE))
                    .col(Alias::new("import_job_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new(TABLE))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
