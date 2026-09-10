use sea_orm_migration::prelude::*;

const INDEXES: &[(&str, &str, &[&str])] = &[
    (
        "identity_matches",
        "ix_identity_matches_candidate_scope",
        &["candidate_catalog_item_id", "state", "storage_object_id"],
    ),
    (
        "publication_media_locations",
        "ix_publication_locations_source",
        &["media_source_id", "publication_id", "storage_object_id"],
    ),
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for &(table, name, columns) in INDEXES {
            let mut index = Index::create();
            index.name(name).table(Alias::new(table));
            for &column in columns {
                index.col(Alias::new(column));
            }
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // InnoDB may replace its implicit foreign-key index with the wider
        // lookup index. Restore that support before removing the replacement.
        if manager.get_database_backend() == sea_orm::DbBackend::MySql
            && !manager
                .has_index("identity_matches", "fk_identity_matches_item")
                .await?
        {
            manager
                .create_index(
                    Index::create()
                        .name("fk_identity_matches_item")
                        .table(Alias::new("identity_matches"))
                        .col(Alias::new("candidate_catalog_item_id"))
                        .to_owned(),
                )
                .await?;
        }
        for &(table, name, _) in INDEXES.iter().rev() {
            manager
                .drop_index(Index::drop().name(name).table(Alias::new(table)).to_owned())
                .await?;
        }
        Ok(())
    }
}
