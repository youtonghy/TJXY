use std::collections::{BTreeMap, HashMap, HashSet};

use axum::{
    Json,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr, QueryResult,
    sea_query::{Alias, Expr, JoinType, Order, Query},
};
use serde::Serialize;
use tjxy_db::{DashboardRepository, DashboardTopItem};
use tjxy_metadata::TmdbPopularItem;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{AppState, auth, metadata_settings_admin::MetadataSettingsAdminService};

pub(crate) struct ClientPortalService {
    database: DatabaseConnection,
    tmdb_cache: RwLock<Option<TmdbPopularCache>>,
}

impl ClientPortalService {
    pub(crate) fn new(database: DatabaseConnection) -> Self {
        Self {
            database,
            tmdb_cache: RwLock::new(None),
        }
    }

    async fn user_insights(
        &self,
        user_id: Uuid,
        range: InsightRange,
    ) -> Result<UserInsightsDto, ClientPortalError> {
        let now = Utc::now();
        let from = range.from(now);
        let sessions = self.user_sessions(user_id, from, now).await?;
        let mut watched_ticks = 0_i64;
        let mut unique_titles = HashSet::new();
        let mut daily = BTreeMap::<NaiveDate, i64>::new();
        let mut movies = 0_u64;
        let mut series = 0_u64;
        let mut recent = Vec::new();
        let mut recent_ids = HashSet::new();
        for session in &sessions {
            watched_ticks = watched_ticks.saturating_add(session.watched_ticks.max(0));
            unique_titles.insert(session.item_id);
            *daily.entry(session.started_at.date_naive()).or_default() +=
                session.watched_ticks.max(0);
            if session.item_type == "Movie" {
                movies += 1;
            } else if matches!(session.item_type.as_str(), "Series" | "Episode") {
                series += 1;
            }
            if recent_ids.insert(session.item_id) && recent.len() < 12 {
                recent.push(MediaItemDto {
                    id: session.item_id,
                    name: session.name.clone(),
                    item_type: session.item_type.clone(),
                    production_year: session.production_year,
                });
            }
        }
        let genres = self.user_genres(user_id, from, now).await?;
        Ok(UserInsightsDto {
            watched_ticks,
            play_count: sessions.len() as u64,
            unique_titles: unique_titles.len() as u64,
            media: InsightMediaDto { movies, series },
            daily: daily
                .into_iter()
                .map(|(date, watched_ticks)| InsightDailyDto {
                    date: date.to_string(),
                    watched_ticks,
                })
                .collect(),
            genres,
            recent,
        })
    }

