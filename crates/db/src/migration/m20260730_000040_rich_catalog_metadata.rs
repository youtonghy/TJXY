use sea_orm_migration::{
    prelude::*,
    schema::{
        big_integer_null, double_null, integer, integer_null, json, string, string_len,
        string_len_null, string_null, text_null, timestamp_with_time_zone,
        timestamp_with_time_zone_null, uuid, uuid_null,
    },
};

const NEW_TABLES: &[&str] = &[
    "metadata_snapshots",
    "person_assets",
    "person_provider_ids",
    "item_languages",
    "languages",
    "item_countries",
    "countries",
];

const CATALOG_COLUMNS: &[&str] = &[
    "tagline",
    "community_rating",
    "vote_count",
    "runtime_ticks",
    "premiere_date",
    "end_date",
    "release_status",
    "official_rating",
    "original_language",
    "index_number",
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in catalog_columns() {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("catalog_items"))
                        .add_column(column)
                        .to_owned(),
                )
                .await?;
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("item_people"))
                    .add_column(string_len_null(Alias::new("credit_type"), 32))
                    .to_owned(),
            )
            .await?;

        for table in new_tables() {
            manager.create_table(table).await?;
        }

        for index in indexes() {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (table, index) in [
            ("metadata_snapshots", "uq_metadata_snapshots_identity"),
            ("person_provider_ids", "uq_person_provider_ids_person"),
            ("person_provider_ids", "uq_person_provider_ids_identity"),
            ("item_people", "idx_item_people_order"),
            ("catalog_items", "idx_catalog_items_parent_index"),
        ] {
            manager
                .drop_index(
                    Index::drop()
                        .name(index)
                        .table(Alias::new(table))
                        .to_owned(),
                )
                .await?;
        }

        for table in NEW_TABLES {
            manager
                .drop_table(
                    Table::drop()
                        .table(Alias::new(*table))
                        .if_exists()
                        .to_owned(),
                )
                .await?;
        }

        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("item_people"))
                    .drop_column(Alias::new("credit_type"))
                    .to_owned(),
            )
            .await?;
        for column in CATALOG_COLUMNS.iter().rev() {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("catalog_items"))
                        .drop_column(Alias::new(*column))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

fn catalog_columns() -> Vec<ColumnDef> {
    vec![
        text_null(Alias::new("tagline")),
        double_null(Alias::new("community_rating"))
            .check(Expr::col(Alias::new("community_rating")).between(0.0_f64, 10.0_f64))
            .take(),
        big_integer_null(Alias::new("vote_count"))
            .check(Expr::col(Alias::new("vote_count")).gte(0_i64))
            .take(),
        big_integer_null(Alias::new("runtime_ticks"))
            .check(Expr::col(Alias::new("runtime_ticks")).gte(0_i64))
            .take(),
        timestamp_with_time_zone_null(Alias::new("premiere_date")),
        timestamp_with_time_zone_null(Alias::new("end_date")),
        string_len_null(Alias::new("release_status"), 64),
        string_len_null(Alias::new("official_rating"), 32),
        string_len_null(Alias::new("original_language"), 16),
        integer_null(Alias::new("index_number"))
            .check(Expr::col(Alias::new("index_number")).gte(0_i32))
            .take(),
    ]
}

fn new_tables() -> Vec<TableCreateStatement> {
    vec![
        countries(),
        item_countries(),
        languages(),
        item_languages(),
        person_provider_ids(),
        person_assets(),
        metadata_snapshots(),
    ]
}

fn countries() -> TableCreateStatement {
    base("countries")
        .col(string_len(Alias::new("code"), 2).unique_key().take())
        .col(string(Alias::new("name")))
        .to_owned()
}

fn item_countries() -> TableCreateStatement {
    base("item_countries")
        .col(uuid(Alias::new("catalog_item_id")))
        .col(uuid(Alias::new("country_id")))
        .col(integer(Alias::new("sort_order")))
        .index(&mut unique(
            "uq_item_countries",
            &["catalog_item_id", "country_id"],
        ))
        .foreign_key(&mut fk(
            "fk_item_countries_item",
            "item_countries",
            "catalog_item_id",
            "catalog_items",
        ))
        .foreign_key(&mut fk(
            "fk_item_countries_country",
            "item_countries",
            "country_id",
            "countries",
        ))
        .to_owned()
}

