use sea_orm::{ConnectionTrait, DbErr, QueryResult};
use sea_orm_migration::{
    prelude::{
        Alias, ColumnDef, DeriveMigrationName, Index, MigrationTrait, Query, SchemaManager, Table,
    },
    schema::{blob, boolean, json_null, string, string_null},
};
use tjxy_common::SortKey;
use uuid::Uuid;

const ADDED_COLUMNS: &[(&str, &[&str])] = &[
    ("libraries", &["collection_type", "sort_key", "is_enabled"]),
    ("catalog_items", &["sort_key"]),
    (
        "auth_sessions",
        &[
            "playable_media_types",
            "supported_commands",
            "supports_media_control",
            "supports_persistent_identifier",
            "device_profile",
            "app_store_url",
            "icon_url",
        ],
    ),
];

const INDEXES: &[&str] = &[
    "idx_libraries_browse",
    "idx_catalog_items_parent_browse",
    "idx_catalog_items_type_browse",
    "idx_library_catalog_items_reverse",
    "idx_media_sources_item_probe",
    "idx_media_locations_source_availability",
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in library_columns() {
            add_column(manager, "libraries", column).await?;
        }
        add_column(
            manager,
            "catalog_items",
            blob(Alias::new("sort_key"))
                .default(Vec::<u8>::new())
                .take(),
        )
        .await?;
        for column in session_columns() {
            add_column(manager, "auth_sessions", column).await?;
        }
        backfill_sort_keys(manager).await?;
        for index in indexes() {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for name in INDEXES.iter().rev() {
            manager
                .drop_index(Index::drop().name(*name).to_owned())
                .await?;
        }
        for (table, columns) in ADDED_COLUMNS.iter().rev() {
            for column in columns.iter().rev() {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Alias::new(*table))
                            .drop_column(Alias::new(*column))
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

fn library_columns() -> Vec<ColumnDef> {
    vec![
        string(Alias::new("collection_type"))
            .default("unknown")
            .take(),
        blob(Alias::new("sort_key"))
            .default(Vec::<u8>::new())
            .take(),
        boolean(Alias::new("is_enabled")).default(true).take(),
    ]
}

fn session_columns() -> Vec<ColumnDef> {
    vec![
        json_null(Alias::new("playable_media_types")),
        json_null(Alias::new("supported_commands")),
        boolean(Alias::new("supports_media_control"))
            .default(false)
            .take(),
        boolean(Alias::new("supports_persistent_identifier"))
            .default(false)
            .take(),
        json_null(Alias::new("device_profile")),
        string_null(Alias::new("app_store_url")),
        string_null(Alias::new("icon_url")),
    ]
}

async fn add_column(
    manager: &SchemaManager<'_>,
    table: &str,
    column: ColumnDef,
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(Alias::new(table))
                .add_column(column)
                .to_owned(),
        )
        .await
}

async fn backfill_sort_keys(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    backfill_table(manager, "libraries", "name").await?;
    backfill_table(manager, "catalog_items", "sort_name").await
}

async fn backfill_table(
    manager: &SchemaManager<'_>,
    table: &str,
    source_column: &str,
) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let backend = connection.get_database_backend();
    let select = Query::select()
        .columns([Alias::new("id"), Alias::new(source_column)])
        .from(Alias::new(table))
        .to_owned();
    for row in connection.query_all(backend.build(&select)).await? {
        let (id, value) = row_identity(&row, source_column)?;
        let update = Query::update()
            .table(Alias::new(table))
            .value(
                Alias::new("sort_key"),
                SortKey::from_text(&value).into_bytes(),
            )
            .and_where(sea_orm_migration::prelude::Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        connection.execute(backend.build(&update)).await?;
    }
    Ok(())
}

fn row_identity(row: &QueryResult, source_column: &str) -> Result<(Uuid, String), DbErr> {
    Ok((row.try_get("", "id")?, row.try_get("", source_column)?))
}

fn indexes() -> Vec<sea_orm_migration::prelude::IndexCreateStatement> {
    vec![
        index(
            "idx_libraries_browse",
            "libraries",
            &["is_enabled", "sort_key", "id"],
        ),
        index(
            "idx_catalog_items_parent_browse",
            "catalog_items",
            &["parent_id", "is_present", "sort_key", "id"],
        ),
        index(
            "idx_catalog_items_type_browse",
            "catalog_items",
            &["is_present", "item_type", "sort_key", "id"],
        ),
        index(
            "idx_library_catalog_items_reverse",
            "library_catalog_items",
            &["catalog_item_id", "library_id"],
        ),
        index(
            "idx_media_sources_item_probe",
            "media_sources",
            &["catalog_item_id", "probe_state", "id"],
        ),
        index(
            "idx_media_locations_source_availability",
            "media_locations",
            &["media_source_id", "availability_state", "priority", "id"],
        ),
    ]
}

fn index(
    name: &str,
    table: &str,
    columns: &[&str],
) -> sea_orm_migration::prelude::IndexCreateStatement {
    let mut statement = Index::create();
    statement.name(name).table(Alias::new(table));
    for column in columns {
        statement.col(Alias::new(*column));
    }
    statement.to_owned()
}
