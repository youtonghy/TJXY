use sea_orm_migration::{
    prelude::{
        Alias, ColumnDef, ConditionalStatement, DbErr, DeriveMigrationName, Expr, ForeignKey,
        ForeignKeyCreateStatement, Index, IndexCreateStatement, MigrationTrait, SchemaManager,
        Table, TableAlterStatement, TableCreateStatement,
    },
    schema::{
        blob, double, integer, json, string, string_len, string_null,
        timestamp_with_time_zone_null, uuid,
    },
};

const NEW_TABLES: &[&str] = &[
    "provider_ids",
    "identity_matches",
    "metadata_provenance",
    "people",
    "item_people",
    "genres",
    "item_genres",
    "studios",
    "item_studios",
    "storage_credentials",
];

const ADDED_COLUMNS: &[(&str, &[&str])] = &[
    ("libraries", &["created_at", "updated_at"]),
    ("user_catalog_state", &["updated_at"]),
    ("user_data", &["last_played_at", "updated_at"]),
    ("storage_accounts", &["last_authenticated_at"]),
    (
        "storage_objects",
        &["mime_type", "etag", "remote_modified_at", "last_listed_at"],
    ),
    (
        "storage_sync_cursors",
        &["last_success_at", "last_full_sync_at"],
    ),
    ("storage_change_outbox", &["created_at", "processed_at"]),
    ("work_jobs", &["created_at", "started_at", "completed_at"]),
    ("asset_blobs", &["created_at"]),
    ("media_source_aliases", &["created_at"]),
    ("import_jobs", &["created_at", "updated_at"]),
    ("import_staging_items", &["created_at", "updated_at"]),
    ("import_conflicts", &["created_at", "updated_at"]),
    ("import_errors", &["created_at"]),
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in new_tables() {
            manager.create_table(table).await?;
        }
        for alteration in added_columns() {
            manager.alter_table(alteration).await?;
        }
        manager
            .create_index(
                Index::create()
                    .name("uq_work_jobs_active")
                    .table(Alias::new("work_jobs"))
                    .col(Alias::new("scope_id"))
                    .col(Alias::new("task_kind"))
                    .col(Alias::new("expected_revision"))
                    .unique()
                    .and_where(Expr::col(Alias::new("state")).is_in(["Pending", "Running"]))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("uq_work_jobs_active")
                    .table(Alias::new("work_jobs"))
                    .to_owned(),
            )
            .await?;
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
        for table in NEW_TABLES.iter().rev() {
            manager
                .drop_table(
                    Table::drop()
                        .table(Alias::new(*table))
                        .if_exists()
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

fn id() -> ColumnDef {
    uuid(Alias::new("id")).primary_key().take()
}

fn base(table: &str) -> TableCreateStatement {
    Table::create()
        .table(Alias::new(table))
        .if_not_exists()
        .col(id())
        .to_owned()
}

fn fk(
    name: &str,
    from_table: &str,
    from_column: &str,
    to_table: &str,
) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(Alias::new(from_table), Alias::new(from_column))
        .to(Alias::new(to_table), Alias::new("id"))
        .to_owned()
}

fn unique(name: &str, columns: &[&str]) -> IndexCreateStatement {
    let mut index = Index::create();
    index.name(name).unique();
    for column in columns {
        index.col(Alias::new(*column));
    }
    index.clone()
}

fn new_tables() -> Vec<TableCreateStatement> {
    vec![
        provider_ids(),
        identity_matches(),
        metadata_provenance(),
        people(),
        item_people(),
        genres(),
        item_genres(),
        studios(),
        item_studios(),
        storage_credentials(),
    ]
}

fn provider_ids() -> TableCreateStatement {
    base("provider_ids")
        .col(uuid(Alias::new("catalog_item_id")))
        .col(string(Alias::new("provider")))
        .col(string(Alias::new("provider_item_id")))
        .index(&mut unique(
            "uq_provider_ids_identity",
            &["provider", "provider_item_id"],
        ))
        .foreign_key(&mut fk(
            "fk_provider_ids_item",
            "provider_ids",
            "catalog_item_id",
            "catalog_items",
        ))
        .to_owned()
}

fn identity_matches() -> TableCreateStatement {
    base("identity_matches")
        .col(uuid(Alias::new("storage_object_id")))
        .col(uuid(Alias::new("candidate_catalog_item_id")))
        .col(double(Alias::new("confidence")))
        .col(string(Alias::new("state")))
        .col(json(Alias::new("evidence")))
        .index(&mut unique(
            "uq_identity_match_candidate",
            &["storage_object_id", "candidate_catalog_item_id"],
        ))
        .foreign_key(&mut fk(
            "fk_identity_matches_object",
            "identity_matches",
            "storage_object_id",
            "storage_objects",
        ))
        .foreign_key(&mut fk(
            "fk_identity_matches_item",
            "identity_matches",
            "candidate_catalog_item_id",
            "catalog_items",
        ))
        .to_owned()
}

fn metadata_provenance() -> TableCreateStatement {
    base("metadata_provenance")
        .col(uuid(Alias::new("catalog_item_id")))
        .col(string(Alias::new("field_name")))
        .col(string(Alias::new("source_provider")))
        .col(string_null(Alias::new("source_reference")))
        .col(string_len(Alias::new("value_hash"), 64))
        .index(&mut unique(
            "uq_metadata_provenance_field",
            &["catalog_item_id", "field_name", "source_provider"],
        ))
        .foreign_key(&mut fk(
            "fk_metadata_provenance_item",
            "metadata_provenance",
            "catalog_item_id",
            "catalog_items",
        ))
        .to_owned()
}

fn people() -> TableCreateStatement {
    base("people")
        .col(string(Alias::new("name")))
        .col(string(Alias::new("sort_name")))
        .to_owned()
}

fn item_people() -> TableCreateStatement {
    base("item_people")
        .col(uuid(Alias::new("catalog_item_id")))
        .col(uuid(Alias::new("person_id")))
        .col(string(Alias::new("role")))
        .col(integer(Alias::new("sort_order")))
        .index(&mut unique(
            "uq_item_people_role",
            &["catalog_item_id", "person_id", "role"],
        ))
        .foreign_key(&mut fk(
            "fk_item_people_item",
            "item_people",
            "catalog_item_id",
            "catalog_items",
        ))
        .foreign_key(&mut fk(
            "fk_item_people_person",
            "item_people",
            "person_id",
            "people",
        ))
        .to_owned()
}

fn genres() -> TableCreateStatement {
    base("genres")
        .col(string(Alias::new("name")).unique_key().take())
        .to_owned()
}

fn item_genres() -> TableCreateStatement {
    base("item_genres")
        .col(uuid(Alias::new("catalog_item_id")))
        .col(uuid(Alias::new("genre_id")))
        .index(&mut unique(
            "uq_item_genres",
            &["catalog_item_id", "genre_id"],
        ))
        .foreign_key(&mut fk(
            "fk_item_genres_item",
            "item_genres",
            "catalog_item_id",
            "catalog_items",
        ))
        .foreign_key(&mut fk(
            "fk_item_genres_genre",
            "item_genres",
            "genre_id",
            "genres",
        ))
        .to_owned()
}

fn studios() -> TableCreateStatement {
    base("studios")
        .col(string(Alias::new("name")).unique_key().take())
        .to_owned()
}

fn item_studios() -> TableCreateStatement {
    base("item_studios")
        .col(uuid(Alias::new("catalog_item_id")))
        .col(uuid(Alias::new("studio_id")))
        .index(&mut unique(
            "uq_item_studios",
            &["catalog_item_id", "studio_id"],
        ))
        .foreign_key(&mut fk(
            "fk_item_studios_item",
            "item_studios",
            "catalog_item_id",
            "catalog_items",
        ))
        .foreign_key(&mut fk(
            "fk_item_studios_studio",
            "item_studios",
            "studio_id",
            "studios",
        ))
        .to_owned()
}

fn storage_credentials() -> TableCreateStatement {
    base("storage_credentials")
        .col(blob(Alias::new("encrypted_payload")))
        .col(integer(Alias::new("key_version")))
        .col(string(Alias::new("refresh_state")))
        .col(timestamp_with_time_zone_null(Alias::new("created_at")))
        .col(timestamp_with_time_zone_null(Alias::new("updated_at")))
        .to_owned()
}

fn added_columns() -> Vec<TableAlterStatement> {
    let mut statements = Vec::new();
    for (table, columns) in ADDED_COLUMNS {
        for column in *columns {
            let definition = match (*table, *column) {
                ("storage_objects", "mime_type" | "etag") => string_null(Alias::new(*column)),
                _ => timestamp_with_time_zone_null(Alias::new(*column)),
            };
            statements.push(add_column(table, definition));
        }
    }
    statements
}

fn add_column(table: &str, column: ColumnDef) -> TableAlterStatement {
    Table::alter()
        .table(Alias::new(table))
        .add_column(column)
        .to_owned()
}
