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

const INDEXES: &[(&str, &str)] = &[
    ("idx_libraries_browse", "libraries"),
    ("idx_catalog_items_parent_browse", "catalog_items"),
    ("idx_catalog_items_type_browse", "catalog_items"),
    ("idx_library_catalog_items_reverse", "library_catalog_items"),
    ("idx_media_sources_item_probe", "media_sources"),
    ("idx_media_locations_source_availability", "media_locations"),
];
const MAX_SORT_KEY_BYTES: u32 = 2_000;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mysql = manager.get_connection().get_database_backend() == sea_orm::DbBackend::MySql;
        for column in library_columns(mysql) {
            add_column(manager, "libraries", column).await?;
        }
        add_column(manager, "catalog_items", sort_key_column(mysql)).await?;
        for column in session_columns() {
            add_column(manager, "auth_sessions", column).await?;
        }
        backfill_sort_keys(manager).await?;
        if mysql {
            for index in [
                Index::create()
                    .name("ix_catalog_items_parent")
                    .table(Alias::new("catalog_items"))
                    .col(Alias::new("parent_id"))
                    .to_owned(),
                Index::create()
                    .name("ix_library_catalog_items_catalog_item")
                    .table(Alias::new("library_catalog_items"))
                    .col(Alias::new("catalog_item_id"))
                    .to_owned(),
                Index::create()
                    .name("ix_media_sources_catalog_item")
                    .table(Alias::new("media_sources"))
                    .col(Alias::new("catalog_item_id"))
                    .to_owned(),
                Index::create()
                    .name("ix_media_locations_media_source")
                    .table(Alias::new("media_locations"))
                    .col(Alias::new("media_source_id"))
                    .to_owned(),
            ] {
                manager.create_index(index).await?;
            }
        }
        for index in indexes() {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (name, table) in INDEXES.iter().rev() {
            manager
                .drop_index(
                    Index::drop()
                        .name(*name)
                        .table(Alias::new(*table))
                        .to_owned(),
                )
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

fn library_columns(mysql: bool) -> Vec<ColumnDef> {
    vec![
        string(Alias::new("collection_type"))
            .default("unknown")
            .take(),
        sort_key_column(mysql),
        boolean(Alias::new("is_enabled")).default(true).take(),
    ]
}

fn sort_key_column(mysql: bool) -> ColumnDef {
    if mysql {
        ColumnDef::new(Alias::new("sort_key"))
            .var_binary(MAX_SORT_KEY_BYTES)
            .not_null()
            .default(Vec::<u8>::new())
            .take()
    } else {
        blob(Alias::new("sort_key"))
            .default(Vec::<u8>::new())
            .take()
    }
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
    const BATCH_SIZE: u64 = 500;
    let connection = manager.get_connection();
    let backend = connection.get_database_backend();
    loop {
        let select = Query::select()
            .columns([Alias::new("id"), Alias::new(source_column)])
            .from(Alias::new(table))
            .and_where(
                sea_orm_migration::prelude::Expr::col(Alias::new("sort_key")).eq(Vec::<u8>::new()),
            )
            .limit(BATCH_SIZE)
            .to_owned();
        let rows = connection.query_all(backend.build(&select)).await?;
        if rows.is_empty() {
            return Ok(());
        }
        for row in rows {
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
    }
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
    statement.clone()
}
