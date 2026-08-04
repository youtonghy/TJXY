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
        for (table, index, _) in INDEXES.into_iter().rev() {
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
