use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::{
    Alias, DbErr, DeriveMigrationName, Index, MigrationTrait, SchemaManager,
};

const INDEXES: [(&str, &str, &str); 5] = [
    ("item_genres", "ix_item_genres_genre_item", "genre_id"),
    ("item_people", "ix_item_people_person_item", "person_id"),
    (
        "item_languages",
        "ix_item_languages_language_item",
        "language_id",
    ),
    ("item_studios", "ix_item_studios_studio_item", "studio_id"),
    (
        "item_countries",
        "ix_item_countries_country_item",
        "country_id",
    ),
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (table, index, feature_column) in INDEXES {
            manager
                .create_index(
                    Index::create()
                        .name(index)
                        .table(Alias::new(table))
                        .col(Alias::new(feature_column))
                        .col(Alias::new("catalog_item_id"))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (table, index, feature_column) in INDEXES.into_iter().rev() {
            // InnoDB may remove its implicit FK index when this covering index
            // is created. Restore that index before removing its replacement.
            if manager.get_connection().get_database_backend() == DbBackend::MySql {
                let foreign_index =
                    format!("fk_{table}_{}", feature_column.trim_end_matches("_id"));
                if !manager.has_index(table, &foreign_index).await? {
                    manager
                        .create_index(
                            Index::create()
                                .name(&foreign_index)
                                .table(Alias::new(table))
                                .col(Alias::new(feature_column))
                                .to_owned(),
                        )
                        .await?;
                }
            }
            manager
                .drop_index(
                    Index::drop()
                        .name(index)
                        .table(Alias::new(table))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
