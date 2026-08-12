use sea_orm_migration::{prelude::*, schema::string};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        ensure_schema(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.has_column("asset_blobs", "storage_root_id").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("asset_blobs"))
                        .drop_column(Alias::new("storage_root_id"))
                        .to_owned(),
                )
                .await?;
        }
        if manager.has_table("asset_storage_roots").await? {
            manager
                .drop_table(
                    Table::drop()
                        .table(Alias::new("asset_storage_roots"))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

pub(crate) async fn ensure_schema(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if !manager.has_table("asset_storage_roots").await? {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("asset_storage_roots"))
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(string(Alias::new("canonical_path")).not_null().unique_key())
                    .col(string(Alias::new("state")).not_null())
                    .col(
                        ColumnDef::new(Alias::new("revision"))
                            .big_integer()
                            .not_null()
                            .default(1_i64),
                    )
                    .col(
                        ColumnDef::new(Alias::new("created_at"))
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("updated_at"))
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .check(Expr::col(Alias::new("state")).is_in(["Current", "Pending", "History"]))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_asset_storage_roots_state")
                    .table(Alias::new("asset_storage_roots"))
                    .col(Alias::new("state"))
                    .to_owned(),
            )
            .await?;
    }
    if !manager.has_column("asset_blobs", "storage_root_id").await? {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("asset_blobs"))
                    .add_column(ColumnDef::new(Alias::new("storage_root_id")).uuid().null())
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}
