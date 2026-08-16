use sea_orm::{ConnectionTrait, DbErr};
use sea_orm_migration::prelude::{
    Alias, ColumnDef, DeriveMigrationName, ForeignKey, MigrationTrait, SchemaManager, Table,
};
use sea_orm_migration::schema::{
    blob, string_len, timestamp_with_time_zone, timestamp_with_time_zone_null, uuid,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mysql = manager.get_connection().get_database_backend() == sea_orm::DbBackend::MySql;
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("qr_login_challenges"))
                    .if_not_exists()
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(if mysql {
                        ColumnDef::new(Alias::new("poll_digest"))
                            .var_binary(32)
                            .not_null()
                            .take()
                    } else {
                        blob(Alias::new("poll_digest"))
                    })
                    .col(if mysql {
                        ColumnDef::new(Alias::new("approval_digest"))
                            .var_binary(32)
                            .not_null()
                            .take()
                    } else {
                        blob(Alias::new("approval_digest"))
                    })
                    .col(string_len(Alias::new("state"), 16))
                    .col(string_len(Alias::new("device_id"), 512))
                    .col(string_len(Alias::new("device_name"), 256))
                    .col(string_len(Alias::new("client_name"), 256))
                    .col(string_len(Alias::new("client_version"), 128))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("expires_at")))
                    .col(timestamp_with_time_zone_null(Alias::new("approved_at")))
                    .col(ColumnDef::new(Alias::new("approved_user_id")).uuid().null())
                    .col(
                        ColumnDef::new(Alias::new("approved_session_id"))
                            .uuid()
                            .null(),
                    )
                    .col(timestamp_with_time_zone_null(Alias::new("consumed_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                Alias::new("qr_login_challenges"),
                                Alias::new("approved_user_id"),
                            )
                            .to(Alias::new("users"), Alias::new("id")),
                    )
                    .to_owned(),
            )
            .await?;
        let backend = manager.get_connection().get_database_backend();
        manager
            .get_connection()
            .execute(
                backend.build(
                    &sea_orm::sea_query::Index::create()
                        .name("idx_qr_login_challenges_state_expiry")
                        .table(Alias::new("qr_login_challenges"))
                        .col(Alias::new("state"))
                        .col(Alias::new("expires_at"))
                        .to_owned(),
                ),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("qr_login_challenges"))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
