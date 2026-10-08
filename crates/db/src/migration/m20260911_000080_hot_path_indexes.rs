use sea_orm_migration::prelude::*;

/// Indexes for lookups that previously fell back to sequential scans on busy tables:
/// credited-person resolution during every metadata write, the publication reverse lookup used
/// by catalog visibility, the owner pointer join, and the queue retention scans.
const INDEXES: &[(&str, &str, &[&str])] = &[
    ("people", "ix_people_name", &["name"]),
    (
        "publication_catalog_items",
        "ix_publication_catalog_items_item",
        &["catalog_item_id", "publication_id"],
    ),
    (
        "catalog_items",
        "ix_catalog_items_active_structure_publication",
        &["active_structure_publication_id"],
    ),
    (
        "work_jobs",
        "ix_work_jobs_required_sync_job",
        &["required_sync_job_id"],
    ),
    (
        "work_job_retention_queue",
        "ix_work_job_retention_terminal",
        &["terminal_at", "job_id"],
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
        for &(table, name, _) in INDEXES.iter().rev() {
            manager
                .drop_index(Index::drop().name(name).table(Alias::new(table)).to_owned())
                .await?;
        }
        Ok(())
    }
}
