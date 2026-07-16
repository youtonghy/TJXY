use std::collections::BTreeMap;

use sea_orm::{ConnectionTrait, DbErr, QueryResult};
use sea_orm_migration::{
    prelude::{
        Alias, ColumnDef, DeriveMigrationName, ForeignKey, Index, MigrationTrait, Query,
        SchemaManager, Table,
    },
    schema::{
        big_integer, blob, boolean, integer, string_len, string_null, timestamp_with_time_zone,
        timestamp_with_time_zone_null, uuid,
    },
};
use tjxy_common::Username;
use uuid::Uuid;

const USER_COLUMNS: &[&str] = &[
    "username_key",
    "has_password",
    "auth_revision",
    "disabled_at",
    "created_at",
    "updated_at",
    "last_login_at",
    "last_activity_at",
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(auth_state()).await?;
        seed_auth_state(manager).await?;
        for statement in user_columns() {
            manager.alter_table(statement).await?;
        }
        backfill_username_keys(manager).await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_users_username_key")
                    .table(Alias::new("users"))
                    .col(Alias::new("username_key"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager.create_table(auth_sessions()).await?;
        for index in auth_session_indexes() {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("auth_sessions"))
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("auth_state"))
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("uq_users_username_key")
                    .table(Alias::new("users"))
                    .to_owned(),
            )
            .await?;
        for column in USER_COLUMNS.iter().rev() {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("users"))
                        .drop_column(Alias::new(*column))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

fn auth_state() -> sea_orm_migration::prelude::TableCreateStatement {
    Table::create()
        .table(Alias::new("auth_state"))
        .if_not_exists()
        .col(integer(Alias::new("id")).primary_key().take())
        .col(big_integer(Alias::new("bootstrap_revision")))
        .to_owned()
}

async fn seed_auth_state(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let backend = connection.get_database_backend();
    let insert = Query::insert()
        .into_table(Alias::new("auth_state"))
        .columns([Alias::new("id"), Alias::new("bootstrap_revision")])
        .values_panic([1_i32.into(), 0_i64.into()])
        .to_owned();
    connection.execute(backend.build(&insert)).await?;
    Ok(())
}

fn user_columns() -> Vec<sea_orm_migration::prelude::TableAlterStatement> {
    vec![
        add_user_column(
            blob(Alias::new("username_key"))
                .default(Vec::<u8>::new())
                .take(),
        ),
        add_user_column(boolean(Alias::new("has_password")).default(true).take()),
        add_user_column(
            big_integer(Alias::new("auth_revision"))
                .default(0_i64)
                .take(),
        ),
        add_user_column(timestamp_with_time_zone_null(Alias::new("disabled_at"))),
        add_user_column(timestamp_with_time_zone_null(Alias::new("created_at"))),
        add_user_column(timestamp_with_time_zone_null(Alias::new("updated_at"))),
        add_user_column(timestamp_with_time_zone_null(Alias::new("last_login_at"))),
        add_user_column(timestamp_with_time_zone_null(Alias::new(
            "last_activity_at",
        ))),
    ]
}

fn add_user_column(column: ColumnDef) -> sea_orm_migration::prelude::TableAlterStatement {
    Table::alter()
        .table(Alias::new("users"))
        .add_column(column)
        .to_owned()
}

async fn backfill_username_keys(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let backend = connection.get_database_backend();
    let select = Query::select()
        .columns([Alias::new("id"), Alias::new("username")])
        .from(Alias::new("users"))
        .to_owned();
    let rows = connection.query_all(backend.build(&select)).await?;
    let mut seen = BTreeMap::<Vec<u8>, Uuid>::new();
    for row in rows {
        let (user_id, username) = user_identity(&row)?;
        let username = Username::parse(&username).map_err(|error| {
            DbErr::Migration(format!("cannot normalize existing user {user_id}: {error}"))
        })?;
        if let Some(existing) = seen.insert(username.key().to_vec(), user_id) {
            return Err(DbErr::Migration(format!(
                "users {existing} and {user_id} normalize to the same identity key"
            )));
        }
        let update = Query::update()
            .table(Alias::new("users"))
            .value(Alias::new("username_key"), username.key().to_vec())
            .and_where(sea_orm_migration::prelude::Expr::col(Alias::new("id")).eq(user_id))
            .to_owned();
        connection.execute(backend.build(&update)).await?;
    }
    Ok(())
}

fn user_identity(row: &QueryResult) -> Result<(Uuid, String), DbErr> {
    Ok((row.try_get("", "id")?, row.try_get("", "username")?))
}

fn auth_sessions() -> sea_orm_migration::prelude::TableCreateStatement {
    Table::create()
        .table(Alias::new("auth_sessions"))
        .if_not_exists()
        .col(uuid(Alias::new("id")).primary_key().take())
        .col(uuid(Alias::new("user_id")))
        .col(blob(Alias::new("token_digest")))
        .col(big_integer(Alias::new("auth_revision")))
        .col(string_len(Alias::new("device_id"), 512))
        .col(string_len(Alias::new("device_name"), 256))
        .col(string_len(Alias::new("client_name"), 256))
        .col(string_len(Alias::new("client_version"), 128))
        .col(timestamp_with_time_zone(Alias::new("created_at")))
        .col(timestamp_with_time_zone_null(Alias::new("expires_at")))
        .col(timestamp_with_time_zone_null(Alias::new("last_seen_at")))
        .col(timestamp_with_time_zone_null(Alias::new("revoked_at")))
        .col(string_null(Alias::new("revoke_reason")))
        .foreign_key(&mut auth_session_user_fk())
        .to_owned()
}

fn auth_session_user_fk() -> sea_orm_migration::prelude::ForeignKeyCreateStatement {
    ForeignKey::create()
        .name("fk_auth_sessions_user")
        .from(Alias::new("auth_sessions"), Alias::new("user_id"))
        .to(Alias::new("users"), Alias::new("id"))
        .to_owned()
}

fn auth_session_indexes() -> Vec<sea_orm_migration::prelude::IndexCreateStatement> {
    vec![
        Index::create()
            .name("uq_auth_sessions_token_digest")
            .table(Alias::new("auth_sessions"))
            .col(Alias::new("token_digest"))
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_auth_sessions_user_state")
            .table(Alias::new("auth_sessions"))
            .col(Alias::new("user_id"))
            .col(Alias::new("revoked_at"))
            .col(Alias::new("expires_at"))
            .to_owned(),
        Index::create()
            .name("idx_auth_sessions_expiry")
            .table(Alias::new("auth_sessions"))
            .col(Alias::new("expires_at"))
            .to_owned(),
    ]
}
