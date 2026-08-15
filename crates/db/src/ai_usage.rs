use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, DbErr, QueryResult,
    RuntimeErr, SqlxError, TransactionTrait,
    sea_query::{Alias, Expr, JoinType, OnConflict, Order, Query},
};
use thiserror::Error;
use tjxy_common::UserId;
use uuid::Uuid;

const MAX_ANALYTICS_ROWS: u64 = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiExecutionOutcome {
    Success,
    UpstreamRejected,
    UpstreamInvalid,
    UpstreamTimeout,
    ToolFailed,
    PersistenceFailed,
    InternalError,
}

impl AiExecutionOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::UpstreamRejected => "upstream_rejected",
            Self::UpstreamInvalid => "upstream_invalid",
            Self::UpstreamTimeout => "upstream_timeout",
            Self::ToolFailed => "tool_failed",
            Self::PersistenceFailed => "persistence_failed",
            Self::InternalError => "internal_error",
        }
    }

    fn parse(value: &str) -> Result<Self, AiUsageRepositoryError> {
        match value {
            "success" => Ok(Self::Success),
            "upstream_rejected" => Ok(Self::UpstreamRejected),
            "upstream_invalid" => Ok(Self::UpstreamInvalid),
            "upstream_timeout" => Ok(Self::UpstreamTimeout),
            "tool_failed" => Ok(Self::ToolFailed),
            "persistence_failed" => Ok(Self::PersistenceFailed),
            "internal_error" => Ok(Self::InternalError),
            _ => Err(AiUsageRepositoryError::InvalidStoredOutcome),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AiExecutionInput {
    user_id: UserId,
    model_id: Uuid,
    model_display_name: String,
    upstream_model_id: String,
    day_key: String,
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
    elapsed_ms: u64,
    outcome: AiExecutionOutcome,
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

impl AiExecutionInput {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        user_id: UserId,
        model_id: Uuid,
        model_display_name: impl Into<String>,
        upstream_model_id: impl Into<String>,
        day_key: impl Into<String>,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
        elapsed_ms: u64,
        outcome: AiExecutionOutcome,
    ) -> Self {
        Self {
            user_id,
            model_id,
            model_display_name: model_display_name.into(),
            upstream_model_id: upstream_model_id.into(),
            day_key: day_key.into(),
            started_at,
            completed_at,
            elapsed_ms,
            outcome,
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        }
    }

    #[must_use]
    pub const fn with_usage(mut self, prompt_tokens: u64, completion_tokens: u64) -> Self {
        self.prompt_tokens = Some(prompt_tokens);
        self.completion_tokens = Some(completion_tokens);
        self.total_tokens = prompt_tokens.checked_add(completion_tokens);
        self
    }

    #[must_use]
    pub const fn with_unknown_usage(self) -> Self {
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiUsageSummary {
    pub total_requests: u64,
    pub active_users: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub known_token_requests: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiUsageDaily {
    pub day: String,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_tokens: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiUsageUser {
    pub user_id: Uuid,
    pub username: String,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub total_tokens: Option<u64>,
    pub last_used_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiUsageModel {
    pub model_id: Uuid,
    pub display_name: String,
    pub upstream_model_id: String,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub total_tokens: Option<u64>,
    pub last_used_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiUsageFailure {
    pub id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub model_id: Uuid,
    pub model_display_name: String,
    pub outcome: AiExecutionOutcome,
    pub elapsed_ms: u64,
    pub started_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiUsageAnalytics {
    pub summary: AiUsageSummary,
    pub daily: Vec<AiUsageDaily>,
    pub users: Vec<AiUsageUser>,
    pub models: Vec<AiUsageModel>,
    pub recent_failures: Vec<AiUsageFailure>,
}

pub struct AiUsageRepository<'a> {
    database: &'a DatabaseConnection,
}

impl<'a> AiUsageRepository<'a> {
    #[must_use]
    pub const fn new(database: &'a DatabaseConnection) -> Self {
        Self { database }
    }

    /// Records one completed AI execution without storing message or provider error content.
    ///
    /// # Errors
    ///
    /// Returns [`AiUsageRepositoryError::InvalidInput`] when the bounded execution snapshot is
    /// inconsistent, or [`AiUsageRepositoryError::Database`] when persistence fails.
    pub async fn record(&self, input: &AiExecutionInput) -> Result<(), AiUsageRepositoryError> {
        validate_input(input)?;
        let statement = Query::insert()
            .into_table(Alias::new("ai_execution_records"))
            .columns([
                Alias::new("id"),
                Alias::new("user_id"),
                Alias::new("model_id"),
                Alias::new("model_display_name"),
                Alias::new("upstream_model_id"),
                Alias::new("day_key"),
                Alias::new("started_at"),
                Alias::new("completed_at"),
                Alias::new("elapsed_ms"),
                Alias::new("outcome"),
                Alias::new("prompt_tokens"),
                Alias::new("completion_tokens"),
                Alias::new("total_tokens"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                input.user_id.as_uuid().into(),
                input.model_id.into(),
                input.model_display_name.as_str().into(),
                input.upstream_model_id.as_str().into(),
                input.day_key.as_str().into(),
                input.started_at.into(),
                input.completed_at.into(),
                i64::try_from(input.elapsed_ms)
                    .map_err(|_| AiUsageRepositoryError::InvalidInput)?
                    .into(),
                input.outcome.as_str().into(),
                optional_i64(input.prompt_tokens)?.into(),
                optional_i64(input.completion_tokens)?.into(),
                optional_i64(input.total_tokens)?.into(),
            ])
            .to_owned();
        self.database
            .execute(self.database.get_database_backend().build(&statement))
            .await?;
        Ok(())
    }

    /// Atomically consumes one request from a user's daily AI quota.
    ///
    /// # Errors
    ///
    /// Returns [`AiUsageRepositoryError::InvalidInput`] when `limit` is zero, or a database
    /// error when the quota row cannot be persisted.
    pub async fn try_consume_daily_quota(
        &self,
        user_id: UserId,
        usage_day: NaiveDate,
        limit: u32,
    ) -> Result<bool, AiUsageRepositoryError> {
        if limit == 0 {
            return Err(AiUsageRepositoryError::InvalidInput);
        }
        let day_key = usage_day.format("%Y-%m-%d").to_string();
        let backend = self.database.get_database_backend();
        for attempt in 0..3 {
            let transaction = self.database.begin().await?;
            let result =
                consume_daily_quota(&transaction, user_id, &day_key, i64::from(limit)).await;
            match finish(transaction, result).await {
                Err(error)
                    if backend == DbBackend::MySql
                        && attempt < 2
                        && retryable_serialization_failure(&error) => {}
                result => return result,
            }
        }
        unreachable!("bounded retry loop always returns")
    }

    /// Returns the number of AI requests recorded for a user on one UTC day.
    ///
    /// # Errors
    ///
    /// Returns a database error when the quota count cannot be read.
    pub async fn daily_quota_count(
        &self,
        user_id: UserId,
        usage_day: NaiveDate,
    ) -> Result<u64, AiUsageRepositoryError> {
        let day_key = usage_day.format("%Y-%m-%d").to_string();
        let query = Query::select()
            .expr(Expr::col(Alias::new("request_count")))
            .from(Alias::new("ai_daily_usage"))
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
            .and_where(Expr::col(Alias::new("day_key")).eq(day_key))
            .to_owned();
        let Some(row) = self
            .database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
        else {
            return Ok(0);
        };
        unsigned(row.try_get("", "request_count")?)
    }

    /// Returns the recorded token usage for a user on one UTC day.
    pub async fn daily_token_usage(
        &self,
        user_id: UserId,
        usage_day: NaiveDate,
    ) -> Result<u64, AiUsageRepositoryError> {
        self.token_usage(Some(user_id), usage_day).await
    }

    /// Returns the recorded token usage across all users on one UTC day.
    pub async fn daily_total_token_usage(
        &self,
        usage_day: NaiveDate,
    ) -> Result<u64, AiUsageRepositoryError> {
        self.token_usage(None, usage_day).await
    }

    async fn token_usage(
        &self,
        user_id: Option<UserId>,
        usage_day: NaiveDate,
    ) -> Result<u64, AiUsageRepositoryError> {
        let mut query = Query::select()
            .expr_as(
                Expr::col(Alias::new("total_tokens")).sum(),
                Alias::new("token_total"),
            )
            .from(Alias::new("ai_execution_records"))
            .and_where(Expr::col(Alias::new("day_key")).eq(usage_day.to_string()))
            .to_owned();
        if let Some(user_id) = user_id {
            query.and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()));
        }
        let Some(row) = self
            .database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
        else {
            return Ok(0);
        };
        let value: Option<i64> = row.try_get("", "token_total")?;
        value.map_or(Ok(0), unsigned)
    }

    /// Aggregates a bounded local-day window for the administrator analytics view.
    ///
    /// # Errors
    ///
    /// Returns [`AiUsageRepositoryError::InvalidInput`] for invalid dates, ranges, or limits;
    /// stored-data and database failures are surfaced through their corresponding variants.
    pub async fn analytics(
        &self,
        today: &str,
        trend_start: &str,
        trend_end: &str,
        limit: u64,
    ) -> Result<AiUsageAnalytics, AiUsageRepositoryError> {
        validate_day(today)?;
        let start = validate_day(trend_start)?;
        let end = validate_day(trend_end)?;
        if start > end || limit == 0 || limit > MAX_ANALYTICS_ROWS {
            return Err(AiUsageRepositoryError::InvalidInput);
        }
        Ok(AiUsageAnalytics {
            summary: self.summary(today).await?,
            daily: self.daily(start, end).await?,
            users: self.users(today, limit).await?,
            models: self.models(today, limit).await?,
            recent_failures: self.failures(trend_start, trend_end, limit).await?,
        })
    }

    async fn summary(&self, day: &str) -> Result<AiUsageSummary, AiUsageRepositoryError> {
        let query = aggregate_query(self.database.get_database_backend())
            .from(Alias::new("ai_execution_records"))
            .and_where(Expr::col(Alias::new("day_key")).eq(day))
            .to_owned();
        let row = self
            .database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
            .ok_or(AiUsageRepositoryError::MissingAggregate)?;
        aggregate_summary(&row, distinct_count(&row, "active_users")?)
    }

    async fn daily(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<AiUsageDaily>, AiUsageRepositoryError> {
        let mut query = aggregate_query(self.database.get_database_backend());
        query
            .column(Alias::new("day_key"))
            .from(Alias::new("ai_execution_records"))
            .and_where(Expr::col(Alias::new("day_key")).gte(start.to_string()))
            .and_where(Expr::col(Alias::new("day_key")).lte(end.to_string()))
            .group_by_col(Alias::new("day_key"))
            .order_by(Alias::new("day_key"), Order::Asc);
        let mut by_day = self
            .database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .into_iter()
            .map(|row| {
                let day: String = row.try_get("", "day_key")?;
                let summary = aggregate_summary(&row, 0)?;
                Ok((
                    day.clone(),
                    AiUsageDaily {
                        day,
                        total_requests: summary.total_requests,
                        successful_requests: summary.successful_requests,
                        failed_requests: summary.failed_requests,
                        total_tokens: summary.total_tokens,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>, AiUsageRepositoryError>>()?;
        let mut rows = Vec::new();
        let mut day = start;
        while day <= end {
            let key = day.to_string();
            rows.push(by_day.remove(&key).unwrap_or(AiUsageDaily {
                day: key,
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                total_tokens: Some(0),
            }));
            day += Duration::days(1);
        }
        Ok(rows)
    }

    async fn users(
        &self,
        day: &str,
        limit: u64,
    ) -> Result<Vec<AiUsageUser>, AiUsageRepositoryError> {
        let records = Alias::new("r");
        let users = Alias::new("u");
        let mut query = aggregate_query_for(&records, self.database.get_database_backend());
        query
            .column((records.clone(), Alias::new("user_id")))
            .expr_as(
                Expr::col((users.clone(), Alias::new("username"))),
                Alias::new("username"),
            )
            .expr_as(
                Expr::col((records.clone(), Alias::new("completed_at"))).max(),
                Alias::new("last_used_at"),
            )
            .from_as(Alias::new("ai_execution_records"), records.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("users"),
                users.clone(),
                Expr::col((users.clone(), Alias::new("id")))
                    .equals((records.clone(), Alias::new("user_id"))),
            )
            .and_where(Expr::col((records.clone(), Alias::new("day_key"))).eq(day))
            .group_by_col((records.clone(), Alias::new("user_id")))
            .group_by_col((users, Alias::new("username")))
            .order_by(Alias::new("total_requests"), Order::Desc)
            .order_by(Alias::new("username"), Order::Asc)
            .limit(limit);
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(|row| {
                let summary = aggregate_summary(row, 0)?;
                Ok(AiUsageUser {
                    user_id: row.try_get("", "user_id")?,
                    username: row.try_get("", "username")?,
                    total_requests: summary.total_requests,
                    successful_requests: summary.successful_requests,
                    total_tokens: summary.total_tokens,
                    last_used_at: row.try_get("", "last_used_at")?,
                })
            })
            .collect()
    }

    async fn models(
        &self,
        day: &str,
        limit: u64,
    ) -> Result<Vec<AiUsageModel>, AiUsageRepositoryError> {
        let mut query = aggregate_query(self.database.get_database_backend());
        query
            .columns([
                Alias::new("model_id"),
                Alias::new("model_display_name"),
                Alias::new("upstream_model_id"),
            ])
            .expr_as(
                Expr::col(Alias::new("completed_at")).max(),
                Alias::new("last_used_at"),
            )
            .from(Alias::new("ai_execution_records"))
            .and_where(Expr::col(Alias::new("day_key")).eq(day))
            .group_by_col(Alias::new("model_id"))
            .group_by_col(Alias::new("model_display_name"))
            .group_by_col(Alias::new("upstream_model_id"))
            .order_by(Alias::new("total_requests"), Order::Desc)
            .order_by(Alias::new("model_display_name"), Order::Asc)
            .limit(limit);
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(|row| {
                let summary = aggregate_summary(row, 0)?;
                Ok(AiUsageModel {
                    model_id: row.try_get("", "model_id")?,
                    display_name: row.try_get("", "model_display_name")?,
                    upstream_model_id: row.try_get("", "upstream_model_id")?,
                    total_requests: summary.total_requests,
                    successful_requests: summary.successful_requests,
                    total_tokens: summary.total_tokens,
                    last_used_at: row.try_get("", "last_used_at")?,
                })
            })
            .collect()
    }

    async fn failures(
        &self,
        start: &str,
        end: &str,
        limit: u64,
    ) -> Result<Vec<AiUsageFailure>, AiUsageRepositoryError> {
        let records = Alias::new("r");
        let users = Alias::new("u");
        let query = Query::select()
            .columns([
                (records.clone(), Alias::new("id")),
                (records.clone(), Alias::new("user_id")),
                (records.clone(), Alias::new("model_id")),
                (records.clone(), Alias::new("model_display_name")),
                (records.clone(), Alias::new("outcome")),
                (records.clone(), Alias::new("elapsed_ms")),
                (records.clone(), Alias::new("started_at")),
            ])
            .expr_as(
                Expr::col((users.clone(), Alias::new("username"))),
                Alias::new("username"),
            )
            .from_as(Alias::new("ai_execution_records"), records.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("users"),
                users.clone(),
                Expr::col((users, Alias::new("id")))
                    .equals((records.clone(), Alias::new("user_id"))),
            )
            .and_where(Expr::col((records.clone(), Alias::new("day_key"))).gte(start))
            .and_where(Expr::col((records.clone(), Alias::new("day_key"))).lte(end))
            .and_where(Expr::col((records.clone(), Alias::new("outcome"))).ne("success"))
            .order_by((records, Alias::new("started_at")), Order::Desc)
            .limit(limit)
            .to_owned();
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(|row| {
                Ok(AiUsageFailure {
                    id: row.try_get("", "id")?,
                    user_id: row.try_get("", "user_id")?,
                    username: row.try_get("", "username")?,
                    model_id: row.try_get("", "model_id")?,
                    model_display_name: row.try_get("", "model_display_name")?,
                    outcome: AiExecutionOutcome::parse(&row.try_get::<String>("", "outcome")?)?,
                    elapsed_ms: unsigned(row.try_get("", "elapsed_ms")?)?,
                    started_at: row.try_get("", "started_at")?,
                })
            })
            .collect()
    }
}

fn aggregate_query(backend: DbBackend) -> sea_orm::sea_query::SelectStatement {
    aggregate_query_for(&Alias::new("ai_execution_records"), backend)
}

fn aggregate_query_for(table: &Alias, backend: DbBackend) -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((table.clone(), Alias::new("id"))).count(),
            Alias::new("total_requests"),
        )
        .expr_as(
            integer_sum(
                "COALESCE(SUM(CASE WHEN outcome = 'success' THEN 1 ELSE 0 END), 0)",
                backend,
            ),
            Alias::new("successful_requests"),
        )
        .expr_as(
            Expr::col((table.clone(), Alias::new("total_tokens"))).count(),
            Alias::new("known_token_requests"),
        )
        .expr_as(
            integer_sum("SUM(prompt_tokens)", backend),
            Alias::new("prompt_tokens"),
        )
        .expr_as(
            integer_sum("SUM(completion_tokens)", backend),
            Alias::new("completion_tokens"),
        )
        .expr_as(
            integer_sum("SUM(total_tokens)", backend),
            Alias::new("total_tokens"),
        )
        .expr_as(
            Expr::cust("COUNT(DISTINCT user_id)"),
            Alias::new("active_users"),
        )
        .to_owned()
}

fn integer_sum(expression: &str, backend: DbBackend) -> sea_orm::sea_query::SimpleExpr {
    let integer_type = match backend {
        DbBackend::MySql => "SIGNED",
        DbBackend::Postgres => "BIGINT",
        DbBackend::Sqlite => "INTEGER",
    };
    Expr::cust(format!("CAST({expression} AS {integer_type})"))
}

fn retryable_serialization_failure(error: &AiUsageRepositoryError) -> bool {
    let AiUsageRepositoryError::Database(
        DbErr::Exec(RuntimeErr::SqlxError(SqlxError::Database(database)))
        | DbErr::Query(RuntimeErr::SqlxError(SqlxError::Database(database))),
    ) = error
    else {
        return false;
    };
    database.code().as_deref() == Some("40001")
}

fn aggregate_summary(
    row: &QueryResult,
    active_users: u64,
) -> Result<AiUsageSummary, AiUsageRepositoryError> {
    let total_requests = distinct_count(row, "total_requests")?;
    let successful_requests = distinct_count(row, "successful_requests")?;
    let known = distinct_count(row, "known_token_requests")?;
    let all_known = total_requests == 0 || known == total_requests;
    Ok(AiUsageSummary {
        total_requests,
        active_users,
        successful_requests,
        failed_requests: total_requests.saturating_sub(successful_requests),
        prompt_tokens: aggregate_token(row, "prompt_tokens", all_known, total_requests)?,
        completion_tokens: aggregate_token(row, "completion_tokens", all_known, total_requests)?,
        total_tokens: aggregate_token(row, "total_tokens", all_known, total_requests)?,
        known_token_requests: known,
    })
}

fn aggregate_token(
    row: &QueryResult,
    column: &str,
    all_known: bool,
    total_requests: u64,
) -> Result<Option<u64>, AiUsageRepositoryError> {
    if !all_known {
        return Ok(None);
    }
    if total_requests == 0 {
        return Ok(Some(0));
    }
    let value: Option<i64> = row.try_get("", column)?;
    value.map(unsigned).transpose()
}

fn distinct_count(row: &QueryResult, column: &str) -> Result<u64, AiUsageRepositoryError> {
    unsigned(row.try_get("", column)?)
}

fn unsigned(value: i64) -> Result<u64, AiUsageRepositoryError> {
    u64::try_from(value).map_err(|_| AiUsageRepositoryError::InvalidStoredAggregate)
}

fn optional_i64(value: Option<u64>) -> Result<Option<i64>, AiUsageRepositoryError> {
    value
        .map(|value| i64::try_from(value).map_err(|_| AiUsageRepositoryError::InvalidInput))
        .transpose()
}

async fn consume_daily_quota(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    day_key: &str,
    limit: i64,
) -> Result<bool, AiUsageRepositoryError> {
    let now = Utc::now();
    let insert = Query::insert()
        .into_table(Alias::new("ai_daily_usage"))
        .columns([
            Alias::new("id"),
            Alias::new("user_id"),
            Alias::new("day_key"),
            Alias::new("request_count"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            user_id.as_uuid().into(),
            day_key.into(),
            0_i64.into(),
            now.into(),
            now.into(),
        ])
        .on_conflict(idempotent_insert_conflict(
            transaction.get_database_backend(),
        ))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&insert)).await?;

    let update = Query::update()
        .table(Alias::new("ai_daily_usage"))
        .value(
            Alias::new("request_count"),
            Expr::col(Alias::new("request_count")).add(1_i64),
        )
        .value(Alias::new("updated_at"), Utc::now())
        .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
        .and_where(Expr::col(Alias::new("day_key")).eq(day_key))
        .and_where(Expr::col(Alias::new("request_count")).lt(limit))
        .to_owned();
    Ok(transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        == 1)
}

fn idempotent_insert_conflict(backend: DbBackend) -> OnConflict {
    if backend == DbBackend::MySql {
        OnConflict::new().update_column(Alias::new("id")).to_owned()
    } else {
        OnConflict::new().do_nothing().to_owned()
    }
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, AiUsageRepositoryError>,
) -> Result<T, AiUsageRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(AiUsageRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

fn validate_input(input: &AiExecutionInput) -> Result<(), AiUsageRepositoryError> {
    validate_day(&input.day_key)?;
    if input.model_id.is_nil()
        || !valid_text(&input.model_display_name, 128)
        || !valid_text(&input.upstream_model_id, 255)
        || input.completed_at < input.started_at
        || input.total_tokens
            != input
                .prompt_tokens
                .zip(input.completion_tokens)
                .and_then(|(prompt, completion)| prompt.checked_add(completion))
    {
        return Err(AiUsageRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_day(value: &str) -> Result<NaiveDate, AiUsageRepositoryError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| AiUsageRepositoryError::InvalidInput)
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

#[derive(Debug, Error)]
pub enum AiUsageRepositoryError {
    #[error("AI usage input is invalid")]
    InvalidInput,
    #[error("stored AI usage outcome is invalid")]
    InvalidStoredOutcome,
    #[error("stored AI usage aggregate is invalid")]
    InvalidStoredAggregate,
    #[error("AI usage aggregate row is missing")]
    MissingAggregate,
    #[error("AI usage database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("AI usage rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}
