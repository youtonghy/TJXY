use sea_orm_migration::{
    prelude::*,
    schema::{big_integer, integer, string_len, string_len_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    #[allow(clippy::too_many_lines)] // One migration atomically introduces the direct metadata policy and reference schema.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("libraries"))
                    .add_column(
                        string_len(Alias::new("local_metadata_access_mode"), 16).default("import"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("work_jobs"))
                    .add_column(string_len_null(
                        Alias::new("local_metadata_access_mode"),
                        16,
                    ))
                    .to_owned(),
            )
            .await?;
        let connection = manager.get_connection();
        connection
            .execute(
                connection.get_database_backend().build(
                    Query::update()
                        .table(Alias::new("work_jobs"))
                        .value(Alias::new("local_metadata_access_mode"), "import")
                        .and_where(Expr::col(Alias::new("task_kind")).eq("ResolveMetadata")),
                ),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("direct_metadata_refs"))
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Alias::new("library_id")).uuid().not_null())
                    .col(
                        ColumnDef::new(Alias::new("catalog_item_id"))
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("storage_root_id"))
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("storage_object_id"))
                            .uuid()
                            .not_null(),
                    )
                    .col(string_len(Alias::new("resource_kind"), 16))
                    .col(integer(Alias::new("priority")))
                    .col(big_integer(Alias::new("input_revision")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_direct_metadata_refs_library")
                            .from(Alias::new("direct_metadata_refs"), Alias::new("library_id"))
                            .to(Alias::new("libraries"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_direct_metadata_refs_item")
                            .from(
                                Alias::new("direct_metadata_refs"),
                                Alias::new("catalog_item_id"),
                            )
                            .to(Alias::new("catalog_items"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_direct_metadata_refs_root")
                            .from(
                                Alias::new("direct_metadata_refs"),
                                Alias::new("storage_root_id"),
                            )
                            .to(Alias::new("storage_roots"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_direct_metadata_refs_object")
                            .from(
                                Alias::new("direct_metadata_refs"),
                                Alias::new("storage_object_id"),
                            )
                            .to(Alias::new("storage_objects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .check(
                        Expr::col(Alias::new("resource_kind"))
                            .is_in(["Nfo", "Primary", "Backdrop"]),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_direct_metadata_refs_item_kind_priority")
                    .table(Alias::new("direct_metadata_refs"))
                    .col(Alias::new("library_id"))
                    .col(Alias::new("catalog_item_id"))
                    .col(Alias::new("resource_kind"))
                    .col(Alias::new("priority"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_direct_metadata_refs_storage_object")
                    .table(Alias::new("direct_metadata_refs"))
                    .col(Alias::new("storage_object_id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("direct_metadata_refs"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("work_jobs"))
                    .drop_column(Alias::new("local_metadata_access_mode"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("libraries"))
                    .drop_column(Alias::new("local_metadata_access_mode"))
                    .to_owned(),
            )
            .await
    }
}
