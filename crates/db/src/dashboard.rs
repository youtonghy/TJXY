use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr, QueryResult,
    sea_query::{Alias, Condition, Expr, JoinType, Order, Query},
};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub struct DashboardSnapshot {
    pub users_total: u64,
    pub users_disabled: u64,
    pub catalog_total: u64,
    pub movies: u64,
    pub series: u64,
    pub episodes: u64,
    pub play_count: u64,
    pub unique_viewers: u64,
    pub currently_watching: u64,
    pub events: Vec<DashboardPlaybackEvent>,
    pub top_items: Vec<DashboardTopItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DashboardPlaybackEvent {
    pub user_id: Uuid,
    pub started_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DashboardTopItem {
    pub item_id: Uuid,
    pub name: String,
    pub item_type: String,
    pub production_year: Option<i32>,
    pub play_count: u64,
    pub unique_viewers: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DashboardNowPlaying {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub user_name: String,
    pub item_id: Uuid,
    pub item_name: String,
    pub item_type: String,
    pub runtime_ticks: Option<i64>,
    pub position_ticks: i64,
    pub client_name: String,
    pub device_name: String,
    pub started_at: DateTime<Utc>,
    pub last_event_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DashboardLoginRecord {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub user_name: String,
    pub client_name: String,
    pub client_version: String,
    pub device_name: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DashboardWatchRecord {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub user_name: String,
    pub item_id: Uuid,
    pub item_name: String,
    pub item_type: String,
    pub runtime_ticks: Option<i64>,
    pub position_ticks: i64,
    pub started_at: DateTime<Utc>,
    pub last_event_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DashboardPage<T> {
    pub items: Vec<T>,
    pub total_record_count: u64,
    pub start_index: u64,
}

pub struct DashboardRepository<'a> {
    database: &'a DatabaseConnection,
}

impl<'a> DashboardRepository<'a> {
    #[must_use]
    pub const fn new(database: &'a DatabaseConnection) -> Self {
        Self { database }
    }

    /// Returns bounded dashboard aggregates for the requested UTC range.
    ///
    /// # Errors
    ///
    /// Returns [`DbErr`] when a query or row decode fails.
    pub async fn snapshot(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        active_cutoff: DateTime<Utc>,
        top_limit: u64,
    ) -> Result<DashboardSnapshot, DbErr> {
        let users_total = self.count("users", None).await?;
        let users_disabled = self
            .count(
                "users",
                Some(Expr::col(Alias::new("disabled_at")).is_not_null()),
            )
            .await?;
        let catalog = self.catalog_counts().await?;
        let (play_count, unique_viewers) = self.playback_counts(from, to).await?;
        let currently_watching = self.currently_watching_count(active_cutoff).await?;
        let events = self.playback_events(from, to).await?;
        let top_items = self.top_items(from, to, top_limit).await?;
        Ok(DashboardSnapshot {
            users_total,
            users_disabled,
            catalog_total: catalog.0,
            movies: catalog.1,
            series: catalog.2,
            episodes: catalog.3,
            play_count,
            unique_viewers,
            currently_watching,
            events,
            top_items,
        })
    }

    /// Lists playback sessions with recent heartbeats that have not stopped.
    ///
    /// # Errors
    ///
    /// Returns [`DbErr`] when a query or row decode fails.
    pub async fn now_playing(
        &self,
        active_cutoff: DateTime<Utc>,
    ) -> Result<Vec<DashboardNowPlaying>, DbErr> {
        let ps = Alias::new("ps");
        let users = Alias::new("u");
        let items = Alias::new("ci");
        let sessions = Alias::new("auth");
        let mut query = Query::select();
        query
            .from_as(Alias::new("playback_sessions"), ps.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("users"),
                users.clone(),
                Expr::col((users.clone(), Alias::new("id")))
                    .equals((ps.clone(), Alias::new("user_id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("catalog_items"),
                items.clone(),
                Expr::col((items.clone(), Alias::new("id")))
                    .equals((ps.clone(), Alias::new("catalog_item_id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("auth_sessions"),
                sessions.clone(),
                Expr::col((sessions.clone(), Alias::new("id")))
                    .equals((ps.clone(), Alias::new("auth_session_id"))),
            )
            .and_where(Expr::col((ps.clone(), Alias::new("stopped_at"))).is_null())
            .and_where(Expr::col((ps.clone(), Alias::new("last_event_at"))).gte(active_cutoff))
            .order_by((ps.clone(), Alias::new("last_event_at")), Order::Desc)
            .limit(100);
        select_column(&mut query, &ps, "id", "session_id");
        select_column(&mut query, &ps, "user_id", "user_id");
        select_column(&mut query, &users, "username", "user_name");
        select_column(&mut query, &items, "id", "item_id");
        select_column(&mut query, &items, "name", "item_name");
        select_column(&mut query, &items, "item_type", "item_type");
        select_column(&mut query, &items, "runtime_ticks", "runtime_ticks");
        select_column(&mut query, &ps, "last_position_ticks", "position_ticks");
        select_column(&mut query, &sessions, "client_name", "client_name");
        select_column(&mut query, &sessions, "device_name", "device_name");
        select_column(&mut query, &ps, "started_at", "started_at");
        select_column(&mut query, &ps, "last_event_at", "last_event_at");
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(now_playing_from_row)
            .collect()
    }

    /// Returns a stable page of successful login sessions.
    ///
    /// # Errors
    ///
    /// Returns [`DbErr`] when a query or row decode fails.
    pub async fn login_history(
        &self,
        start_index: u64,
        limit: u64,
    ) -> Result<DashboardPage<DashboardLoginRecord>, DbErr> {
        let sessions = Alias::new("auth");
        let users = Alias::new("u");
        let total_record_count = self.count("auth_sessions", None).await?;
        let mut query = Query::select();
        query
            .from_as(Alias::new("auth_sessions"), sessions.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("users"),
                users.clone(),
                Expr::col((users.clone(), Alias::new("id")))
                    .equals((sessions.clone(), Alias::new("user_id"))),
            )
            .order_by((sessions.clone(), Alias::new("created_at")), Order::Desc)
            .order_by((sessions.clone(), Alias::new("id")), Order::Desc)
            .offset(start_index)
            .limit(limit);
        for column in [
            "id",
            "user_id",
            "client_name",
            "client_version",
            "device_name",
            "created_at",
            "last_seen_at",
            "expires_at",
            "revoked_at",
        ] {
            select_column(&mut query, &sessions, column, column);
        }
        select_column(&mut query, &users, "username", "user_name");
        let items = self
            .database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(login_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DashboardPage {
            items,
            total_record_count,
            start_index,
        })
    }

    /// Returns a stable page of playback attempts.
    ///
    /// # Errors
    ///
    /// Returns [`DbErr`] when a query or row decode fails.
    pub async fn watch_history(
        &self,
        start_index: u64,
        limit: u64,
    ) -> Result<DashboardPage<DashboardWatchRecord>, DbErr> {
        let ps = Alias::new("ps");
        let users = Alias::new("u");
        let items = Alias::new("ci");
        let total_record_count = self.count("playback_sessions", None).await?;
        let mut query = Query::select();
        query
            .from_as(Alias::new("playback_sessions"), ps.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("users"),
                users.clone(),
                Expr::col((users.clone(), Alias::new("id")))
                    .equals((ps.clone(), Alias::new("user_id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("catalog_items"),
                items.clone(),
                Expr::col((items.clone(), Alias::new("id")))
                    .equals((ps.clone(), Alias::new("catalog_item_id"))),
            )
            .order_by((ps.clone(), Alias::new("started_at")), Order::Desc)
            .order_by((ps.clone(), Alias::new("id")), Order::Desc)
            .offset(start_index)
            .limit(limit);
        for column in [
            "id",
            "user_id",
            "last_position_ticks",
            "started_at",
            "last_event_at",
            "stopped_at",
        ] {
            select_column(&mut query, &ps, column, column);
        }
        select_column(&mut query, &users, "username", "user_name");
        select_column(&mut query, &items, "id", "item_id");
        select_column(&mut query, &items, "name", "item_name");
        select_column(&mut query, &items, "item_type", "item_type");
        select_column(&mut query, &items, "runtime_ticks", "runtime_ticks");
        let items = self
            .database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(watch_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DashboardPage {
            items,
            total_record_count,
            start_index,
        })
    }

    async fn count(
        &self,
        table: &str,
        condition: Option<sea_orm::sea_query::SimpleExpr>,
    ) -> Result<u64, DbErr> {
        let mut query = Query::select();
        query
            .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
            .from(Alias::new(table));
        if let Some(condition) = condition {
            query.and_where(condition);
        }
        let row = self
            .database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
            .ok_or_else(|| DbErr::Custom("dashboard count row missing".to_owned()))?;
        count_from_row(&row, "count")
    }

    async fn catalog_counts(&self) -> Result<(u64, u64, u64, u64), DbErr> {
        let mut query = Query::select();
        query
            .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
            .column(Alias::new("item_type"))
            .from(Alias::new("catalog_items"))
            .and_where(Expr::col(Alias::new("is_present")).eq(true))
            .and_where(Expr::col(Alias::new("classification_state")).eq("Matched"))
            .and_where(Expr::col(Alias::new("item_type")).is_in(["Movie", "Series", "Episode"]))
            .group_by_col(Alias::new("item_type"));
        let mut movies = 0;
        let mut series = 0;
        let mut episodes = 0;
        for row in self
            .database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
        {
            let count = count_from_row(&row, "count")?;
            match row.try_get::<String>("", "item_type")?.as_str() {
                "Movie" => movies = count,
                "Series" => series = count,
                "Episode" => episodes = count,
                _ => {}
            }
        }
        Ok((movies + series + episodes, movies, series, episodes))
    }

    async fn playback_counts(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<(u64, u64), DbErr> {
        let mut query = Query::select();
        query
            .expr_as(
                Expr::col(Alias::new("id")).count(),
                Alias::new("play_count"),
            )
            .expr_as(
                Expr::cust("COUNT(DISTINCT user_id)"),
                Alias::new("unique_viewers"),
            )
            .from(Alias::new("playback_sessions"))
            .and_where(Expr::col(Alias::new("started_at")).gte(from))
            .and_where(Expr::col(Alias::new("started_at")).lt(to));
        let row = self
            .database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
            .ok_or_else(|| DbErr::Custom("dashboard playback count row missing".to_owned()))?;
        Ok((
            count_from_row(&row, "play_count")?,
            count_from_row(&row, "unique_viewers")?,
        ))
    }

    async fn currently_watching_count(&self, cutoff: DateTime<Utc>) -> Result<u64, DbErr> {
        let condition = Condition::all()
            .add(Expr::col(Alias::new("stopped_at")).is_null())
            .add(Expr::col(Alias::new("last_event_at")).gte(cutoff));
        let mut query = Query::select();
        query
            .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
            .from(Alias::new("playback_sessions"))
            .cond_where(condition);
        let row = self
            .database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
            .ok_or_else(|| DbErr::Custom("dashboard active count row missing".to_owned()))?;
        count_from_row(&row, "count")
    }

    async fn playback_events(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<DashboardPlaybackEvent>, DbErr> {
        let query = Query::select()
            .columns([Alias::new("user_id"), Alias::new("started_at")])
            .from(Alias::new("playback_sessions"))
            .and_where(Expr::col(Alias::new("started_at")).gte(from))
            .and_where(Expr::col(Alias::new("started_at")).lt(to))
            .order_by(Alias::new("started_at"), Order::Asc)
            .to_owned();
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(|row| {
                Ok(DashboardPlaybackEvent {
                    user_id: row.try_get("", "user_id")?,
                    started_at: row.try_get("", "started_at")?,
                })
            })
            .collect()
    }

    /// Returns the most-started visible catalog items in one UTC range.
    ///
    /// # Errors
    ///
    /// Returns [`DbErr`] when the aggregate query or row decoding fails.
    pub async fn top_items(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: u64,
    ) -> Result<Vec<DashboardTopItem>, DbErr> {
        let ps = Alias::new("ps");
        let items = Alias::new("ci");
        let mut query = Query::select();
        query
            .from_as(Alias::new("playback_sessions"), ps.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("catalog_items"),
                items.clone(),
                Expr::col((items.clone(), Alias::new("id")))
                    .equals((ps.clone(), Alias::new("catalog_item_id"))),
            )
            .and_where(Expr::col((ps.clone(), Alias::new("started_at"))).gte(from))
            .and_where(Expr::col((ps.clone(), Alias::new("started_at"))).lt(to))
            .expr_as(
                Expr::col((ps.clone(), Alias::new("id"))).count(),
                Alias::new("play_count"),
            )
            .expr_as(
                Expr::cust("COUNT(DISTINCT ps.user_id)"),
                Alias::new("unique_viewers"),
            )
            .order_by(Alias::new("play_count"), Order::Desc)
            .order_by((items.clone(), Alias::new("name")), Order::Asc)
            .limit(limit);
        for column in ["id", "name", "item_type", "production_year"] {
            query.group_by_col((items.clone(), Alias::new(column)));
        }
        select_column(&mut query, &items, "id", "item_id");
        select_column(&mut query, &items, "name", "item_name");
        select_column(&mut query, &items, "item_type", "item_type");
        select_column(&mut query, &items, "production_year", "production_year");
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(|row| {
                Ok(DashboardTopItem {
                    item_id: row.try_get("", "item_id")?,
                    name: row.try_get("", "item_name")?,
                    item_type: row.try_get("", "item_type")?,
                    production_year: row.try_get("", "production_year")?,
                    play_count: count_from_row(row, "play_count")?,
                    unique_viewers: count_from_row(row, "unique_viewers")?,
                })
            })
            .collect()
    }
}

fn select_column(
    query: &mut sea_orm::sea_query::SelectStatement,
    table: &Alias,
    column: &str,
    alias: &str,
) {
    query.expr_as(
        Expr::col((table.clone(), Alias::new(column))),
        Alias::new(alias),
    );
}

fn count_from_row(row: &QueryResult, column: &str) -> Result<u64, DbErr> {
    let count = row.try_get::<i64>("", column)?;
    u64::try_from(count).map_err(|_| DbErr::Custom("dashboard count is invalid".to_owned()))
}

fn now_playing_from_row(row: &QueryResult) -> Result<DashboardNowPlaying, DbErr> {
    Ok(DashboardNowPlaying {
        session_id: row.try_get("", "session_id")?,
        user_id: row.try_get("", "user_id")?,
        user_name: row.try_get("", "user_name")?,
        item_id: row.try_get("", "item_id")?,
        item_name: row.try_get("", "item_name")?,
        item_type: row.try_get("", "item_type")?,
        runtime_ticks: row.try_get("", "runtime_ticks")?,
        position_ticks: row.try_get("", "position_ticks")?,
        client_name: row.try_get("", "client_name")?,
        device_name: row.try_get("", "device_name")?,
        started_at: row.try_get("", "started_at")?,
        last_event_at: row.try_get("", "last_event_at")?,
    })
}

fn login_from_row(row: &QueryResult) -> Result<DashboardLoginRecord, DbErr> {
    Ok(DashboardLoginRecord {
        session_id: row.try_get("", "id")?,
        user_id: row.try_get("", "user_id")?,
        user_name: row.try_get("", "user_name")?,
        client_name: row.try_get("", "client_name")?,
        client_version: row.try_get("", "client_version")?,
        device_name: row.try_get("", "device_name")?,
        created_at: row.try_get("", "created_at")?,
        last_seen_at: row.try_get("", "last_seen_at")?,
        expires_at: row.try_get("", "expires_at")?,
        revoked_at: row.try_get("", "revoked_at")?,
    })
}

fn watch_from_row(row: &QueryResult) -> Result<DashboardWatchRecord, DbErr> {
    Ok(DashboardWatchRecord {
        session_id: row.try_get("", "id")?,
        user_id: row.try_get("", "user_id")?,
        user_name: row.try_get("", "user_name")?,
        item_id: row.try_get("", "item_id")?,
        item_name: row.try_get("", "item_name")?,
        item_type: row.try_get("", "item_type")?,
        runtime_ticks: row.try_get("", "runtime_ticks")?,
        position_ticks: row.try_get("", "last_position_ticks")?,
        started_at: row.try_get("", "started_at")?,
        last_event_at: row.try_get("", "last_event_at")?,
        stopped_at: row.try_get("", "stopped_at")?,
    })
}
