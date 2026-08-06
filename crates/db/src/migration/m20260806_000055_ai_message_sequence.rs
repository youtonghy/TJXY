use sea_orm::{ConnectionTrait, TransactionTrait};
use sea_orm_migration::{
    prelude::{
        Alias, DbErr, DeriveMigrationName, Expr, Index, MigrationTrait, Query, SchemaManager, Table,
    },
    schema::big_integer,
};
use uuid::Uuid;

const TABLE: &str = "ai_messages";
const COLUMN: &str = "sequence_number";
const CREATED_INDEX: &str = "ix_ai_messages_conversation_created";
const SEQUENCE_INDEX: &str = "uq_ai_messages_conversation_sequence";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .add_column(big_integer(Alias::new(COLUMN)).default(0_i64))
                    .to_owned(),
            )
            .await?;
        backfill_sequences(manager).await?;
        manager
            .create_index(
                Index::create()
                    .name(SEQUENCE_INDEX)
                    .table(Alias::new(TABLE))
                    .col(Alias::new("conversation_id"))
                    .col(Alias::new(COLUMN))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name(CREATED_INDEX)
                    .table(Alias::new(TABLE))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name(CREATED_INDEX)
                    .table(Alias::new(TABLE))
                    .col(Alias::new("conversation_id"))
                    .col(Alias::new("created_at"))
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name(SEQUENCE_INDEX)
                    .table(Alias::new(TABLE))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .drop_column(Alias::new(COLUMN))
                    .to_owned(),
            )
            .await
    }
}

async fn backfill_sequences(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let transaction = manager.get_connection().begin().await?;
    let backend = transaction.get_database_backend();
    let select = Query::select()
        .columns([Alias::new("id"), Alias::new("conversation_id")])
        .from(Alias::new(TABLE))
        .order_by(
            Alias::new("conversation_id"),
            sea_orm::sea_query::Order::Asc,
        )
        .order_by(Alias::new("created_at"), sea_orm::sea_query::Order::Asc)
        .order_by(Alias::new("id"), sea_orm::sea_query::Order::Asc)
        .to_owned();
    let mut conversation = None;
    let mut sequence = 0_i64;
    for row in transaction.query_all(backend.build(&select)).await? {
        let id: Uuid = row.try_get("", "id")?;
        let row_conversation: Uuid = row.try_get("", "conversation_id")?;
        if conversation != Some(row_conversation) {
            conversation = Some(row_conversation);
            sequence = 0;
        }
        let update = Query::update()
            .table(Alias::new(TABLE))
            .value(Alias::new(COLUMN), sequence)
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        transaction.execute(backend.build(&update)).await?;
        sequence = sequence.checked_add(1).ok_or_else(|| {
            DbErr::Custom("AI message sequence overflow during migration".to_owned())
        })?;
    }
    transaction.commit().await
}
