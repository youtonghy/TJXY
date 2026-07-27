use sea_orm::{ConnectionTrait, QueryResult};
use sea_orm_migration::{
    prelude::*,
    schema::{string_len, string_len_null, timestamp_with_time_zone},
};
use uuid::Uuid;

const BACKFILL_BATCH_SIZE: u64 = 500;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_column("auth_sessions", "device_key").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("auth_sessions"))
                        .add_column(string_len_null(Alias::new("device_key"), 64))
                        .to_owned(),
                )
                .await?;
        }
        backfill_device_keys(manager).await?;

        if !manager.has_table("device_options").await? {
            manager
                .create_table(
                    Table::create()
                        .table(Alias::new("device_options"))
                        .col(
                            ColumnDef::new(Alias::new("id"))
                                .big_integer()
                                .not_null()
                                .auto_increment()
                                .primary_key(),
                        )
                        .col(string_len(Alias::new("device_key"), 64))
                        .col(string_len(Alias::new("device_id"), 512))
                        .col(string_len_null(Alias::new("custom_name"), 256))
                        .col(timestamp_with_time_zone(Alias::new("created_at")))
                        .col(timestamp_with_time_zone(Alias::new("updated_at")))
                        .to_owned(),
                )
                .await?;
        }
        create_index_if_missing(
            manager,
            "device_options",
            "uq_device_options_device_key",
            Index::create()
                .name("uq_device_options_device_key")
                .table(Alias::new("device_options"))
                .col(Alias::new("device_key"))
                .unique()
                .to_owned(),
        )
        .await?;
        create_index_if_missing(
            manager,
            "auth_sessions",
            "idx_auth_sessions_device_state",
            Index::create()
                .name("idx_auth_sessions_device_state")
                .table(Alias::new("auth_sessions"))
                .col(Alias::new("device_key"))
                .col(Alias::new("revoked_at"))
                .col(Alias::new("expires_at"))
                .to_owned(),
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_index_if_present(manager, "auth_sessions", "idx_auth_sessions_device_state").await?;
        drop_index_if_present(manager, "device_options", "uq_device_options_device_key").await?;
        if manager.has_table("device_options").await? {
            manager
                .drop_table(Table::drop().table(Alias::new("device_options")).to_owned())
                .await?;
        }
        if manager.has_column("auth_sessions", "device_key").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("auth_sessions"))
                        .drop_column(Alias::new("device_key"))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

async fn backfill_device_keys(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let backend = connection.get_database_backend();
    loop {
        let query = Query::select()
            .columns([Alias::new("id"), Alias::new("device_id")])
            .from(Alias::new("auth_sessions"))
            .and_where(Expr::col(Alias::new("device_key")).is_null())
            .limit(BACKFILL_BATCH_SIZE)
            .to_owned();
        let rows = connection.query_all(backend.build(&query)).await?;
        if rows.is_empty() {
            return Ok(());
        }
        for row in rows {
            let (id, device_id) = session_identity(&row)?;
            let update = Query::update()
                .table(Alias::new("auth_sessions"))
                .value(
                    Alias::new("device_key"),
                    crate::natural_key::hash(&["device", &device_id]),
                )
                .and_where(Expr::col(Alias::new("id")).eq(id))
                .and_where(Expr::col(Alias::new("device_key")).is_null())
                .to_owned();
            connection.execute(backend.build(&update)).await?;
        }
    }
}

fn session_identity(row: &QueryResult) -> Result<(Uuid, String), DbErr> {
    Ok((row.try_get("", "id")?, row.try_get("", "device_id")?))
}

async fn create_index_if_missing(
    manager: &SchemaManager<'_>,
    table: &str,
    name: &str,
    index: IndexCreateStatement,
) -> Result<(), DbErr> {
    if !manager.has_index(table, name).await? {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn drop_index_if_present(
    manager: &SchemaManager<'_>,
    table: &str,
    name: &str,
) -> Result<(), DbErr> {
    if manager.has_table(table).await? && manager.has_index(table, name).await? {
        manager
            .drop_index(Index::drop().name(name).table(Alias::new(table)).to_owned())
            .await?;
    }
    Ok(())
}
