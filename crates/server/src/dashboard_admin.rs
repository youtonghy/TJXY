use std::collections::HashSet;

use axum::{
    Json,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use tjxy_db::{
    DashboardLoginRecord, DashboardNowPlaying, DashboardPage, DashboardPlaybackEvent,
    DashboardRepository, DashboardSnapshot, DashboardTopItem, DashboardWatchRecord,
};
use uuid::Uuid;

use crate::{AppState, auth};

const DEFAULT_ACTIVE_SECONDS: i64 = 60;
const DEFAULT_TOP_LIMIT: u64 = 10;
const DEFAULT_PAGE_LIMIT: u64 = 25;
const MAX_PAGE_LIMIT: u64 = 100;
const MAX_RANGE_DAYS: i64 = 31;

pub(crate) struct DashboardAdminService {
    database: sea_orm::DatabaseConnection,
}

impl DashboardAdminService {
    pub(crate) const fn new(database: sea_orm::DatabaseConnection) -> Self {
        Self { database }
    }

    async fn summary(&self, query: SummaryQuery) -> Result<DashboardSummaryDto, sea_orm::DbErr> {
        let active_cutoff = Utc::now() - Duration::seconds(query.active_within_seconds);
        let snapshot = DashboardRepository::new(&self.database)
            .snapshot(query.from, query.to, active_cutoff, query.top_limit)
            .await?;
        Ok(DashboardSummaryDto::from_snapshot(query, snapshot))
    }

    async fn now_playing(
        &self,
        active_within_seconds: i64,
    ) -> Result<Vec<NowPlayingDto>, sea_orm::DbErr> {
        let cutoff = Utc::now() - Duration::seconds(active_within_seconds);
        DashboardRepository::new(&self.database)
            .now_playing(cutoff)
            .await
            .map(|items| items.into_iter().map(NowPlayingDto::from).collect())
    }

    async fn login_history(&self, page: PageQuery) -> Result<LoginHistoryPageDto, sea_orm::DbErr> {
        DashboardRepository::new(&self.database)
            .login_history(page.start_index, page.limit)
            .await
            .map(LoginHistoryPageDto::from)
    }

    async fn watch_history(&self, page: PageQuery) -> Result<WatchHistoryPageDto, sea_orm::DbErr> {
        DashboardRepository::new(&self.database)
            .watch_history(page.start_index, page.limit)
            .await
            .map(WatchHistoryPageDto::from)
    }
}

pub(crate) async fn summary(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Some(query) = parse_summary_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.dashboard_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.summary(query).await {
        Ok(summary) => Json(summary).into_response(),
        Err(error) => {
            tracing::error!("Dashboard summary query failed: {error}");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

pub(crate) async fn now_playing(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Some(active_within_seconds) = parse_active_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.dashboard_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.now_playing(active_within_seconds).await {
        Ok(items) => Json(items).into_response(),
        Err(error) => {
            tracing::error!("Dashboard now-playing query failed: {error}");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

pub(crate) async fn login_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Some(page) = parse_page_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.dashboard_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.login_history(page).await {
        Ok(items) => Json(items).into_response(),
        Err(error) => {
            tracing::error!("Dashboard login-history query failed: {error}");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

pub(crate) async fn watch_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Some(page) = parse_page_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.dashboard_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.watch_history(page).await {
        Ok(items) => Json(items).into_response(),
        Err(error) => {
            tracing::error!("Dashboard watch-history query failed: {error}");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

#[derive(Clone, Copy)]
struct SummaryQuery {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    active_within_seconds: i64,
    top_limit: u64,
}

#[derive(Clone, Copy)]
struct PageQuery {
    start_index: u64,
    limit: u64,
}

fn parse_summary_query(raw_query: Option<&str>) -> Option<SummaryQuery> {
    let mut query = clean_query(raw_query)?;
    let from = parse_timestamp(&query.remove("from")?)?;
    let to = parse_timestamp(&query.remove("to")?)?;
    let active_within_seconds = query
        .remove("activeWithinSeconds")
        .map_or(Some(DEFAULT_ACTIVE_SECONDS), |value| value.parse().ok())?;
    let top_limit = query
        .remove("topLimit")
        .map_or(Some(DEFAULT_TOP_LIMIT), |value| value.parse().ok())?;
    let duration = to.signed_duration_since(from);
    (query.is_empty()
        && duration > Duration::zero()
        && duration <= Duration::days(MAX_RANGE_DAYS)
        && (15..=300).contains(&active_within_seconds)
        && (1..=20).contains(&top_limit))
    .then_some(SummaryQuery {
        from,
        to,
        active_within_seconds,
        top_limit,
    })
}

fn parse_active_query(raw_query: Option<&str>) -> Option<i64> {
    let mut query = clean_query(raw_query)?;
    let seconds = query
        .remove("activeWithinSeconds")
        .map_or(Some(DEFAULT_ACTIVE_SECONDS), |value| value.parse().ok())?;
    (query.is_empty() && (15..=300).contains(&seconds)).then_some(seconds)
}

fn parse_page_query(raw_query: Option<&str>) -> Option<PageQuery> {
    let mut query = clean_query(raw_query)?;
    let start_index = query
        .remove("startIndex")
        .map_or(Some(0), |value| value.parse().ok())?;
    let limit = query
        .remove("limit")
        .map_or(Some(DEFAULT_PAGE_LIMIT), |value| value.parse().ok())?;
    (query.is_empty() && limit > 0 && limit <= MAX_PAGE_LIMIT)
        .then_some(PageQuery { start_index, limit })
}

fn clean_query(raw_query: Option<&str>) -> Option<std::collections::HashMap<String, String>> {
    let mut query = auth::request_query(raw_query).ok()?;
    query.remove("ApiKey");
    query.remove("api_key");
    Some(query)
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct DashboardSummaryDto {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    users_total: u64,
    users_disabled: u64,
    catalog_total: u64,
    movies: u64,
    series: u64,
    episodes: u64,
    play_count: u64,
    unique_viewers: u64,
    currently_watching: u64,
    trend: Vec<TrendPointDto>,
    top_items: Vec<TopItemDto>,
}

impl DashboardSummaryDto {
    fn from_snapshot(query: SummaryQuery, snapshot: DashboardSnapshot) -> Self {
        Self {
            from: query.from,
            to: query.to,
            users_total: snapshot.users_total,
            users_disabled: snapshot.users_disabled,
            catalog_total: snapshot.catalog_total,
            movies: snapshot.movies,
            series: snapshot.series,
            episodes: snapshot.episodes,
            play_count: snapshot.play_count,
            unique_viewers: snapshot.unique_viewers,
            currently_watching: snapshot.currently_watching,
            trend: trend_points(query.from, query.to, &snapshot.events),
            top_items: snapshot
                .top_items
                .into_iter()
                .map(TopItemDto::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct TrendPointDto {
    bucket_start: DateTime<Utc>,
    play_count: u64,
    unique_viewers: u64,
}

fn trend_points(
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    events: &[DashboardPlaybackEvent],
) -> Vec<TrendPointDto> {
    let duration = to.signed_duration_since(from);
    let bucket = if duration <= Duration::days(2) {
        Duration::hours(1)
    } else {
        Duration::days(1)
    };
    let bucket_seconds = bucket.num_seconds();
    let bucket_count =
        usize::try_from((duration.num_seconds() + bucket_seconds - 1) / bucket_seconds)
            .unwrap_or(0);
    let mut plays = vec![0_u64; bucket_count];
    let mut viewers = vec![HashSet::<Uuid>::new(); bucket_count];
    for event in events {
        let seconds = event.started_at.signed_duration_since(from).num_seconds();
        if seconds < 0 {
            continue;
        }
        let Ok(index) = usize::try_from(seconds / bucket_seconds) else {
            continue;
        };
        if let (Some(play_count), Some(unique_viewers)) =
            (plays.get_mut(index), viewers.get_mut(index))
        {
            *play_count += 1;
            unique_viewers.insert(event.user_id);
        }
    }
    plays
        .into_iter()
        .zip(viewers)
        .enumerate()
        .map(|(index, (play_count, viewers))| TrendPointDto {
            bucket_start: from + bucket * i32::try_from(index).unwrap_or(i32::MAX),
            play_count,
            unique_viewers: u64::try_from(viewers.len()).unwrap_or(u64::MAX),
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct TopItemDto {
    item_id: Uuid,
    name: String,
    item_type: String,
    production_year: Option<i32>,
    play_count: u64,
    unique_viewers: u64,
}

impl From<DashboardTopItem> for TopItemDto {
    fn from(item: DashboardTopItem) -> Self {
        Self {
            item_id: item.item_id,
            name: item.name,
            item_type: item.item_type,
            production_year: item.production_year,
            play_count: item.play_count,
            unique_viewers: item.unique_viewers,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct NowPlayingDto {
    session_id: Uuid,
    user_id: Uuid,
    user_name: String,
    item_id: Uuid,
    item_name: String,
    item_type: String,
    runtime_ticks: Option<i64>,
    position_ticks: i64,
    client_name: String,
    device_name: String,
    started_at: DateTime<Utc>,
    last_event_at: DateTime<Utc>,
}

impl From<DashboardNowPlaying> for NowPlayingDto {
    fn from(item: DashboardNowPlaying) -> Self {
        Self {
            session_id: item.session_id,
            user_id: item.user_id,
            user_name: item.user_name,
            item_id: item.item_id,
            item_name: item.item_name,
            item_type: item.item_type,
            runtime_ticks: item.runtime_ticks,
            position_ticks: item.position_ticks,
            client_name: item.client_name,
            device_name: item.device_name,
            started_at: item.started_at,
            last_event_at: item.last_event_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LoginHistoryPageDto {
    items: Vec<LoginHistoryDto>,
    total_record_count: u64,
    start_index: u64,
}

impl From<DashboardPage<DashboardLoginRecord>> for LoginHistoryPageDto {
    fn from(page: DashboardPage<DashboardLoginRecord>) -> Self {
        Self {
            items: page.items.into_iter().map(LoginHistoryDto::from).collect(),
            total_record_count: page.total_record_count,
            start_index: page.start_index,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LoginHistoryDto {
    session_id: Uuid,
    user_id: Uuid,
    user_name: String,
    client_name: String,
    client_version: String,
    device_name: String,
    created_at: DateTime<Utc>,
    last_seen_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

impl From<DashboardLoginRecord> for LoginHistoryDto {
    fn from(item: DashboardLoginRecord) -> Self {
        Self {
            session_id: item.session_id,
            user_id: item.user_id,
            user_name: item.user_name,
            client_name: item.client_name,
            client_version: item.client_version,
            device_name: item.device_name,
            created_at: item.created_at,
            last_seen_at: item.last_seen_at,
            expires_at: item.expires_at,
            revoked_at: item.revoked_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct WatchHistoryPageDto {
    items: Vec<WatchHistoryDto>,
    total_record_count: u64,
    start_index: u64,
}

impl From<DashboardPage<DashboardWatchRecord>> for WatchHistoryPageDto {
    fn from(page: DashboardPage<DashboardWatchRecord>) -> Self {
        Self {
            items: page.items.into_iter().map(WatchHistoryDto::from).collect(),
            total_record_count: page.total_record_count,
            start_index: page.start_index,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct WatchHistoryDto {
    session_id: Uuid,
    user_id: Uuid,
    user_name: String,
    item_id: Uuid,
    item_name: String,
    item_type: String,
    runtime_ticks: Option<i64>,
    position_ticks: i64,
    started_at: DateTime<Utc>,
    last_event_at: DateTime<Utc>,
    stopped_at: Option<DateTime<Utc>>,
}

impl From<DashboardWatchRecord> for WatchHistoryDto {
    fn from(item: DashboardWatchRecord) -> Self {
        Self {
            session_id: item.session_id,
            user_id: item.user_id,
            user_name: item.user_name,
            item_id: item.item_id,
            item_name: item.item_name,
            item_type: item.item_type,
            runtime_ticks: item.runtime_ticks,
            position_ticks: item.position_ticks,
            started_at: item.started_at,
            last_event_at: item.last_event_at,
            stopped_at: item.stopped_at,
        }
    }
}