    async fn user_sessions(
        &self,
        user_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<UserSessionRow>, DbErr> {
        let sessions = Alias::new("ps");
        let items = Alias::new("ci");
        let mut query = Query::select();
        query
            .from_as(Alias::new("playback_sessions"), sessions.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("catalog_items"),
                items.clone(),
                Expr::col((items.clone(), Alias::new("id")))
                    .equals((sessions.clone(), Alias::new("catalog_item_id"))),
            )
            .and_where(Expr::col((sessions.clone(), Alias::new("user_id"))).eq(user_id))
            .and_where(Expr::col((sessions.clone(), Alias::new("started_at"))).gte(from))
            .and_where(Expr::col((sessions.clone(), Alias::new("started_at"))).lt(to))
            .order_by((sessions.clone(), Alias::new("last_event_at")), Order::Desc)
            .expr_as(
                Expr::col((sessions.clone(), Alias::new("catalog_item_id"))),
                Alias::new("item_id"),
            )
            .expr_as(
                Expr::col((sessions.clone(), Alias::new("watched_ticks"))),
                Alias::new("watched_ticks"),
            )
            .expr_as(
                Expr::col((sessions.clone(), Alias::new("started_at"))),
                Alias::new("started_at"),
            );
        for (column, alias) in [
            ("name", "name"),
            ("item_type", "item_type"),
            ("production_year", "production_year"),
        ] {
            query.expr_as(
                Expr::col((items.clone(), Alias::new(column))),
                Alias::new(alias),
            );
        }
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(UserSessionRow::from_row)
            .collect()
    }

    async fn user_genres(
        &self,
        user_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<InsightGenreDto>, DbErr> {
        let sessions = Alias::new("ps");
        let links = Alias::new("ig");
        let genres = Alias::new("g");
        let mut query = Query::select();
        query
            .from_as(Alias::new("playback_sessions"), sessions.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("item_genres"),
                links.clone(),
                Expr::col((links.clone(), Alias::new("catalog_item_id")))
                    .equals((sessions.clone(), Alias::new("catalog_item_id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("genres"),
                genres.clone(),
                Expr::col((genres.clone(), Alias::new("id")))
                    .equals((links.clone(), Alias::new("genre_id"))),
            )
            .and_where(Expr::col((sessions.clone(), Alias::new("user_id"))).eq(user_id))
            .and_where(Expr::col((sessions.clone(), Alias::new("started_at"))).gte(from))
            .and_where(Expr::col((sessions.clone(), Alias::new("started_at"))).lt(to))
            .expr_as(
                Expr::col((genres.clone(), Alias::new("name"))),
                Alias::new("name"),
            )
            .expr_as(
                Expr::col((sessions.clone(), Alias::new("watched_ticks"))),
                Alias::new("watched_ticks"),
            );
        let mut totals = HashMap::<String, i64>::new();
        for row in self
            .database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
        {
            let name: String = row.try_get("", "name")?;
            let watched: i64 = row.try_get("", "watched_ticks")?;
            *totals.entry(name).or_default() += watched.max(0);
        }
        let mut totals = totals
            .into_iter()
            .map(|(name, watched_ticks)| InsightGenreDto {
                name,
                watched_ticks,
            })
            .collect::<Vec<_>>();
        totals.sort_by(|left, right| {
            right
                .watched_ticks
                .cmp(&left.watched_ticks)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(totals)
    }

    async fn server_top(&self, limit: u64) -> Result<Vec<DashboardTopItem>, DbErr> {
        let today = start_of_day(Utc::now());
        DashboardRepository::new(&self.database)
            .top_items(today - Duration::days(1), today, limit)
            .await
    }

    async fn local_popular(&self, limit: u64) -> Result<Vec<DashboardTopItem>, DbErr> {
        let to = Utc::now();
        let items = DashboardRepository::new(&self.database)
            .top_items(to - Duration::days(30), to, limit)
            .await?;
        if items.is_empty() {
            self.latest_visible_items(limit).await
        } else {
            Ok(items)
        }
    }

    async fn latest_visible_items(&self, limit: u64) -> Result<Vec<DashboardTopItem>, DbErr> {
        let query = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("name"),
                Alias::new("item_type"),
                Alias::new("production_year"),
            ])
            .from(Alias::new("catalog_items"))
            .and_where(Expr::col(Alias::new("is_present")).eq(true))
            .and_where(Expr::col(Alias::new("classification_state")).eq("Matched"))
            .and_where(Expr::col(Alias::new("item_type")).is_in(["Movie", "Series", "Episode"]))
            .order_by(Alias::new("created_at"), Order::Desc)
            .order_by(Alias::new("name"), Order::Asc)
            .limit(limit)
            .to_owned();
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(|row| {
                Ok(DashboardTopItem {
                    item_id: row.try_get("", "id")?,
                    name: row.try_get("", "name")?,
                    item_type: row.try_get("", "item_type")?,
                    production_year: row.try_get("", "production_year")?,
                    play_count: 0,
                    unique_viewers: 0,
                })
            })
            .collect()
    }

    async fn tmdb_popular(
        &self,
        metadata_settings: &MetadataSettingsAdminService,
        media_type: TmdbMediaType,
    ) -> Result<Vec<TmdbPopularItem>, ClientPortalError> {
        let today = Utc::now().date_naive();
        if let Some(items) = self.cached_tmdb(today, media_type).await {
            return Ok(items);
        }
        let client = metadata_settings.tmdb_catalog_client().await?;
        let movies = client.popular_movies(1).await;
        let series = client.popular_series(1).await;
        match (movies, series) {
            (Ok(movies), Ok(series)) => {
                let selected = match media_type {
                    TmdbMediaType::Movie => movies.clone(),
                    TmdbMediaType::Series => series.clone(),
                };
                *self.tmdb_cache.write().await = Some(TmdbPopularCache {
                    refreshed_on: today,
                    movies,
                    series,
                });
                Ok(selected)
            }
            (movie_error, series_error) => {
                if let Some(stale) = self.tmdb_cache.read().await.as_ref() {
                    return Ok(match media_type {
                        TmdbMediaType::Movie => stale.movies.clone(),
                        TmdbMediaType::Series => stale.series.clone(),
                    });
                }
                Err(ClientPortalError::Tmdb(format!(
                    "movies={movie_error:?}; series={series_error:?}"
                )))
            }
        }
    }

    async fn cached_tmdb(
        &self,
        today: NaiveDate,
        media_type: TmdbMediaType,
    ) -> Option<Vec<TmdbPopularItem>> {
        self.tmdb_cache
            .read()
            .await
            .as_ref()
            .filter(|cache| cache.refreshed_on == today)
            .map(|cache| match media_type {
                TmdbMediaType::Movie => cache.movies.clone(),
                TmdbMediaType::Series => cache.series.clone(),
            })
    }
}

pub(crate) async fn insights(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Some(range) = parse_insight_range(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.client_portal.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .user_insights(principal.user().id().as_uuid(), range)
        .await
    {
        Ok(value) => Json(value).into_response(),
        Err(error) => service_error("user insights", &error),
    }
}

pub(crate) async fn popular(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Some(limit) = parse_limit(raw_query.as_deref(), 12) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.client_portal.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.local_popular(limit).await {
        Ok(items) => Json(ItemPageDto::from_top(items)).into_response(),
        Err(error) => service_error("local popular titles", &error.into()),
    }
}

pub(crate) async fn server_top(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Some(limit) = parse_server_top(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.client_portal.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.server_top(limit).await {
        Ok(items) => Json(ServerTopPageDto::from(items)).into_response(),
        Err(error) => service_error("server ranking", &error.into()),
    }
}

pub(crate) async fn tmdb_top(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Some(media_type) = parse_tmdb_type(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.client_portal.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(metadata_settings) = state.metadata_settings_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.tmdb_popular(metadata_settings, media_type).await {
        Ok(items) => Json(TmdbPageDto::from(items)).into_response(),
        Err(error) => service_error("TMDB ranking", &error),
    }
}

fn parse_insight_range(raw_query: Option<&str>) -> Option<InsightRange> {
    let mut query = clean_query(raw_query)?;
    let range = match query.remove("range")?.as_str() {
        "today" => InsightRange::Today,
        "7d" => InsightRange::SevenDays,
        "30d" => InsightRange::ThirtyDays,
        "all" => InsightRange::All,
        _ => return None,
    };
    query.is_empty().then_some(range)
}

fn parse_limit(raw_query: Option<&str>, default: u64) -> Option<u64> {
    let mut query = clean_query(raw_query)?;
    let limit = query
        .remove("limit")
        .map_or(Some(default), |value| value.parse().ok())?;
    (query.is_empty() && (1..=50).contains(&limit)).then_some(limit)
}

fn parse_server_top(raw_query: Option<&str>) -> Option<u64> {
    let mut query = clean_query(raw_query)?;
    if query.remove("period").as_deref() != Some("yesterday") {
        return None;
    }
    let limit = query
        .remove("limit")
        .map_or(Some(20), |value| value.parse().ok())?;
    (query.is_empty() && (1..=50).contains(&limit)).then_some(limit)
}

fn parse_tmdb_type(raw_query: Option<&str>) -> Option<TmdbMediaType> {
    let mut query = clean_query(raw_query)?;
    let media_type = match query.remove("mediaType")?.as_str() {
        "Movie" => TmdbMediaType::Movie,
        "Series" => TmdbMediaType::Series,
        _ => return None,
    };
    query.is_empty().then_some(media_type)
}

fn clean_query(raw_query: Option<&str>) -> Option<HashMap<String, String>> {
    let mut query = auth::request_query(raw_query).ok()?;
    query.remove("ApiKey");
    query.remove("api_key");
    Some(query)
}

fn start_of_day(now: DateTime<Utc>) -> DateTime<Utc> {
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_utc()
}

fn service_error(context: &str, error: &ClientPortalError) -> Response {
    eprintln!("Client portal {context} failed: {error}");
    StatusCode::SERVICE_UNAVAILABLE.into_response()
}

#[derive(Clone, Copy)]
enum InsightRange {
    Today,
    SevenDays,
    ThirtyDays,
    All,
}

impl InsightRange {
    fn from(self, now: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Self::Today => start_of_day(now),
            Self::SevenDays => start_of_day(now) - Duration::days(6),
            Self::ThirtyDays => start_of_day(now) - Duration::days(29),
            Self::All => DateTime::from_timestamp(0, 0).expect("Unix epoch is valid"),
        }
    }
}

#[derive(Clone, Copy)]
enum TmdbMediaType {
    Movie,
    Series,
}

struct TmdbPopularCache {
    refreshed_on: NaiveDate,
    movies: Vec<TmdbPopularItem>,
    series: Vec<TmdbPopularItem>,
}

struct UserSessionRow {
    item_id: Uuid,
    name: String,
    item_type: String,
    production_year: Option<i32>,
    watched_ticks: i64,
    started_at: DateTime<Utc>,
}

impl UserSessionRow {
    fn from_row(row: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            item_id: row.try_get("", "item_id")?,
            name: row.try_get("", "name")?,
            item_type: row.try_get("", "item_type")?,
            production_year: row.try_get("", "production_year")?,
            watched_ticks: row.try_get("", "watched_ticks")?,
            started_at: row.try_get("", "started_at")?,
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UserInsightsDto {
    watched_ticks: i64,
    play_count: u64,
    unique_titles: u64,
    media: InsightMediaDto,
    daily: Vec<InsightDailyDto>,
    genres: Vec<InsightGenreDto>,
    recent: Vec<MediaItemDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct InsightMediaDto {
    movies: u64,
    series: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct InsightDailyDto {
    date: String,
    watched_ticks: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct InsightGenreDto {
    name: String,
    watched_ticks: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct MediaItemDto {
    id: Uuid,
    name: String,
    #[serde(rename = "Type")]
    item_type: String,
    production_year: Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ItemPageDto {
    items: Vec<MediaItemDto>,
    total_record_count: usize,
    start_index: u64,
}

impl ItemPageDto {
    fn from_top(items: Vec<DashboardTopItem>) -> Self {
        let total_record_count = items.len();
        Self {
            items: items
                .into_iter()
                .map(|item| MediaItemDto {
                    id: item.item_id,
                    name: item.name,
                    item_type: item.item_type,
                    production_year: item.production_year,
                })
                .collect(),
            total_record_count,
            start_index: 0,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ServerTopPageDto {
    items: Vec<ServerTopItemDto>,
}

impl From<Vec<DashboardTopItem>> for ServerTopPageDto {
    fn from(items: Vec<DashboardTopItem>) -> Self {
        Self {
            items: items
                .into_iter()
                .enumerate()
                .map(|(index, item)| ServerTopItemDto {
                    rank: index as u64 + 1,
                    id: item.item_id,
                    name: item.name,
                    item_type: item.item_type,
                    production_year: item.production_year,
                    play_count: item.play_count,
                    unique_viewers: item.unique_viewers,
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ServerTopItemDto {
    rank: u64,
    id: Uuid,
    name: String,
    item_type: String,
    production_year: Option<i32>,
    play_count: u64,
    unique_viewers: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct TmdbPageDto {
    items: Vec<TmdbItemDto>,
}

impl From<Vec<TmdbPopularItem>> for TmdbPageDto {
    fn from(items: Vec<TmdbPopularItem>) -> Self {
        Self {
            items: items
                .iter()
                .enumerate()
                .map(|(index, item)| TmdbItemDto {
                    rank: index as u64 + 1,
                    tmdb_id: item.id(),
                    name: item.name().to_owned(),
                    overview: item.overview().map(str::to_owned),
                    production_year: item.year(),
                    rating: item.rating(),
                    poster_url: item.poster_url().map(str::to_owned),
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct TmdbItemDto {
    rank: u64,
    tmdb_id: u64,
    name: String,
    overview: Option<String>,
    production_year: Option<i32>,
    rating: Option<f64>,
    poster_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
enum ClientPortalError {
    #[error("database query failed: {0}")]
    Database(#[from] DbErr),
    #[error("metadata settings unavailable: {0}")]
    MetadataSettings(#[from] crate::metadata_settings_admin::MetadataSettingsAdminError),
    #[error("TMDB request failed: {0}")]
    Tmdb(String),
}
