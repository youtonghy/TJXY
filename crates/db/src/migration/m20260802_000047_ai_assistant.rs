use sea_orm_migration::{
    prelude::*,
    schema::{
        big_integer, blob, boolean, integer, string_len, text, timestamp_with_time_zone, uuid,
    },
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[allow(clippy::too_many_lines)] // One reversible migration owns the four related AI tables.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("ai_provider_settings"))
                    .col(string_len(Alias::new("provider"), 64).primary_key().take())
                    .col(boolean(Alias::new("enabled")))
                    .col(string_len(Alias::new("base_url"), 2_048))
                    .col(text(Alias::new("system_prompt")))
                    .col(uuid(Alias::new("credential_id")))
                    .col(blob(Alias::new("encrypted_payload")))
                    .col(integer(Alias::new("key_version")))
                    .col(big_integer(Alias::new("revision")))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("ai_models"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(string_len(Alias::new("provider"), 64))
                    .col(string_len(Alias::new("upstream_id"), 255))
                    .col(string_len(Alias::new("display_name"), 128))
                    .col(boolean(Alias::new("is_visible")))
                    .col(boolean(Alias::new("is_default")))
                    .col(integer(Alias::new("sort_order")))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_ai_models_provider")
                            .from(Alias::new("ai_models"), Alias::new("provider"))
                            .to(Alias::new("ai_provider_settings"), Alias::new("provider"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_ai_models_provider_upstream")
                    .table(Alias::new("ai_models"))
                    .col(Alias::new("provider"))
                    .col(Alias::new("upstream_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_ai_models_visibility_order")
                    .table(Alias::new("ai_models"))
                    .col(Alias::new("provider"))
                    .col(Alias::new("is_visible"))
                    .col(Alias::new("sort_order"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("ai_conversations"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("user_id")))
                    .col(uuid(Alias::new("model_id")))
                    .col(string_len(Alias::new("title"), 160))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .col(timestamp_with_time_zone(Alias::new("updated_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_ai_conversations_user")
                            .from(Alias::new("ai_conversations"), Alias::new("user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_ai_conversations_user_updated")
                    .table(Alias::new("ai_conversations"))
                    .col(Alias::new("user_id"))
                    .col(Alias::new("updated_at"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("ai_messages"))
                    .col(uuid(Alias::new("id")).primary_key().take())
                    .col(uuid(Alias::new("conversation_id")))
                    .col(string_len(Alias::new("role"), 16))
                    .col(text(Alias::new("content")))
                    .col(text(Alias::new("metadata_json")))
                    .col(timestamp_with_time_zone(Alias::new("created_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_ai_messages_conversation")
                            .from(Alias::new("ai_messages"), Alias::new("conversation_id"))
                            .to(Alias::new("ai_conversations"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_ai_messages_conversation_created")
                    .table(Alias::new("ai_messages"))
                    .col(Alias::new("conversation_id"))
                    .col(Alias::new("created_at"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            "ai_messages",
            "ai_conversations",
            "ai_models",
            "ai_provider_settings",
        ] {
            manager
                .drop_table(
                    Table::drop()
                        .table(Alias::new(table))
                        .if_exists()
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
