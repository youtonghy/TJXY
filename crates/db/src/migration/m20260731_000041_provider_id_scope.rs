use sea_orm::ConnectionTrait;
use sea_orm_migration::{
    prelude::*,
    schema::{string, uuid},
};

const TABLE: &str = "provider_ids";
const TEMP_TABLE: &str = "provider_ids_scoped";
const LEGACY_TABLE: &str = "provider_ids_legacy";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(provider_ids_table(TEMP_TABLE)).await?;
        copy_rows(manager, TABLE, TEMP_TABLE).await?;
        manager
            .drop_table(Table::drop().table(Alias::new(TABLE)).to_owned())
            .await?;
        manager
            .rename_table(
                Table::rename()
                    .table(Alias::new(TEMP_TABLE), Alias::new(TABLE))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_provider_ids_item_provider")
                    .table(Alias::new(TABLE))
                    .col(Alias::new("catalog_item_id"))
                    .col(Alias::new("provider"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_provider_ids_identity")
                    .table(Alias::new(TABLE))
                    .col(Alias::new("provider"))
                    .col(Alias::new("provider_item_id"))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        reject_duplicate_identities(manager).await?;
        manager
            .create_table(provider_ids_table(LEGACY_TABLE))
            .await?;
        copy_rows(manager, TABLE, LEGACY_TABLE).await?;
        manager
            .drop_table(Table::drop().table(Alias::new(TABLE)).to_owned())
            .await?;
        manager
            .rename_table(
                Table::rename()
                    .table(Alias::new(LEGACY_TABLE), Alias::new(TABLE))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_provider_ids_identity")
                    .table(Alias::new(TABLE))
                    .col(Alias::new("provider"))
                    .col(Alias::new("provider_item_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

fn provider_ids_table(table: &str) -> TableCreateStatement {
    let mut foreign_key = ForeignKey::create();
    foreign_key
        .name(match table {
            TEMP_TABLE => "fk_provider_ids_scoped_item",
            LEGACY_TABLE => "fk_provider_ids_legacy_item",
            _ => "fk_provider_ids_item",
        })
        .from(Alias::new(table), Alias::new("catalog_item_id"))
        .to(Alias::new("catalog_items"), Alias::new("id"));
    Table::create()
        .table(Alias::new(table))
        .col(uuid(Alias::new("id")).primary_key().take())
        .col(uuid(Alias::new("catalog_item_id")))
        .col(string(Alias::new("provider")))
        .col(string(Alias::new("provider_item_id")))
        .foreign_key(&mut foreign_key)
        .to_owned()
}

async fn reject_duplicate_identities(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let duplicate = Query::select()
        .column(Alias::new("provider"))
        .from(Alias::new(TABLE))
        .group_by_columns([Alias::new("provider"), Alias::new("provider_item_id")])
        .and_having(Expr::col(Alias::new("id")).count().gt(1))
        .limit(1)
        .to_owned();
    if manager
        .get_connection()
        .query_one(manager.get_database_backend().build(&duplicate))
        .await?
        .is_some()
    {
        return Err(DbErr::Custom(
            "cannot roll back provider identity scope while duplicate provider identities exist"
                .to_owned(),
        ));
    }
    Ok(())
}

async fn copy_rows(manager: &SchemaManager<'_>, from: &str, to: &str) -> Result<(), DbErr> {
    let columns = ["id", "catalog_item_id", "provider", "provider_item_id"];
    let select = Query::select()
        .columns(columns.map(Alias::new))
        .from(Alias::new(from))
        .to_owned();
    let mut insert = Query::insert();
    insert
        .into_table(Alias::new(to))
        .columns(columns.map(Alias::new))
        .select_from(select)
        .map_err(|error| DbErr::Custom(error.to_string()))?;
    manager
        .get_connection()
        .execute(manager.get_database_backend().build(&insert))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::sea_orm::sea_query::MysqlQueryBuilder;

    #[test]
    fn mysql_temporary_tables_use_distinct_foreign_key_names() {
        let scoped = provider_ids_table(TEMP_TABLE).to_string(MysqlQueryBuilder);
        let legacy = provider_ids_table(LEGACY_TABLE).to_string(MysqlQueryBuilder);

        assert!(scoped.contains("fk_provider_ids_scoped_item"), "{scoped}");
        assert!(legacy.contains("fk_provider_ids_legacy_item"), "{legacy}");
        assert!(
            !scoped.contains("CONSTRAINT `fk_provider_ids_item`"),
            "{scoped}"
        );
        assert!(
            !legacy.contains("CONSTRAINT `fk_provider_ids_item`"),
            "{legacy}"
        );
    }
}
