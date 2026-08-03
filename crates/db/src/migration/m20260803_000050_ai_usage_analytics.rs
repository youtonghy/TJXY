use sea_orm_migration::{
    prelude::*,
    schema::{big_integer, big_integer_null, string_len, timestamp_with_time_zone, uuid},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("ai_execution_records"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("user_id")))
                    .col(uuid(Alias::new("model_id")))
                    .col(string_len(Alias::new("model_display_name"), 128))
                    .col(string_len(Alias::new("upstream_model_id"), 255))
                    .col(string_len(Alias::new("day_key"), 10))
                    .col(timestamp_with_time_zone(Alias::new("started_at")))
                    .col(timestamp_with_time_zone(Alias::new("completed_at")))
                    .col(big_integer(Alias::new("elapsed_ms")))
                    .col(string_len(Alias::new("outcome"), 32))
                    .col(big_integer_null(Alias::new("prompt_tokens")))
                    .col(big_integer_null(Alias::new("completion_tokens")))
                    .col(big_integer_null(Alias::new("total_tokens")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_ai_execution_records_user")
                            .from(Alias::new("ai_execution_records"), Alias::new("user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        for (name, columns) in [
            ("ix_ai_execution_records_day", vec!["day_key", "started_at"]),
            (
                "ix_ai_execution_records_user_day",
                vec!["user_id", "day_key"],
            ),
            (
                "ix_ai_execution_records_model_day",
                vec!["model_id", "day_key"],
            ),
            (
                "ix_ai_execution_records_outcome_started",
                vec!["outcome", "started_at"],
            ),
        ] {
            let mut index = Index::create();
            index.name(name).table(Alias::new("ai_execution_records"));
            for column in columns {
                index.col(Alias::new(column));
            }
            manager.create_index(index.clone()).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("ai_execution_records"))
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