fn languages() -> TableCreateStatement {
    base("languages")
        .col(string_len(Alias::new("code"), 16).unique_key().take())
        .col(string(Alias::new("name")))
        .to_owned()
}

fn item_languages() -> TableCreateStatement {
    base("item_languages")
        .col(uuid(Alias::new("catalog_item_id")))
        .col(uuid(Alias::new("language_id")))
        .col(integer(Alias::new("sort_order")))
        .index(&mut unique(
            "uq_item_languages",
            &["catalog_item_id", "language_id"],
        ))
        .foreign_key(&mut fk(
            "fk_item_languages_item",
            "item_languages",
            "catalog_item_id",
            "catalog_items",
        ))
        .foreign_key(&mut fk(
            "fk_item_languages_language",
            "item_languages",
            "language_id",
            "languages",
        ))
        .to_owned()
}

fn person_provider_ids() -> TableCreateStatement {
    base("person_provider_ids")
        .col(uuid(Alias::new("person_id")))
        .col(string_len(Alias::new("provider"), 64))
        .col(string_len(Alias::new("provider_person_id"), 128))
        .foreign_key(&mut fk(
            "fk_person_provider_ids_person",
            "person_provider_ids",
            "person_id",
            "people",
        ))
        .to_owned()
}

fn person_assets() -> TableCreateStatement {
    base("person_assets")
        .col(uuid(Alias::new("person_id")))
        .col(uuid(Alias::new("asset_blob_id")))
        .col(string(Alias::new("image_type")))
        .col(integer(Alias::new("priority")))
        .col(string(Alias::new("source_provider")))
        .col(string_null(Alias::new("source_reference")))
        .index(&mut unique(
            "uq_person_asset_role",
            &["person_id", "image_type", "priority"],
        ))
        .foreign_key(&mut fk(
            "fk_person_assets_person",
            "person_assets",
            "person_id",
            "people",
        ))
        .foreign_key(&mut fk(
            "fk_person_assets_blob",
            "person_assets",
            "asset_blob_id",
            "asset_blobs",
        ))
        .to_owned()
}

fn metadata_snapshots() -> TableCreateStatement {
    base("metadata_snapshots")
        .col(uuid_null(Alias::new("catalog_item_id")))
        .col(uuid_null(Alias::new("person_id")))
        .col(string_len(Alias::new("provider"), 64))
        .col(string_len(Alias::new("entity_kind"), 32))
        .col(string_len(Alias::new("provider_entity_id"), 128))
        .col(string_len(Alias::new("language"), 32))
        .col(timestamp_with_time_zone(Alias::new("fetched_at")))
        .col(json(Alias::new("payload")))
        .foreign_key(&mut fk(
            "fk_metadata_snapshots_item",
            "metadata_snapshots",
            "catalog_item_id",
            "catalog_items",
        ))
        .foreign_key(&mut fk(
            "fk_metadata_snapshots_person",
            "metadata_snapshots",
            "person_id",
            "people",
        ))
        .to_owned()
}

fn indexes() -> Vec<IndexCreateStatement> {
    vec![
        Index::create()
            .name("idx_catalog_items_parent_index")
            .table(Alias::new("catalog_items"))
            .col(Alias::new("parent_id"))
            .col(Alias::new("index_number"))
            .col(Alias::new("sort_key"))
            .col(Alias::new("id"))
            .to_owned(),
        Index::create()
            .name("idx_item_people_order")
            .table(Alias::new("item_people"))
            .col(Alias::new("catalog_item_id"))
            .col(Alias::new("sort_order"))
            .col(Alias::new("id"))
            .to_owned(),
        unique(
            "uq_person_provider_ids_identity",
            &["provider", "provider_person_id"],
        )
        .table(Alias::new("person_provider_ids"))
        .to_owned(),
        unique("uq_person_provider_ids_person", &["person_id", "provider"])
            .table(Alias::new("person_provider_ids"))
            .to_owned(),
        unique(
            "uq_metadata_snapshots_identity",
            &["provider", "entity_kind", "provider_entity_id", "language"],
        )
        .table(Alias::new("metadata_snapshots"))
        .to_owned(),
    ]
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
    index.to_owned()
}
