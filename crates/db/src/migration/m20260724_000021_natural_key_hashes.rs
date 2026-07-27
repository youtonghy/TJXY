use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::{
    prelude::{
        Alias, DbErr, DeriveMigrationName, Index, MigrationTrait, Query, SchemaManager, Table,
    },
    schema::string_len_null,
};

use crate::natural_key;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_connection().get_database_backend() == DbBackend::MySql {
            return Ok(());
        }
        add_identity_key(manager, "storage_objects").await?;
        add_identity_key(manager, "import_staging_items").await?;
        add_identity_key(manager, "legacy_item_mappings").await?;
        backfill_storage_object_keys(manager).await?;
        backfill_import_staging_keys(manager).await?;
        backfill_legacy_mapping_keys(manager).await?;
        create_unique_index(
            manager,
            "storage_objects",
            "uq_storage_objects_identity_key",
            &["storage_account_id", "identity_key"],
        )
        .await?;
        create_unique_index(
            manager,
            "import_staging_items",
            "uq_import_staging_item_identity_key",
            &["import_job_id", "identity_key"],
        )
        .await?;
        create_unique_index(
            manager,
            "legacy_item_mappings",
            "uq_legacy_item_mapping_identity_key",
            &["identity_key"],
        )
        .await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // The replacement keys are required by every current write path. Downgrades retain them.
        Ok(())
    }
}

async fn add_identity_key(manager: &SchemaManager<'_>, table: &str) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(Alias::new(table))
                .add_column(string_len_null(Alias::new("identity_key"), 64))
                .to_owned(),
        )
        .await
}

async fn create_unique_index(
    manager: &SchemaManager<'_>,
    table: &str,
    name: &str,
    columns: &[&str],
) -> Result<(), DbErr> {
    let mut index = Index::create();
    index.name(name).table(Alias::new(table)).unique();
    for column in columns {
        index.col(Alias::new(*column));
    }
    manager.create_index(index.clone()).await
}

async fn backfill_storage_object_keys(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let backend = connection.get_database_backend();
    let select = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("provider_drive_id"),
            Alias::new("provider_object_id"),
        ])
        .from(Alias::new("storage_objects"))
        .to_owned();
    let rows = connection.query_all(backend.build(&select)).await?;
    for row in rows {
        let id: uuid::Uuid = row.try_get("", "id")?;
        let drive: String = row.try_get("", "provider_drive_id")?;
        let object: String = row.try_get("", "provider_object_id")?;
        let update = Query::update()
            .table(Alias::new("storage_objects"))
            .value(
                Alias::new("identity_key"),
                natural_key::hash(&[&drive, &object]),
            )
            .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        connection.execute(backend.build(&update)).await?;
    }
    Ok(())
}

async fn backfill_import_staging_keys(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    backfill_two_part_key(
        manager,
        "import_staging_items",
        "entity_kind",
        "legacy_item_id",
    )
    .await
}

async fn backfill_legacy_mapping_keys(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    backfill_two_part_key(
        manager,
        "legacy_item_mappings",
        "source_instance_id",
        "legacy_item_id",
    )
    .await
}

async fn backfill_two_part_key(
    manager: &SchemaManager<'_>,
    table: &str,
    first_column: &str,
    second_column: &str,
) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let backend = connection.get_database_backend();
    let select = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new(first_column),
            Alias::new(second_column),
        ])
        .from(Alias::new(table))
        .to_owned();
    let rows = connection.query_all(backend.build(&select)).await?;
    for row in rows {
        let id: uuid::Uuid = row.try_get("", "id")?;
        let first: String = row.try_get("", first_column)?;
        let second: String = row.try_get("", second_column)?;
        let update = Query::update()
            .table(Alias::new(table))
            .value(
                Alias::new("identity_key"),
                natural_key::hash(&[&first, &second]),
            )
            .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        connection.execute(backend.build(&update)).await?;
    }
    Ok(())
}
