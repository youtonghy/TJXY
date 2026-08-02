use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::{
    MetadataError, MetadataImageReference, MetadataItemKind, MetadataProviderError, valid_text,
};

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const TICKS_PER_MINUTE: i64 = 600_000_000;
const ROOT_CAST_LIMIT: usize = 24;
const ROOT_CREW_LIMIT: usize = 12;
const EPISODE_CAST_LIMIT: usize = 12;
const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(50), Duration::from_millis(200)];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RichRemoteImageKind {
    Primary,
    Backdrop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RichRemoteImage {
    kind: RichRemoteImageKind,
    path: String,
}

impl RichRemoteImage {
    #[must_use]
    pub const fn kind(&self) -> RichRemoteImageKind {
        self.kind
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RichCountry {
    code: String,
    name: String,
}

impl RichCountry {
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RichLanguage {
    code: String,
    name: String,
}

impl RichLanguage {
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RichCredit {
    person_provider_id: u64,
    person_name: String,
    credit_type: String,
    role: Option<String>,
    order: u32,
    profile_path: Option<String>,
}

impl RichCredit {
    #[must_use]
    pub const fn person_provider_id(&self) -> u64 {
        self.person_provider_id
    }

    #[must_use]
    pub fn person_name(&self) -> &str {
        &self.person_name
    }

    #[must_use]
    pub fn credit_type(&self) -> &str {
        &self.credit_type
    }

    #[must_use]
    pub fn role(&self) -> Option<&str> {
        self.role.as_deref()
    }

    #[must_use]
    pub const fn order(&self) -> u32 {
        self.order
    }

    #[must_use]
    pub fn profile_path(&self) -> Option<&str> {
        self.profile_path.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RichCatalogItem {
    kind: MetadataItemKind,
    provider_id: u64,
    title: String,
    original_title: Option<String>,
    overview: Option<String>,
    tagline: Option<String>,
    production_year: Option<i32>,
    community_rating: Option<f64>,
    vote_count: Option<u64>,
    runtime_ticks: Option<i64>,
    premiere_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    release_status: Option<String>,
    official_rating: Option<String>,
    original_language: Option<String>,
    index_number: Option<u32>,
    genres: Vec<String>,
    studios: Vec<String>,
    countries: Vec<RichCountry>,
    languages: Vec<RichLanguage>,
    credits: Vec<RichCredit>,
    provider_ids: BTreeMap<String, String>,
    images: Vec<RichRemoteImage>,
    snapshot: Value,
}

impl RichCatalogItem {
    #[must_use]
    pub const fn kind(&self) -> MetadataItemKind {
        self.kind
    }

    #[must_use]
    pub const fn provider_id(&self) -> u64 {
        self.provider_id
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn original_title(&self) -> Option<&str> {
        self.original_title.as_deref()
    }

    #[must_use]
    pub fn overview(&self) -> Option<&str> {
        self.overview.as_deref()
    }

    #[must_use]
    pub fn tagline(&self) -> Option<&str> {
        self.tagline.as_deref()
    }

    #[must_use]
    pub const fn production_year(&self) -> Option<i32> {
        self.production_year
    }

    #[must_use]
    pub const fn community_rating(&self) -> Option<f64> {
        self.community_rating
    }

    #[must_use]
    pub const fn vote_count(&self) -> Option<u64> {
        self.vote_count
    }

    #[must_use]
    pub const fn runtime_ticks(&self) -> Option<i64> {
        self.runtime_ticks
    }

    #[must_use]
    pub const fn premiere_date(&self) -> Option<NaiveDate> {
        self.premiere_date
    }

    #[must_use]
    pub const fn end_date(&self) -> Option<NaiveDate> {
        self.end_date
    }

    #[must_use]
    pub fn release_status(&self) -> Option<&str> {
        self.release_status.as_deref()
    }

    #[must_use]
    pub fn official_rating(&self) -> Option<&str> {
        self.official_rating.as_deref()
    }

    #[must_use]
    pub fn original_language(&self) -> Option<&str> {
        self.original_language.as_deref()
    }

    #[must_use]
    pub const fn index_number(&self) -> Option<u32> {
        self.index_number
    }

    #[must_use]
    pub fn genres(&self) -> &[String] {
        &self.genres
    }

    #[must_use]
    pub fn studios(&self) -> &[String] {
        &self.studios
    }

    #[must_use]
    pub fn countries(&self) -> &[RichCountry] {
        &self.countries
    }

    #[must_use]
    pub fn languages(&self) -> &[RichLanguage] {
        &self.languages
    }

    #[must_use]
    pub fn credits(&self) -> &[RichCredit] {
        &self.credits
    }

    #[must_use]
    pub const fn provider_ids(&self) -> &BTreeMap<String, String> {
        &self.provider_ids
    }

    #[must_use]
    pub fn images(&self) -> &[RichRemoteImage] {
        &self.images
    }

    #[must_use]
    pub const fn snapshot(&self) -> &Value {
        &self.snapshot
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RichEpisode {
    item: RichCatalogItem,
}

impl RichEpisode {
    #[must_use]
    pub const fn item(&self) -> &RichCatalogItem {
        &self.item
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RichSeason {
    item: RichCatalogItem,
    episodes: Vec<RichEpisode>,
}

impl RichSeason {
    #[must_use]
    pub const fn item(&self) -> &RichCatalogItem {
        &self.item
    }

    #[must_use]
    pub fn episodes(&self) -> &[RichEpisode] {
        &self.episodes
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RichSeries {
    item: RichCatalogItem,
    seasons: Vec<RichSeason>,
}

impl RichSeries {
    #[must_use]
    pub const fn item(&self) -> &RichCatalogItem {
        &self.item
    }

    #[must_use]
    pub fn seasons(&self) -> &[RichSeason] {
        &self.seasons
    }

    /// Retains a bounded, ordered subset of the fetched Season and Episode structure.
    #[must_use]
    pub fn with_structure_limits(
        mut self,
        max_seasons: usize,
        max_episodes_per_season: usize,
    ) -> Self {
        self.seasons.truncate(max_seasons);
        for season in &mut self.seasons {
            season.episodes.truncate(max_episodes_per_season);
        }
        self
    }
}

#[async_trait]
pub trait TmdbCatalogTransport: Send + Sync {
    /// Fetches one bounded TMDB API resource.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for rejected, unavailable, or invalid responses.
    async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<Vec<u8>, MetadataProviderError>;
}

pub struct TmdbCatalogClient {
    transport: Arc<dyn TmdbCatalogTransport>,
    language: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TmdbPopularItem {
    id: u64,
    name: String,
    overview: Option<String>,
    year: Option<i32>,
    rating: Option<f64>,
    popularity: Option<f64>,
    poster_url: Option<String>,
}

impl TmdbPopularItem {
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub fn overview(&self) -> Option<&str> {
        self.overview.as_deref()
    }
    #[must_use]
    pub const fn year(&self) -> Option<i32> {
        self.year
    }
    #[must_use]
    pub const fn rating(&self) -> Option<f64> {
        self.rating
    }
    #[must_use]
    pub const fn popularity(&self) -> Option<f64> {
        self.popularity
    }
    #[must_use]
    pub fn poster_url(&self) -> Option<&str> {
        self.poster_url.as_deref()
    }
}

impl TmdbCatalogClient {
    /// Creates the production catalog client.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidProvider`] for invalid token or language input.
    pub fn new(
        access_token: impl Into<String>,
        language: impl Into<String>,
    ) -> Result<Self, MetadataError> {
        let access_token = access_token.into();
        let language = language.into();
        if !valid_text(&access_token, 4096) || !valid_text(&language, 32) {
            return Err(MetadataError::InvalidProvider);
        }
        Ok(Self {
            transport: Arc::new(ReqwestTmdbCatalogTransport::new(access_token)?),
            language,
        })
    }

    /// Creates a deterministic client around an alternate transport.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidProvider`] for an invalid language.
    pub fn with_transport(
        language: impl Into<String>,
        transport: Arc<dyn TmdbCatalogTransport>,
    ) -> Result<Self, MetadataError> {
        let language = language.into();
        if !valid_text(&language, 32) {
            return Err(MetadataError::InvalidProvider);
        }
        Ok(Self {
            transport,
            language,
        })
    }

    /// Fetches and validates one rich Movie record.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid IDs, transport failures, or invalid data.
    pub async fn movie(&self, id: u64) -> Result<RichCatalogItem, MetadataProviderError> {
        if id == 0 {
            return Err(MetadataProviderError::Rejected);
        }
        let path = format!("/movie/{id}");
        let bytes = self.fetch(&path, &self.root_query("movie")).await?;
        let (mut wire, snapshot) = parse_response::<MovieWire>(&bytes)?;
        if self.language != "en-US" && needs_text_fallback(&wire.title, wire.overview.as_deref()) {
            let bytes = self
                .fetch(&path, &Self::root_query_for("movie", "en-US"))
                .await?;
            let (fallback, _) = parse_response::<MovieWire>(&bytes)?;
            merge_missing_text(
                &mut wire.title,
                &mut wire.overview,
                &mut wire.tagline,
                fallback.title,
                fallback.overview,
                fallback.tagline,
            );
        }
        movie_item(wire, snapshot, &self.language)
    }

    /// Fetches and validates one Series and every declared Season and Episode.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid IDs, transport failures, or invalid data.
    pub async fn series(&self, id: u64) -> Result<RichSeries, MetadataProviderError> {
        if id == 0 {
            return Err(MetadataProviderError::Rejected);
        }
        let path = format!("/tv/{id}");
        let bytes = self.fetch(&path, &self.root_query("tv")).await?;
        let (mut wire, snapshot) = parse_response::<SeriesWire>(&bytes)?;
        if self.language != "en-US" && needs_text_fallback(&wire.name, wire.overview.as_deref()) {
            let bytes = self
                .fetch(&path, &Self::root_query_for("tv", "en-US"))
                .await?;
            let (fallback, _) = parse_response::<SeriesWire>(&bytes)?;
            merge_missing_text(
                &mut wire.name,
                &mut wire.overview,
                &mut wire.tagline,
                fallback.name,
                fallback.overview,
                fallback.tagline,
            );
        }
        let season_numbers = wire
            .seasons
            .iter()
            .map(|season| season.season_number)
            .collect::<Vec<_>>();
        let item = series_item(wire, snapshot, &self.language)?;
        let mut seasons = Vec::with_capacity(season_numbers.len());
        for season_number in season_numbers {
            let path = format!("/tv/{id}/season/{season_number}");
            let bytes = self.fetch(&path, &self.season_query()).await?;
            let (wire, snapshot) = parse_response::<SeasonWire>(&bytes)?;
            seasons.push(season_item(wire, snapshot)?);
        }
        seasons.sort_by_key(|season| season.item.index_number);
        Ok(RichSeries { item, seasons })
    }

    /// Returns one validated page of popular Movie IDs for manifest generation.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid page numbers or response data.
    pub async fn popular_movie_ids(&self, page: u16) -> Result<Vec<u64>, MetadataProviderError> {
        self.popular_movies(page)
            .await
            .map(|items| items.into_iter().map(|item| item.id).collect())
    }

    /// Returns one validated page of popular Series IDs for manifest generation.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid page numbers or response data.
    pub async fn popular_series_ids(&self, page: u16) -> Result<Vec<u64>, MetadataProviderError> {
        self.popular_series(page)
            .await
            .map(|items| items.into_iter().map(|item| item.id).collect())
    }

    /// Returns one validated page of top-rated Series IDs for manifest generation.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid page numbers or response data.
    pub async fn top_rated_series_ids(&self, page: u16) -> Result<Vec<u64>, MetadataProviderError> {
        self.top_rated_series(page)
            .await
            .map(|items| items.into_iter().map(|item| item.id).collect())
    }

    /// Returns one validated page of top-rated Movie IDs for manifest generation.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid page numbers or response data.
    pub async fn top_rated_movie_ids(&self, page: u16) -> Result<Vec<u64>, MetadataProviderError> {
        self.top_rated_movies(page)
            .await
            .map(|items| items.into_iter().map(|item| item.id).collect())
    }

    /// Returns one validated page of now-playing Movie IDs for manifest generation.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid page numbers or response data.
    pub async fn now_playing_movie_ids(&self, page: u16) -> Result<Vec<u64>, MetadataProviderError> {
        self.now_playing_movies(page)
            .await
            .map(|items| items.into_iter().map(|item| item.id).collect())
    }

    /// Returns one validated page of upcoming Movie IDs for manifest generation.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid page numbers or response data.
    pub async fn upcoming_movie_ids(&self, page: u16) -> Result<Vec<u64>, MetadataProviderError> {
        self.upcoming_movies(page)
            .await
            .map(|items| items.into_iter().map(|item| item.id).collect())
    }

    /// Returns one lightweight page of ranked Movies without detail requests.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid pages, transport failures, or invalid data.
    pub async fn popular_movies(
        &self,
        page: u16,
    ) -> Result<Vec<TmdbPopularItem>, MetadataProviderError> {
        self.popular_items("/movie/popular", page, true).await
    }

    /// Returns one lightweight page of ranked Series without season requests.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid pages, transport failures, or invalid data.
    pub async fn popular_series(
        &self,
        page: u16,
    ) -> Result<Vec<TmdbPopularItem>, MetadataProviderError> {
        self.popular_items("/tv/popular", page, false).await
    }

    /// Returns one lightweight page of top-rated Series without season requests.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid pages, transport failures, or invalid data.
    pub async fn top_rated_series(
        &self,
        page: u16,
    ) -> Result<Vec<TmdbPopularItem>, MetadataProviderError> {
        self.popular_items("/tv/top_rated", page, false).await
    }

    /// Returns one lightweight page of top-rated Movies without detail requests.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid pages, transport failures, or invalid data.
    pub async fn top_rated_movies(
        &self,
        page: u16,
    ) -> Result<Vec<TmdbPopularItem>, MetadataProviderError> {
        self.popular_items("/movie/top_rated", page, true).await
    }

    /// Returns one lightweight page of now-playing Movies without detail requests.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid pages, transport failures, or invalid data.
    pub async fn now_playing_movies(
        &self,
        page: u16,
    ) -> Result<Vec<TmdbPopularItem>, MetadataProviderError> {
        self.popular_items("/movie/now_playing", page, true).await
    }

    /// Returns one lightweight page of upcoming Movies without detail requests.
    ///
    /// # Errors
    ///
    /// Returns a sanitized provider error for invalid pages, transport failures, or invalid data.
    pub async fn upcoming_movies(
        &self,
        page: u16,
    ) -> Result<Vec<TmdbPopularItem>, MetadataProviderError> {
        self.popular_items("/movie/upcoming", page, true).await
    }

    fn root_query(&self, kind: &str) -> Vec<(String, String)> {
        Self::root_query_for(kind, &self.language)
    }

    fn root_query_for(kind: &str, language: &str) -> Vec<(String, String)> {
        vec![
            ("language".to_owned(), language.to_owned()),
            (
                "append_to_response".to_owned(),
                if kind == "movie" {
                    "credits,release_dates,external_ids,images"
                } else {
                    "aggregate_credits,content_ratings,external_ids,images"
                }
                .to_owned(),
            ),
            (
                "include_image_language".to_owned(),
                included_image_languages(language),
            ),
        ]
    }

    fn season_query(&self) -> Vec<(String, String)> {
        vec![
            ("language".to_owned(), self.language.clone()),
            ("append_to_response".to_owned(), "credits,images".to_owned()),
            (
                "include_image_language".to_owned(),
                included_image_languages(&self.language),
            ),
        ]
    }

    async fn popular_items(
        &self,
        path: &str,
        page: u16,
        movie: bool,
    ) -> Result<Vec<TmdbPopularItem>, MetadataProviderError> {
        if !(1..=500).contains(&page) {
            return Err(MetadataProviderError::Rejected);
        }
        let query = vec![
            ("language".to_owned(), self.language.clone()),
            ("page".to_owned(), page.to_string()),
        ];
        let bytes = self.fetch(path, &query).await?;
        let (wire, _) = parse_response::<PopularPageWire>(&bytes)?;
        if wire.page != page || wire.total_pages < u32::from(page) || wire.results.len() > 20 {
            return Err(MetadataProviderError::InvalidResponse);
        }
        let mut seen = HashSet::with_capacity(wire.results.len());
        wire.results
            .into_iter()
            .filter(|result| seen.insert(result.id))
            .map(|result| {
                if result.id == 0 {
                    return Err(MetadataProviderError::InvalidResponse);
                }
                let name = if movie { result.title } else { result.name }
                    .filter(|value| valid_text(value, 512))
                    .ok_or(MetadataProviderError::InvalidResponse)?;
                let date = if movie {
                    result.release_date
                } else {
                    result.first_air_date
                };
                let year = date
                    .as_deref()
                    .and_then(|value| value.get(..4))
                    .and_then(|value| value.parse::<i32>().ok());
                let poster_url = result
                    .poster_path
                    .as_deref()
                    .and_then(MetadataImageReference::tmdb)
                    .map(|image| image.url().to_owned());
                Ok(TmdbPopularItem {
                    id: result.id,
                    name,
                    overview: result.overview.filter(|value| valid_text(value, 32 * 1024)),
                    year,
                    rating: result
                        .vote_average
                        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 10.0),
                    popularity: result
                        .popularity
                        .filter(|value| value.is_finite() && *value >= 0.0),
                    poster_url,
                })
            })
            .collect()
    }

    async fn fetch(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<Vec<u8>, MetadataProviderError> {
        for delay in RETRY_DELAYS {
            match self.transport.get(path, query).await {
                Err(MetadataProviderError::TemporarilyUnavailable) => {
                    tokio::time::sleep(delay).await;
                }
                result => return result,
            }
        }
        self.transport.get(path, query).await
    }
}

struct ReqwestTmdbCatalogTransport {
    client: reqwest::Client,
    access_token: Zeroizing<String>,
}

impl ReqwestTmdbCatalogTransport {
    fn new(access_token: String) -> Result<Self, MetadataError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|_| MetadataError::InvalidProvider)?;
        Ok(Self {
            client,
            access_token: Zeroizing::new(access_token),
        })
    }
}

#[async_trait]
impl TmdbCatalogTransport for ReqwestTmdbCatalogTransport {
    async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<Vec<u8>, MetadataProviderError> {
        let response = self
            .client
            .get(format!("https://api.themoviedb.org/3{path}"))
            .bearer_auth(self.access_token.as_str())
            .query(query)
            .send()
            .await
            .map_err(|_| MetadataProviderError::TemporarilyUnavailable)?;
        if !response.status().is_success() {
            return Err(
                if response.status().is_server_error()
                    || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || response.status() == reqwest::StatusCode::REQUEST_TIMEOUT
                {
                    MetadataProviderError::TemporarilyUnavailable
                } else {
                    MetadataProviderError::Rejected
                },
            );
        }
        read_response(response).await
    }
}

async fn read_response(mut response: reqwest::Response) -> Result<Vec<u8>, MetadataProviderError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(MetadataProviderError::InvalidResponse);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| MetadataProviderError::TemporarilyUnavailable)?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(MetadataProviderError::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn parse_response<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
) -> Result<(T, Value), MetadataProviderError> {
    if bytes.is_empty() || bytes.len() > MAX_RESPONSE_BYTES {
        return Err(MetadataProviderError::InvalidResponse);
    }
    let snapshot: Value =
        serde_json::from_slice(bytes).map_err(|_| MetadataProviderError::InvalidResponse)?;
    let wire = serde_json::from_value(snapshot.clone())
        .map_err(|_| MetadataProviderError::InvalidResponse)?;
    Ok((wire, snapshot))
}

fn movie_item(
    wire: MovieWire,
    snapshot: Value,
    language: &str,
) -> Result<RichCatalogItem, MetadataProviderError> {
    let credits = root_credits(wire.credits.cast, wire.credits.crew)?;
    common_item(CommonItemInput {
        kind: MetadataItemKind::Movie,
        provider_id: wire.id,
        title: wire.title,
        original_title: wire.original_title,
        overview: wire.overview,
        tagline: wire.tagline,
        premiere_date: wire.release_date,
        end_date: None,
        runtime_minutes: wire.runtime,
        status: wire.status,
        rating: wire.vote_average,
        vote_count: wire.vote_count,
        official_rating: movie_rating(&wire.release_dates, language),
        original_language: wire.original_language,
        index_number: None,
        genres: wire.genres,
        studios: wire.production_companies,
        countries: wire.production_countries,
        languages: wire.spoken_languages,
        credits,
        external_ids: wire.external_ids,
        poster_path: wire.poster_path,
        backdrop_path: wire.backdrop_path,
        snapshot,
    })
}

fn series_item(
    wire: SeriesWire,
    snapshot: Value,
    language: &str,
) -> Result<RichCatalogItem, MetadataProviderError> {
    let credits = aggregate_credits(wire.aggregate_credits)?;
    common_item(CommonItemInput {
        kind: MetadataItemKind::Series,
        provider_id: wire.id,
        title: wire.name,
        original_title: wire.original_name,
        overview: wire.overview,
        tagline: wire.tagline,
        premiere_date: wire.first_air_date,
        end_date: wire.last_air_date,
        runtime_minutes: wire.episode_run_time.into_iter().next(),
        status: wire.status,
        rating: wire.vote_average,
        vote_count: wire.vote_count,
        official_rating: tv_rating(&wire.content_ratings, language),
        original_language: wire.original_language,
        index_number: None,
        genres: wire.genres,
        studios: wire.production_companies,
        countries: wire.production_countries,
        languages: wire.spoken_languages,
        credits,
        external_ids: wire.external_ids,
        poster_path: wire.poster_path,
        backdrop_path: wire.backdrop_path,
        snapshot,
    })
}

fn season_item(wire: SeasonWire, snapshot: Value) -> Result<RichSeason, MetadataProviderError> {
    let season_number = checked_index(wire.season_number)?;
    let mut episodes = wire
        .episodes
        .into_iter()
        .map(episode_item)
        .collect::<Result<Vec<_>, _>>()?;
    episodes.sort_by_key(|episode| episode.item.index_number);
    let item = common_item(CommonItemInput {
        kind: MetadataItemKind::Season,
        provider_id: wire.id,
        title: wire.name,
        original_title: None,
        overview: wire.overview,
        tagline: None,
        premiere_date: wire.air_date,
        end_date: None,
        runtime_minutes: None,
        status: None,
        rating: None,
        vote_count: None,
        official_rating: None,
        original_language: None,
        index_number: Some(season_number),
        genres: Vec::new(),
        studios: Vec::new(),
        countries: Vec::new(),
        languages: Vec::new(),
        credits: root_credits(wire.credits.cast, wire.credits.crew)?,
        external_ids: ExternalIdsWire::default(),
        poster_path: wire.poster_path,
        backdrop_path: None,
        snapshot,
    })?;
    Ok(RichSeason { item, episodes })
}

fn episode_item(wire: EpisodeWire) -> Result<RichEpisode, MetadataProviderError> {
    let snapshot =
        serde_json::to_value(&wire).map_err(|_| MetadataProviderError::InvalidResponse)?;
    let mut credits = cast_credits(wire.guest_stars, EPISODE_CAST_LIMIT)?;
    let crew_start =
        u32::try_from(credits.len()).map_err(|_| MetadataProviderError::InvalidResponse)?;
    credits.extend(crew_credits(wire.crew, ROOT_CREW_LIMIT, crew_start)?);
    let item = common_item(CommonItemInput {
        kind: MetadataItemKind::Episode,
        provider_id: wire.id,
        title: wire.name,
        original_title: None,
        overview: wire.overview,
        tagline: None,
        premiere_date: wire.air_date,
        end_date: None,
        runtime_minutes: wire.runtime,
        status: None,
        rating: wire.vote_average,
        vote_count: wire.vote_count,
        official_rating: None,
        original_language: None,
        index_number: Some(checked_index(wire.episode_number)?),
        genres: Vec::new(),
        studios: Vec::new(),
        countries: Vec::new(),
        languages: Vec::new(),
        credits,
        external_ids: ExternalIdsWire::default(),
        poster_path: wire.still_path,
        backdrop_path: None,
        snapshot,
    })?;
    Ok(RichEpisode { item })
}

struct CommonItemInput {
    kind: MetadataItemKind,
    provider_id: u64,
    title: String,
    original_title: Option<String>,
    overview: Option<String>,
    tagline: Option<String>,
    premiere_date: Option<String>,
    end_date: Option<String>,
    runtime_minutes: Option<i64>,
    status: Option<String>,
    rating: Option<f64>,
    vote_count: Option<u64>,
    official_rating: Option<String>,
    original_language: Option<String>,
    index_number: Option<u32>,
    genres: Vec<NamedWire>,
    studios: Vec<NamedWire>,
    countries: Vec<CountryWire>,
    languages: Vec<LanguageWire>,
    credits: Vec<RichCredit>,
    external_ids: ExternalIdsWire,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    snapshot: Value,
}

fn common_item(input: CommonItemInput) -> Result<RichCatalogItem, MetadataProviderError> {
    if input.provider_id == 0 || !valid_text(&input.title, 512) {
        return Err(MetadataProviderError::InvalidResponse);
    }
    let premiere_date = parse_date(input.premiere_date.as_deref())?;
    let end_date = parse_date(input.end_date.as_deref())?;
    let production_year = premiere_date.map(|date| chrono::Datelike::year(&date));
    let community_rating = input.rating.map(validate_rating).transpose()?.flatten();
    let runtime_ticks = input
        .runtime_minutes
        .map(|minutes| {
            if minutes < 0 {
                return Err(MetadataProviderError::InvalidResponse);
            }
            minutes
                .checked_mul(TICKS_PER_MINUTE)
                .ok_or(MetadataProviderError::InvalidResponse)
        })
        .transpose()?;
    let genres = named_values(input.genres)?;
    let studios = named_values(input.studios)?;
    let countries = countries(input.countries)?;
    let languages = languages(input.languages)?;
    let images = images(input.poster_path, input.backdrop_path)?;
    let mut provider_ids = BTreeMap::from([("tmdb".to_owned(), input.provider_id.to_string())]);
    if let Some(value) = nonempty(input.external_ids.imdb_id) {
        provider_ids.insert("imdb".to_owned(), value);
    }
    if let Some(value) = input.external_ids.tvdb_id {
        provider_ids.insert("tvdb".to_owned(), value.to_string());
    }
    if let Some(value) = nonempty(input.external_ids.wikidata_id) {
        provider_ids.insert("wikidata".to_owned(), value);
    }
    Ok(RichCatalogItem {
        kind: input.kind,
        provider_id: input.provider_id,
        title: input.title,
        original_title: validated_optional(input.original_title, 512)?,
        overview: validated_optional(input.overview, 32 * 1024)?,
        tagline: validated_optional(input.tagline, 2 * 1024)?,
        production_year,
        community_rating,
        vote_count: input.vote_count,
        runtime_ticks,
        premiere_date,
        end_date,
        release_status: validated_optional(input.status, 64)?,
        official_rating: validated_optional(input.official_rating, 32)?,
        original_language: validated_optional(input.original_language, 16)?,
        index_number: input.index_number,
        genres,
        studios,
        countries,
        languages,
        credits: input.credits,
        provider_ids,
        images,
        snapshot: input.snapshot,
    })
}

fn root_credits(
    cast: Vec<CastWire>,
    crew: Vec<CrewWire>,
) -> Result<Vec<RichCredit>, MetadataProviderError> {
    let mut credits = cast_credits(cast, ROOT_CAST_LIMIT)?;
    let crew_start =
        u32::try_from(credits.len()).map_err(|_| MetadataProviderError::InvalidResponse)?;
    credits.extend(crew_credits(crew, ROOT_CREW_LIMIT, crew_start)?);
    Ok(credits)
}

fn cast_credits(
    mut cast: Vec<CastWire>,
    limit: usize,
) -> Result<Vec<RichCredit>, MetadataProviderError> {
    cast.sort_by_key(|person| person.order);
    cast.into_iter()
        .take(limit)
        .enumerate()
        .map(|(index, person)| {
            credit(
                person.id,
                person.name,
                "Actor".to_owned(),
                person.character,
                u32::try_from(index).map_err(|_| MetadataProviderError::InvalidResponse)?,
                person.profile_path,
            )
        })
        .collect()
}

fn crew_credits(
    crew: Vec<CrewWire>,
    limit: usize,
    start: u32,
) -> Result<Vec<RichCredit>, MetadataProviderError> {
    crew.into_iter()
        .filter(|person| {
            matches!(
                person.job.as_str(),
                "Director"
                    | "Writer"
                    | "Screenplay"
                    | "Creator"
                    | "Executive Producer"
                    | "Original Music Composer"
            )
        })
        .take(limit)
        .enumerate()
        .map(|(index, person)| {
            credit(
                person.id,
                person.name,
                person.job.clone(),
                Some(person.job),
                start
                    .checked_add(
                        u32::try_from(index).map_err(|_| MetadataProviderError::InvalidResponse)?,
                    )
                    .ok_or(MetadataProviderError::InvalidResponse)?,
                person.profile_path,
            )
        })
        .collect()
}

fn aggregate_credits(wire: AggregateCreditsWire) -> Result<Vec<RichCredit>, MetadataProviderError> {
    let cast = wire
        .cast
        .into_iter()
        .map(|person| CastWire {
            id: person.id,
            name: person.name,
            character: person.roles.into_iter().next().map(|role| role.character),
            order: person.order,
            profile_path: person.profile_path,
        })
        .collect();
    let crew = wire
        .crew
        .into_iter()
        .filter_map(|person| {
            person.jobs.into_iter().next().map(|job| CrewWire {
                id: person.id,
                name: person.name,
                job: job.job,
                department: person.department,
                profile_path: person.profile_path,
            })
        })
        .collect();
    root_credits(cast, crew)
}

fn credit(
    id: u64,
    name: String,
    credit_type: String,
    role: Option<String>,
    order: u32,
    profile_path: Option<String>,
) -> Result<RichCredit, MetadataProviderError> {
    if id == 0 || !valid_text(&name, 512) || !valid_text(&credit_type, 32) {
        return Err(MetadataProviderError::InvalidResponse);
    }
    let profile_path = validated_path(profile_path)?;
    Ok(RichCredit {
        person_provider_id: id,
        person_name: name,
        credit_type,
        role: validated_optional(role, 512)?,
        order,
        profile_path,
    })
}

fn named_values(values: Vec<NamedWire>) -> Result<Vec<String>, MetadataProviderError> {
    if values.len() > 512 || values.iter().any(|value| !valid_text(&value.name, 512)) {
        return Err(MetadataProviderError::InvalidResponse);
    }
    Ok(values.into_iter().map(|value| value.name).collect())
}

fn countries(values: Vec<CountryWire>) -> Result<Vec<RichCountry>, MetadataProviderError> {
    if values.len() > 64 {
        return Err(MetadataProviderError::InvalidResponse);
    }
    values
        .into_iter()
        .map(|value| {
            if value.iso_3166_1.len() != 2 || !valid_text(&value.name, 512) {
                return Err(MetadataProviderError::InvalidResponse);
            }
            Ok(RichCountry {
                code: value.iso_3166_1,
                name: value.name,
            })
        })
        .collect()
}

fn languages(values: Vec<LanguageWire>) -> Result<Vec<RichLanguage>, MetadataProviderError> {
    if values.len() > 64 {
        return Err(MetadataProviderError::InvalidResponse);
    }
    values
        .into_iter()
        .map(|value| {
            let name = nonempty(value.english_name)
                .or_else(|| nonempty(value.name))
                .ok_or(MetadataProviderError::InvalidResponse)?;
            if !valid_text(&value.iso_639_1, 16) || !valid_text(&name, 512) {
                return Err(MetadataProviderError::InvalidResponse);
            }
            Ok(RichLanguage {
                code: value.iso_639_1,
                name,
            })
        })
        .collect()
}

fn images(
    poster_path: Option<String>,
    backdrop_path: Option<String>,
) -> Result<Vec<RichRemoteImage>, MetadataProviderError> {
    [
        (RichRemoteImageKind::Primary, poster_path),
        (RichRemoteImageKind::Backdrop, backdrop_path),
    ]
    .into_iter()
    .filter_map(|(kind, path)| path.map(|path| (kind, path)))
    .map(|(kind, path)| {
        if !valid_image_path(&path) {
            return Err(MetadataProviderError::InvalidResponse);
        }
        Ok(RichRemoteImage { kind, path })
    })
    .collect()
}

fn movie_rating(wire: &ReleaseDatesWire, language: &str) -> Option<String> {
    select_region(
        wire.results.iter().map(|entry| {
            (
                entry.iso_3166_1.as_str(),
                entry
                    .release_dates
                    .iter()
                    .find(|release| {
                        !release.certification.trim().is_empty()
                            && matches!(release.release_type, 2..=6)
                    })
                    .map(|release| release.certification.as_str()),
            )
        }),
        language,
    )
}

fn tv_rating(wire: &ContentRatingsWire, language: &str) -> Option<String> {
    select_region(
        wire.results
            .iter()
            .map(|entry| (entry.iso_3166_1.as_str(), Some(entry.rating.as_str()))),
        language,
    )
}

fn select_region<'a>(
    values: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    language: &str,
) -> Option<String> {
    let values = values
        .into_iter()
        .filter_map(|(region, value)| {
            nonempty(value.map(str::to_owned)).map(|value| (region, value))
        })
        .collect::<Vec<_>>();
    let preferred = language
        .split_once('-')
        .map_or("US", |(_, region)| region)
        .to_ascii_uppercase();
    values
        .iter()
        .find(|(region, _)| region.eq_ignore_ascii_case(&preferred))
        .or_else(|| {
            values
                .iter()
                .find(|(region, _)| region.eq_ignore_ascii_case("US"))
        })
        .or_else(|| values.first())
        .map(|(_, value)| value.clone())
}

fn validate_rating(value: f64) -> Result<Option<f64>, MetadataProviderError> {
    if !value.is_finite() || !(0.0..=10.0).contains(&value) {
        return Err(MetadataProviderError::InvalidResponse);
    }
    Ok(Some(value))
}

fn parse_date(value: Option<&str>) -> Result<Option<NaiveDate>, MetadataProviderError> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(Some)
        .map_err(|_| MetadataProviderError::InvalidResponse)
}

fn checked_index(value: i32) -> Result<u32, MetadataProviderError> {
    u32::try_from(value).map_err(|_| MetadataProviderError::InvalidResponse)
}

fn validated_optional(
    value: Option<String>,
    limit: usize,
) -> Result<Option<String>, MetadataProviderError> {
    let value = nonempty(value).map(|value| {
        value
            .chars()
            .map(|character| {
                if matches!(character, '\r' | '\n' | '\t') {
                    ' '
                } else {
                    character
                }
            })
            .collect::<String>()
    });
    if value
        .as_deref()
        .is_some_and(|value| !valid_text(value, limit))
    {
        return Err(MetadataProviderError::InvalidResponse);
    }
    Ok(value)
}

fn validated_path(value: Option<String>) -> Result<Option<String>, MetadataProviderError> {
    let value = nonempty(value);
    if value.as_deref().is_some_and(|path| !valid_image_path(path)) {
        return Err(MetadataProviderError::InvalidResponse);
    }
    Ok(value)
}

fn valid_image_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.contains("..")
        && !path.contains(['?', '#', '\\'])
        && valid_text(path, 512)
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn included_image_languages(language: &str) -> String {
    let primary = language.split('-').next().unwrap_or(language);
    let mut values = vec![primary];
    if primary != "en" {
        values.push("en");
    }
    values.push("null");
    values.join(",")
}

fn needs_text_fallback(title: &str, overview: Option<&str>) -> bool {
    title.trim().is_empty() || overview.is_none_or(|value| value.trim().is_empty())
}

fn merge_missing_text(
    title: &mut String,
    overview: &mut Option<String>,
    tagline: &mut Option<String>,
    fallback_title: String,
    fallback_overview: Option<String>,
    fallback_tagline: Option<String>,
) {
    if title.trim().is_empty() {
        *title = fallback_title;
    }
    if overview
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        *overview = nonempty(fallback_overview);
    }
    if tagline
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        *tagline = nonempty(fallback_tagline);
    }
}

#[derive(Deserialize)]
struct MovieWire {
    id: u64,
    title: String,
    original_title: Option<String>,
    overview: Option<String>,
    tagline: Option<String>,
    release_date: Option<String>,
    runtime: Option<i64>,
    status: Option<String>,
    vote_average: Option<f64>,
    vote_count: Option<u64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    original_language: Option<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    genres: Vec<NamedWire>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    production_companies: Vec<NamedWire>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    production_countries: Vec<CountryWire>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    spoken_languages: Vec<LanguageWire>,
    #[serde(default, deserialize_with = "deserialize_default_or_null")]
    credits: CreditsWire,
    #[serde(default, deserialize_with = "deserialize_default_or_null")]
    release_dates: ReleaseDatesWire,
    #[serde(default, deserialize_with = "deserialize_default_or_null")]
    external_ids: ExternalIdsWire,
}

#[derive(Deserialize)]
struct PopularPageWire {
    page: u16,
    total_pages: u32,
    results: Vec<PopularResultWire>,
}

#[derive(Deserialize)]
struct PopularResultWire {
    id: u64,
    title: Option<String>,
    name: Option<String>,
    overview: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    poster_path: Option<String>,
    vote_average: Option<f64>,
    popularity: Option<f64>,
}

#[derive(Deserialize)]
struct SeriesWire {
    id: u64,
    name: String,
    original_name: Option<String>,
    overview: Option<String>,
    tagline: Option<String>,
    first_air_date: Option<String>,
    last_air_date: Option<String>,
    status: Option<String>,
    vote_average: Option<f64>,
    vote_count: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    episode_run_time: Vec<i64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    original_language: Option<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    genres: Vec<NamedWire>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    production_companies: Vec<NamedWire>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    production_countries: Vec<CountryWire>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    spoken_languages: Vec<LanguageWire>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    seasons: Vec<SeasonSummaryWire>,
    #[serde(default, deserialize_with = "deserialize_default_or_null")]
    aggregate_credits: AggregateCreditsWire,
    #[serde(default, deserialize_with = "deserialize_default_or_null")]
    content_ratings: ContentRatingsWire,
    #[serde(default, deserialize_with = "deserialize_default_or_null")]
    external_ids: ExternalIdsWire,
}

#[derive(Deserialize)]
struct SeasonSummaryWire {
    season_number: i32,
}

#[derive(Deserialize)]
struct SeasonWire {
    id: u64,
    name: String,
    overview: Option<String>,
    air_date: Option<String>,
    season_number: i32,
    poster_path: Option<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    episodes: Vec<EpisodeWire>,
    #[serde(default, deserialize_with = "deserialize_default_or_null")]
    credits: CreditsWire,
}

#[derive(Deserialize, serde::Serialize)]
struct EpisodeWire {
    id: u64,
    name: String,
    overview: Option<String>,
    air_date: Option<String>,
    episode_number: i32,
    #[serde(default)]
    season_number: i32,
    runtime: Option<i64>,
    vote_average: Option<f64>,
    vote_count: Option<u64>,
    still_path: Option<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    guest_stars: Vec<CastWire>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    crew: Vec<CrewWire>,
}

#[derive(Deserialize)]
struct NamedWire {
    #[allow(dead_code)]
    id: Option<u64>,
    name: String,
}

#[derive(Deserialize)]
struct CountryWire {
    iso_3166_1: String,
    name: String,
}

#[derive(Deserialize)]
struct LanguageWire {
    english_name: Option<String>,
    iso_639_1: String,
    name: Option<String>,
}

#[derive(Default, Deserialize)]
struct CreditsWire {
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    cast: Vec<CastWire>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    crew: Vec<CrewWire>,
}

#[derive(Clone, Deserialize, serde::Serialize)]
struct CastWire {
    id: u64,
    name: String,
    character: Option<String>,
    #[serde(default)]
    order: u32,
    profile_path: Option<String>,
}

#[derive(Clone, Deserialize, serde::Serialize)]
struct CrewWire {
    id: u64,
    name: String,
    job: String,
    #[serde(default)]
    department: String,
    profile_path: Option<String>,
}

#[derive(Default, Deserialize)]
struct AggregateCreditsWire {
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    cast: Vec<AggregateCastWire>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    crew: Vec<AggregateCrewWire>,
}

#[derive(Deserialize)]
struct AggregateCastWire {
    id: u64,
    name: String,
    #[serde(default)]
    order: u32,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    roles: Vec<AggregateRoleWire>,
    profile_path: Option<String>,
}

#[derive(Deserialize)]
struct AggregateRoleWire {
    character: String,
}

#[derive(Deserialize)]
struct AggregateCrewWire {
    id: u64,
    name: String,
    #[serde(default)]
    department: String,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    jobs: Vec<AggregateJobWire>,
    profile_path: Option<String>,
}

#[derive(Deserialize)]
struct AggregateJobWire {
    job: String,
}

#[derive(Default, Deserialize)]
struct ReleaseDatesWire {
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    results: Vec<ReleaseRegionWire>,
}

#[derive(Deserialize)]
struct ReleaseRegionWire {
    iso_3166_1: String,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    release_dates: Vec<ReleaseWire>,
}

#[derive(Deserialize)]
struct ReleaseWire {
    certification: String,
    #[serde(rename = "type")]
    release_type: u8,
}

#[derive(Default, Deserialize)]
struct ContentRatingsWire {
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    results: Vec<ContentRatingWire>,
}

#[derive(Deserialize)]
struct ContentRatingWire {
    iso_3166_1: String,
    rating: String,
}

#[allow(clippy::struct_field_names)] // Mirrors TMDB's external_ids object.
#[derive(Default, Deserialize)]
struct ExternalIdsWire {
    imdb_id: Option<String>,
    tvdb_id: Option<u64>,
    wikidata_id: Option<String>,
}

fn deserialize_vec_or_default<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<Vec<T>>::deserialize(deserializer).map(Option::unwrap_or_default)
}

fn deserialize_default_or_null<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}
