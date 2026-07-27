use sea_orm_migration::{
    prelude::{
        Alias, DbErr, DeriveMigrationName, ForeignKey, Index, IndexCreateStatement, MigrationTrait,
        SchemaManager, Table, TableCreateStatement,
    },
    schema::{
        big_integer, boolean, string, string_len, string_null, timestamp_with_time_zone_null, uuid,
        uuid_null,
    },
};

const ROOT_OBJECTS: &str = "storage_root_objects";
const SYNC_PAGES: &str = "storage_sync_pages";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(root_objects_table()).await?;
        for index in root_object_indexes() {
            manager.create_index(index).await?;
        }
        manager.create_table(sync_pages_table()).await?;
        manager.create_index(sync_pages_index()).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [SYNC_PAGES, ROOT_OBJECTS] {
            manager
                .drop_table(
                    Table::drop()
                        .table(Alias::new(table))
                        .if_exists()
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

fn root_objects_table() -> TableCreateStatement {
    Table::create()
        .table(Alias::new(ROOT_OBJECTS))
        .col(uuid(Alias::new("id")).primary_key().take())
        .col(uuid(Alias::new("storage_root_id")))
        .col(uuid(Alias::new("storage_object_id")))
        .col(uuid_null(Alias::new("parent_storage_object_id")))
        .col(big_integer(Alias::new("observed_sync_revision")))
        .col(boolean(Alias::new("children_indexed")))
        .col(big_integer(Alias::new("children_index_revision")))
        .col(string(Alias::new("presence_state")))
        .col(string_null(Alias::new("availability_reason")))
        .col(timestamp_with_time_zone_null(Alias::new("last_listed_at")))
        .foreign_key(
            ForeignKey::create()
                .name("fk_storage_root_objects_root")
                .from(Alias::new(ROOT_OBJECTS), Alias::new("storage_root_id"))
                .to(Alias::new("storage_roots"), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_storage_root_objects_object")
                .from(Alias::new(ROOT_OBJECTS), Alias::new("storage_object_id"))
                .to(Alias::new("storage_objects"), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_storage_root_objects_parent")
                .from(
                    Alias::new(ROOT_OBJECTS),
                    Alias::new("parent_storage_object_id"),
                )
                .to(Alias::new("storage_objects"), Alias::new("id")),
        )
        .to_owned()
}

fn root_object_indexes() -> [IndexCreateStatement; 3] {
    [
        Index::create()
            .name("uq_storage_root_objects")
            .table(Alias::new(ROOT_OBJECTS))
            .col(Alias::new("storage_root_id"))
            .col(Alias::new("storage_object_id"))
            .unique()
            .to_owned(),
        Index::create()
            .name("ix_storage_root_objects_parent")
            .table(Alias::new(ROOT_OBJECTS))
            .col(Alias::new("storage_root_id"))
            .col(Alias::new("parent_storage_object_id"))
            .col(Alias::new("presence_state"))
            .to_owned(),
        Index::create()
            .name("ix_storage_root_objects_object")
            .table(Alias::new(ROOT_OBJECTS))
            .col(Alias::new("storage_object_id"))
            .to_owned(),
    ]
}

fn sync_pages_table() -> TableCreateStatement {
    Table::create()
        .table(Alias::new(SYNC_PAGES))
        .col(uuid(Alias::new("id")).primary_key().take())
        .col(uuid(Alias::new("job_id")))
        .col(uuid(Alias::new("storage_root_id")))
        .col(uuid(Alias::new("scope_storage_object_id")))
        .col(string(Alias::new("page_identity")))
        .col(string_len(Alias::new("payload_sha256"), 64))
        .col(big_integer(Alias::new("sync_revision")))
        .col(boolean(Alias::new("scope_completed")))
        .col(timestamp_with_time_zone_null(Alias::new("created_at")))
        .foreign_key(
            ForeignKey::create()
                .name("fk_storage_sync_pages_job")
                .from(Alias::new(SYNC_PAGES), Alias::new("job_id"))
                .to(Alias::new("work_jobs"), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_storage_sync_pages_root")
                .from(Alias::new(SYNC_PAGES), Alias::new("storage_root_id"))
                .to(Alias::new("storage_roots"), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_storage_sync_pages_scope")
                .from(
                    Alias::new(SYNC_PAGES),
                    Alias::new("scope_storage_object_id"),
                )
                .to(Alias::new("storage_objects"), Alias::new("id")),
        )
        .to_owned()
}

fn sync_pages_index() -> IndexCreateStatement {
    Index::create()
        .name("uq_storage_sync_pages_identity")
        .table(Alias::new(SYNC_PAGES))
        .col(Alias::new("job_id"))
        .col(Alias::new("page_identity"))
        .unique()
        .to_owned()
}
