use sea_orm_migration::{
    prelude::*,
    schema::{integer, timestamp_with_time_zone},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("work_maintenance_state"))
                    .col(integer(Alias::new("id")).primary_key())
                    .col(timestamp_with_time_zone(Alias::new("available_at")))
                    .to_owned(),
            )
            .await?;
        let query = Query::insert()
            .into_table(Alias::new("work_maintenance_state"))
            .columns([Alias::new("id"), Alias::new("available_at")])
            .values_panic([
                1_i32.into(),
                // MySQL TIMESTAMP excludes the zero epoch second.
                (chrono::DateTime::<chrono::Utc>::UNIX_EPOCH + chrono::Duration::seconds(1)).into(),
            ])
            .to_owned();
        manager
            .get_connection()
            .execute(manager.get_database_backend().build(&query))
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("work_maintenance_state"))
                    .to_owned(),
            )
            .await
    }
}
