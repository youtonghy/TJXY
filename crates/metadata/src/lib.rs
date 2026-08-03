//! Metadata provider contracts and bounded offline metadata parsers.

mod music;
mod tmdb_catalog;

pub use music::{MusicBrainzProvider, TheAudioDbProvider};

pub use tmdb_catalog::{
    RichCatalogItem, RichCountry, RichCredit, RichEpisode, RichLanguage, RichRemoteImage,
    RichRemoteImageKind, RichSeason, RichSeries, TmdbCatalogClient, TmdbCatalogTransport,
    TmdbPopularItem,
};

use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use async_trait::async_trait;
use quick_xml::{Reader, events::Event};
use thiserror::Error;

const MAX_TMDB_RESPONSE_BYTES: usize = 1024 * 1024;

const MAX_DEPTH: usize = 64;
const MAX_TEXT_CHARS: usize = 32 * 1024;
const MAX_ASSOCIATIONS: usize = 512;
const MAX_REFERENCE_CHARS: usize = 2048;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataItemKind {
    Audio,
    Movie,
    Series,
    Season,
    Episode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataState {
    Partial,
    Ready,
}

impl MetadataState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Partial => "Partial",
            Self::Ready => "Ready",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataSource {
    provider: String,
    reference: Option<String>,
    confidence: u16,
}

impl MetadataSource {
    /// Creates a bounded field source with confidence expressed in basis points.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidSource`] for invalid names, references, or confidence.
    pub fn new(
        provider: impl Into<String>,
        reference: Option<impl Into<String>>,
        confidence: u16,
    ) -> Result<Self, MetadataError> {
        let provider = provider.into();
        let reference = reference.map(Into::into);
        if !valid_text(&provider, 128)
            || reference
                .as_deref()
                .is_some_and(|value| !valid_text(value, MAX_REFERENCE_CHARS))
            || confidence > 10_000
        {
            return Err(MetadataError::InvalidSource);
        }
        Ok(Self {
            provider,
            reference,
            confidence,
        })
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn reference(&self) -> Option<&str> {
        self.reference.as_deref()
    }

    #[must_use]
    pub const fn confidence(&self) -> u16 {
        self.confidence
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataPerson {
    name: String,
    role: Option<String>,
    order: Option<u32>,
}

impl MetadataPerson {
    /// Defines one bounded credited person.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidAssociation`] for invalid text or order values.
    pub fn new(
        name: impl Into<String>,
        role: Option<impl Into<String>>,
        order: Option<u32>,
    ) -> Result<Self, MetadataError> {
        let name = name.into();
        let role = role.map(Into::into);
        if !valid_text(&name, 512)
            || role.as_deref().is_some_and(|value| !valid_text(value, 512))
            || order.is_some_and(|value| i32::try_from(value).is_err())
        {
            return Err(MetadataError::InvalidAssociation);
        }
        Ok(Self { name, role, order })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn role(&self) -> Option<&str> {
        self.role.as_deref()
    }

    #[must_use]
    pub const fn order(&self) -> Option<u32> {
        self.order
    }
}

#[derive(Clone, Debug)]
pub struct NfoDocument {
    kind: MetadataItemKind,
    title: Option<String>,
    original_title: Option<String>,
    production_year: Option<i32>,
    overview: Option<String>,
    provider_ids: BTreeMap<String, String>,
    genres: Vec<String>,
    studios: Vec<String>,
    people: Vec<MetadataPerson>,
    source: MetadataSource,
}

impl NfoDocument {
    pub const MAX_BYTES: usize = 2 * 1024 * 1024;

    /// Parses one bounded NFO document without resolving external or custom entities.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError`] for malformed, unsafe, oversized, or unsupported NFO input.
    pub fn parse(bytes: &[u8], source_reference: &str) -> Result<Self, MetadataError> {
        if bytes.is_empty() || bytes.len() > Self::MAX_BYTES {
            return Err(MetadataError::InputTooLarge);
        }
        let source = MetadataSource::new("Nfo", Some(source_reference), 9_000)?;
        let parser = NfoParser::new(source);
        parser.parse(bytes)
    }

    #[must_use]
    pub const fn kind(&self) -> MetadataItemKind {
        self.kind
    }

    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    #[must_use]
    pub fn original_title(&self) -> Option<&str> {
        self.original_title.as_deref()
    }

    #[must_use]
    pub const fn production_year(&self) -> Option<i32> {
        self.production_year
    }

    #[must_use]
    pub fn overview(&self) -> Option<&str> {
        self.overview.as_deref()
    }

    #[must_use]
    pub fn provider_id(&self, provider: &str) -> Option<&str> {
        self.provider_ids.get(provider).map(String::as_str)
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
    pub fn people(&self) -> &[MetadataPerson] {
        &self.people
    }

    #[must_use]
    pub fn source(&self) -> &MetadataSource {
        &self.source
    }

    #[must_use]
    pub fn state(&self) -> MetadataState {
        completeness(
            self.title.as_deref(),
            self.production_year,
            self.overview.as_deref(),
            &self.provider_ids,
        )
    }

    #[must_use]
    pub fn into_candidate(self) -> MetadataCandidate {
        MetadataCandidate {
            title: self.title,
            original_title: self.original_title,
            production_year: self.production_year,
            overview: self.overview,
            provider_ids: self.provider_ids,
            primary_image: None,
            genres: Some(self.genres),
            studios: Some(self.studios),
            people: Some(self.people),
            source: self.source,
        }
    }
}

struct NfoParser {
    stack: Vec<String>,
    text: String,
    unique_id_provider: Option<String>,
    actor: ActorBuilder,
    document: Option<NfoDocument>,
    source: MetadataSource,
}

#[derive(Default)]
struct ActorBuilder {
    name: Option<String>,
    role: Option<String>,
    order: Option<u32>,
}

impl NfoParser {
    fn new(source: MetadataSource) -> Self {
        Self {
            stack: Vec::new(),
            text: String::new(),
            unique_id_provider: None,
            actor: ActorBuilder::default(),
            document: None,
            source,
        }
    }

    fn parse(mut self, bytes: &[u8]) -> Result<NfoDocument, MetadataError> {
        let mut reader = Reader::from_reader(bytes);
        reader.config_mut().trim_text(false);
        loop {
            match reader.read_event() {
                Ok(Event::Start(start)) => {
                    let name = tag_name(start.name().as_ref())?;
                    self.stack.push(name);
                    if self.stack.len() > MAX_DEPTH {
                        return Err(MetadataError::NestingTooDeep);
                    }
                    self.text.clear();
                    if self.stack.len() == 1 {
                        self.start_document()?;
                    } else if self.stack.len() == 2 && self.current_tag() == Some("uniqueid") {
                        self.unique_id_provider = attribute(&start, b"type")?;
                    } else if self.stack.len() == 2 && self.current_tag() == Some("actor") {
                        self.actor = ActorBuilder::default();
                    }
                }
                Ok(Event::Empty(empty)) => {
                    let name = tag_name(empty.name().as_ref())?;
                    self.stack.push(name);
                    if self.stack.len() > MAX_DEPTH {
                        return Err(MetadataError::NestingTooDeep);
                    }
                    self.text.clear();
                    self.finish_element()?;
                    self.stack.pop();
                }
                Ok(Event::Text(text)) => {
                    self.append_text(text.decode().map_err(xml_error)?.as_ref())?;
                }
                Ok(Event::CData(text)) => {
                    self.append_text(text.decode().map_err(xml_error)?.as_ref())?;
                }
                Ok(Event::GeneralRef(reference)) => {
                    let reference = std::str::from_utf8(reference.as_ref())
                        .map_err(|_| MetadataError::UnsafeXml)?;
                    self.append_text(&safe_entity(reference)?)?;
                }
                Ok(Event::End(_)) => {
                    self.finish_element()?;
                    self.stack.pop().ok_or(MetadataError::MalformedXml)?;
                    self.text.clear();
                }
                Ok(Event::DocType(_)) => return Err(MetadataError::UnsafeXml),
                Ok(Event::Eof) => break,
                Ok(Event::Decl(_) | Event::Comment(_) | Event::PI(_)) => {}
                Err(_) => return Err(MetadataError::MalformedXml),
            }
        }
        if !self.stack.is_empty() {
            return Err(MetadataError::MalformedXml);
        }
        let document = self.document.ok_or(MetadataError::UnsupportedDocument)?;
        if document.title.is_none() {
            return Err(MetadataError::MissingTitle);
        }
        Ok(document)
    }

    fn start_document(&mut self) -> Result<(), MetadataError> {
        let kind = match self.current_tag() {
            Some("movie") => MetadataItemKind::Movie,
            Some("tvshow") => MetadataItemKind::Series,
            Some("season") => MetadataItemKind::Season,
            Some("episodedetails") => MetadataItemKind::Episode,
            _ => return Err(MetadataError::UnsupportedDocument),
        };
        self.document = Some(NfoDocument {
            kind,
            title: None,
            original_title: None,
            production_year: None,
            overview: None,
            provider_ids: BTreeMap::new(),
            genres: Vec::new(),
            studios: Vec::new(),
            people: Vec::new(),
            source: self.source.clone(),
        });
        Ok(())
    }

    fn append_text(&mut self, value: &str) -> Result<(), MetadataError> {
        if self
            .text
            .chars()
            .count()
            .saturating_add(value.chars().count())
            > MAX_TEXT_CHARS
        {
            return Err(MetadataError::FieldTooLarge);
        }
        self.text.push_str(value);
        Ok(())
    }

    fn finish_element(&mut self) -> Result<(), MetadataError> {
        let value = self.text.trim();
        let path = self.stack.iter().map(String::as_str).collect::<Vec<_>>();
        let Some(document) = self.document.as_mut() else {
            return Ok(());
        };
        match path.as_slice() {
            [_, "title"] => set_once(&mut document.title, value, 512)?,
            [_, "originaltitle"] => set_once(&mut document.original_title, value, 512)?,
            [_, "year"] if !value.is_empty() => {
                let year = value
                    .parse::<i32>()
                    .map_err(|_| MetadataError::InvalidYear)?;
                if !(1..=9999).contains(&year) {
                    return Err(MetadataError::InvalidYear);
                }
                document.production_year = Some(year);
            }
            [_, "plot"] => set_once(&mut document.overview, value, MAX_TEXT_CHARS)?,
            [_, "uniqueid"] => {
                if let Some(provider) = self.unique_id_provider.take() {
                    insert_provider_id(&mut document.provider_ids, &provider, value)?;
                }
            }
            [_, "tmdbid"] => insert_provider_id(&mut document.provider_ids, "tmdb", value)?,
            [_, "imdbid"] => insert_provider_id(&mut document.provider_ids, "imdb", value)?,
            [_, "tvdbid"] => insert_provider_id(&mut document.provider_ids, "tvdb", value)?,
            [_, "genre"] => push_unique(&mut document.genres, value)?,
            [_, "studio"] => push_unique(&mut document.studios, value)?,
            [_, "actor", "name"] => set_once(&mut self.actor.name, value, 512)?,
            [_, "actor", "role"] => set_once(&mut self.actor.role, value, 512)?,
            [_, "actor", "order"] if !value.is_empty() => {
                self.actor.order = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| MetadataError::InvalidAssociation)?,
                );
            }
            [_, "actor"] => {
                if let Some(name) = self.actor.name.take() {
                    if document.people.len() >= MAX_ASSOCIATIONS {
                        return Err(MetadataError::TooManyAssociations);
                    }
                    document.people.push(MetadataPerson {
                        name,
                        role: self.actor.role.take(),
                        order: self.actor.order.take(),
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn current_tag(&self) -> Option<&str> {
        self.stack.last().map(String::as_str)
    }
}

#[derive(Clone, Debug)]
pub struct MetadataLookup {
    kind: MetadataItemKind,
    fallback_title: String,
    fallback_year: Option<i32>,
}

impl MetadataLookup {
    /// Defines the stable naming evidence used when richer providers are unavailable.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidLookup`] for invalid fallback data.
    pub fn new(
        kind: MetadataItemKind,
        fallback_title: impl Into<String>,
        fallback_year: Option<i32>,
    ) -> Result<Self, MetadataError> {
        let fallback_title = fallback_title.into();
        if !valid_text(&fallback_title, 512)
            || fallback_year.is_some_and(|year| !(1..=9999).contains(&year))
        {
            return Err(MetadataError::InvalidLookup);
        }
        Ok(Self {
            kind,
            fallback_title,
            fallback_year,
        })
    }

    #[must_use]
    pub const fn kind(&self) -> MetadataItemKind {
        self.kind
    }

    #[must_use]
    pub fn fallback_title(&self) -> &str {
        &self.fallback_title
    }

    #[must_use]
    pub const fn fallback_year(&self) -> Option<i32> {
        self.fallback_year
    }
}

#[derive(Clone, Debug)]
pub struct MetadataCandidate {
    title: Option<String>,
    original_title: Option<String>,
    production_year: Option<i32>,
    overview: Option<String>,
    provider_ids: BTreeMap<String, String>,
    primary_image: Option<MetadataImageReference>,
    genres: Option<Vec<String>>,
    studios: Option<Vec<String>>,
    people: Option<Vec<MetadataPerson>>,
    source: MetadataSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataImageReference {
    provider: String,
    reference: String,
    url: String,
}

impl MetadataImageReference {
    /// Builds the pinned HTTPS TMDB image reference for a validated provider path.
    #[must_use]
    pub fn tmdb(path: &str) -> Option<Self> {
        let path = path.strip_prefix('/')?;
        if path.is_empty()
            || path.len() > 512
            || path.chars().any(|character| {
                !(character.is_ascii_alphanumeric() || matches!(character, '/' | '.' | '_' | '-'))
            })
        {
            return None;
        }
        Some(Self {
            provider: "Tmdb".to_owned(),
            reference: format!("/{path}"),
            url: format!("https://image.tmdb.org/t/p/w500/{path}"),
        })
    }

    /// Builds a pinned HTTPS `TheAudioDB` image reference.
    #[must_use]
    pub fn the_audio_db(url: &str) -> Option<Self> {
        let parsed = reqwest::Url::parse(url).ok()?;
        if parsed.scheme() != "https"
            || !matches!(
                parsed.host_str(),
                Some(
                    "theaudiodb.com"
                        | "www.theaudiodb.com"
                        | "r2.theaudiodb.com"
                        | "media.theaudiodb.com"
                )
            )
            || parsed.port().is_some_and(|port| port != 443)
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
            || parsed.query().is_some()
            || !valid_text(parsed.path(), 2048)
        {
            return None;
        }
        Some(Self {
            provider: "TheAudioDB".to_owned(),
            reference: parsed.path().to_owned(),
            url: parsed.into(),
        })
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn reference(&self) -> &str {
        &self.reference
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl MetadataCandidate {
    #[must_use]
    pub fn new(source: MetadataSource) -> Self {
        Self {
            title: None,
            original_title: None,
            production_year: None,
            overview: None,
            provider_ids: BTreeMap::new(),
            primary_image: None,
            genres: None,
            studios: None,
            people: None,
            source,
        }
    }

    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[must_use]
    pub fn with_original_title(mut self, title: impl Into<String>) -> Self {
        self.original_title = Some(title.into());
        self
    }

    #[must_use]
    pub const fn with_year(mut self, year: i32) -> Self {
        self.production_year = Some(year);
        self
    }

    #[must_use]
    pub fn with_overview(mut self, overview: impl Into<String>) -> Self {
        self.overview = Some(overview.into());
        self
    }

    #[must_use]
    pub fn with_provider_id(
        mut self,
        provider: impl Into<String>,
        provider_id: impl Into<String>,
    ) -> Self {
        self.provider_ids
            .insert(provider.into(), provider_id.into());
        self
    }

    #[must_use]
    pub fn with_primary_image(mut self, path: impl AsRef<str>) -> Self {
        self.primary_image = MetadataImageReference::tmdb(path.as_ref());
        self
    }

    #[must_use]
    pub fn with_image_reference(mut self, image: MetadataImageReference) -> Self {
        self.primary_image = Some(image);
        self
    }

    #[must_use]
    pub fn with_genres(mut self, genres: Vec<String>) -> Self {
        self.genres = Some(genres);
        self
    }

    #[must_use]
    pub fn with_people(mut self, people: Vec<MetadataPerson>) -> Self {
        self.people = Some(people);
        self
    }

    fn is_valid(&self) -> bool {
        self.title
            .as_deref()
            .is_none_or(|value| valid_text(value, 512))
            && self
                .original_title
                .as_deref()
                .is_none_or(|value| valid_text(value, 512))
            && self
                .production_year
                .is_none_or(|year| (1..=9999).contains(&year))
            && self
                .overview
                .as_deref()
                .is_none_or(|value| valid_text(value, MAX_TEXT_CHARS))
            && self.provider_ids.len() <= MAX_ASSOCIATIONS
            && self.provider_ids.iter().all(|(provider, provider_id)| {
                valid_text(provider, 128) && valid_text(provider_id, 2048)
            })
            && self.primary_image.as_ref().is_none_or(|image| {
                matches!(image.provider.as_str(), "Tmdb" | "TheAudioDB")
                    && valid_text(&image.reference, 2048)
                    && valid_text(&image.url, 2048)
            })
            && self.genres.as_ref().is_none_or(|values| {
                values.len() <= MAX_ASSOCIATIONS
                    && values.iter().all(|value| valid_text(value, 512))
            })
            && self.studios.as_ref().is_none_or(|values| {
                values.len() <= MAX_ASSOCIATIONS
                    && values.iter().all(|value| valid_text(value, 512))
            })
            && self.people.as_ref().is_none_or(|values| {
                values.len() <= MAX_ASSOCIATIONS
                    && values.iter().all(|person| {
                        valid_text(person.name(), 512)
                            && person.role().is_none_or(|role| valid_text(role, 512))
                            && person
                                .order()
                                .is_none_or(|order| i32::try_from(order).is_ok())
                    })
                    && values.iter().enumerate().all(|(index, person)| {
                        !values[..index].iter().any(|earlier| {
                            earlier.name() == person.name() && earlier.role() == person.role()
                        })
                    })
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MetadataProviderError {
    #[error("provider is temporarily unavailable")]
    TemporarilyUnavailable,
    #[error("provider rejected the lookup")]
    Rejected,
    #[error("provider returned invalid metadata")]
    InvalidResponse,
}

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn resolve(
        &self,
        lookup: &MetadataLookup,
    ) -> Result<Option<MetadataCandidate>, MetadataProviderError>;
}

/// A metadata provider whose active delegate can be replaced at runtime.
pub struct ReloadableMetadataProvider {
    name: &'static str,
    provider: RwLock<Option<Arc<dyn MetadataProvider>>>,
}

impl ReloadableMetadataProvider {
    /// Creates an empty wrapper with a stable provider name.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            provider: RwLock::new(None),
        }
    }

    /// Replaces the active provider, or disables resolution when passed `None`.
    pub fn replace(&self, provider: Option<Arc<dyn MetadataProvider>>) {
        *self
            .provider
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = provider;
    }
}

#[async_trait]
impl MetadataProvider for ReloadableMetadataProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn resolve(
        &self,
        lookup: &MetadataLookup,
    ) -> Result<Option<MetadataCandidate>, MetadataProviderError> {
        let provider = self
            .provider
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let Some(provider) = provider else {
            return Ok(None);
        };
        provider.resolve(lookup).await
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmdbSearchItem {
    id: u64,
    title: String,
    original_title: Option<String>,
    overview: Option<String>,
    year: Option<i32>,
    poster_path: Option<String>,
}

impl TmdbSearchItem {
    #[must_use]
    pub fn new(id: u64, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            original_title: None,
            overview: None,
            year: None,
            poster_path: None,
        }
    }

    #[must_use]
    pub fn with_details(
        mut self,
        original_title: Option<String>,
        overview: Option<String>,
        year: Option<i32>,
    ) -> Self {
        self.original_title = original_title;
        self.overview = overview;
        self.year = year;
        self
    }

    #[must_use]
    pub fn with_poster_path(mut self, path: impl Into<String>) -> Self {
        self.poster_path = Some(path.into());
        self
    }
}

#[async_trait]
pub trait TmdbTransport: Send + Sync {
    /// Validates TMDB access without performing a title search.
    ///
    /// The default preserves compatibility for alternate transports that do not support
    /// validation.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataProviderError::TemporarilyUnavailable`] by default.
    async fn validate(&self) -> Result<(), MetadataProviderError> {
        Err(MetadataProviderError::TemporarilyUnavailable)
    }

    async fn search(
        &self,
        kind: MetadataItemKind,
        query: &str,
        year: Option<i32>,
        language: &str,
    ) -> Result<Vec<TmdbSearchItem>, MetadataProviderError>;
}

pub struct TmdbProvider {
    transport: Arc<dyn TmdbTransport>,
    language: String,
}

impl TmdbProvider {
    /// Creates the production `TMDb` provider from an application access token.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidProvider`] for unsafe configuration.
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
            transport: Arc::new(ReqwestTmdbTransport::new(access_token)?),
            language,
        })
    }

    /// Creates a deterministic provider around a test or alternate transport.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidProvider`] for an invalid language.
    pub fn with_transport(
        language: impl Into<String>,
        transport: Arc<dyn TmdbTransport>,
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

    /// Validates the configured transport without performing a title search.
    ///
    /// # Errors
    ///
    /// Propagates the transport's validation failure.
    pub async fn validate_connection(&self) -> Result<(), MetadataProviderError> {
        self.transport.validate().await
    }
}

#[async_trait]
impl MetadataProvider for TmdbProvider {
    fn name(&self) -> &'static str {
        "Tmdb"
    }

    async fn resolve(
        &self,
        lookup: &MetadataLookup,
    ) -> Result<Option<MetadataCandidate>, MetadataProviderError> {
        if !matches!(
            lookup.kind(),
            MetadataItemKind::Movie | MetadataItemKind::Series
        ) {
            return Ok(None);
        }
        let results = self
            .transport
            .search(
                lookup.kind(),
                lookup.fallback_title(),
                lookup.fallback_year(),
                &self.language,
            )
            .await?;
        let selected = lookup.fallback_year().map_or_else(
            || results.first(),
            |year| results.iter().find(|item| item.year == Some(year)),
        );
        let Some(selected) = selected else {
            return Ok(None);
        };
        if selected.id == 0 || !valid_text(&selected.title, 512) {
            return Err(MetadataProviderError::InvalidResponse);
        }
        let source = MetadataSource::new(
            "Tmdb",
            Some(format!(
                "{}:{}",
                if lookup.kind() == MetadataItemKind::Movie {
                    "movie"
                } else {
                    "tv"
                },
                selected.id
            )),
            8_000,
        )
        .map_err(|_| MetadataProviderError::InvalidResponse)?;
        let mut candidate = MetadataCandidate::new(source)
            .with_title(selected.title.clone())
            .with_provider_id("tmdb", selected.id.to_string());
        if let Some(original) = &selected.original_title {
            candidate = candidate.with_original_title(original.clone());
        }
        if let Some(overview) = &selected.overview {
            candidate = candidate.with_overview(overview.clone());
        }
        if let Some(year) = selected.year {
            candidate = candidate.with_year(year);
        }
        if let Some(path) = &selected.poster_path
            && let Some(image) = MetadataImageReference::tmdb(path)
        {
            candidate.primary_image = Some(image);
        }
        Ok(Some(candidate))
    }
}

struct ReqwestTmdbTransport {
    client: reqwest::Client,
    access_token: zeroize::Zeroizing<String>,
}

impl ReqwestTmdbTransport {
    fn new(access_token: String) -> Result<Self, MetadataError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|_| MetadataError::InvalidProvider)?;
        Ok(Self {
            client,
            access_token: zeroize::Zeroizing::new(access_token),
        })
    }
}

async fn read_tmdb_response(
    mut response: reqwest::Response,
) -> Result<Vec<u8>, MetadataProviderError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_TMDB_RESPONSE_BYTES as u64)
    {
        return Err(MetadataProviderError::InvalidResponse);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| MetadataProviderError::TemporarilyUnavailable)?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_TMDB_RESPONSE_BYTES {
            return Err(MetadataProviderError::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn validate_tmdb_configuration_response(bytes: &[u8]) -> Result<(), MetadataProviderError> {
    let response: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| MetadataProviderError::InvalidResponse)?;
    let Some(response) = response.as_object() else {
        return Err(MetadataProviderError::InvalidResponse);
    };
    if !response
        .get("images")
        .is_some_and(serde_json::Value::is_object)
        || !response
            .get("change_keys")
            .is_some_and(serde_json::Value::is_array)
    {
        return Err(MetadataProviderError::InvalidResponse);
    }
    Ok(())
}

#[derive(serde::Deserialize)]
struct TmdbSearchResponse {
    results: Vec<TmdbWireItem>,
}

#[derive(serde::Deserialize)]
struct TmdbWireItem {
    id: u64,
    title: Option<String>,
    name: Option<String>,
    original_title: Option<String>,
    original_name: Option<String>,
    overview: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    poster_path: Option<String>,
}

#[async_trait]
impl TmdbTransport for ReqwestTmdbTransport {
    async fn validate(&self) -> Result<(), MetadataProviderError> {
        let response = self
            .client
            .get("https://api.themoviedb.org/3/configuration")
            .bearer_auth(self.access_token.as_str())
            .send()
            .await
            .map_err(|_| MetadataProviderError::TemporarilyUnavailable)?;
        if !response.status().is_success() {
            return Err(
                if matches!(
                    response.status(),
                    reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
                ) {
                    MetadataProviderError::Rejected
                } else if response.status().is_server_error()
                    || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || response.status() == reqwest::StatusCode::REQUEST_TIMEOUT
                {
                    MetadataProviderError::TemporarilyUnavailable
                } else {
                    MetadataProviderError::InvalidResponse
                },
            );
        }
        validate_tmdb_configuration_response(&read_tmdb_response(response).await?)
    }

    async fn search(
        &self,
        kind: MetadataItemKind,
        query: &str,
        year: Option<i32>,
        language: &str,
    ) -> Result<Vec<TmdbSearchItem>, MetadataProviderError> {
        let endpoint = if kind == MetadataItemKind::Movie {
            "https://api.themoviedb.org/3/search/movie"
        } else {
            "https://api.themoviedb.org/3/search/tv"
        };
        let mut parameters = vec![
            ("query", query.to_owned()),
            ("language", language.to_owned()),
            ("include_adult", "false".to_owned()),
            ("page", "1".to_owned()),
        ];
        if let Some(year) = year {
            parameters.push((
                if kind == MetadataItemKind::Movie {
                    "year"
                } else {
                    "first_air_date_year"
                },
                year.to_string(),
            ));
        }
        let response = self
            .client
            .get(endpoint)
            .bearer_auth(self.access_token.as_str())
            .query(&parameters)
            .send()
            .await
            .map_err(|_| MetadataProviderError::TemporarilyUnavailable)?;
        if !response.status().is_success() {
            return Err(
                if response.status().is_server_error()
                    || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                {
                    MetadataProviderError::TemporarilyUnavailable
                } else {
                    MetadataProviderError::Rejected
                },
            );
        }
        let bytes = read_tmdb_response(response).await?;
        let wire: TmdbSearchResponse =
            serde_json::from_slice(&bytes).map_err(|_| MetadataProviderError::InvalidResponse)?;
        if wire.results.len() > 100 {
            return Err(MetadataProviderError::InvalidResponse);
        }
        wire.results
            .into_iter()
            .map(|item| {
                let title = item
                    .title
                    .or(item.name)
                    .ok_or(MetadataProviderError::InvalidResponse)?;
                let date = item.release_date.or(item.first_air_date);
                let year = date
                    .as_deref()
                    .and_then(|value| value.get(..4))
                    .and_then(|value| value.parse().ok());
                let mut result = TmdbSearchItem::new(item.id, title).with_details(
                    item.original_title.or(item.original_name),
                    item.overview.filter(|value| !value.is_empty()),
                    year,
                );
                if let Some(path) = item.poster_path {
                    result = result.with_poster_path(path);
                }
                Ok(result)
            })
            .collect()
    }
}

pub struct MetadataResolver {
    providers: Vec<Arc<dyn MetadataProvider>>,
}

impl MetadataResolver {
    /// Creates an ordered provider chain. Earlier providers retain field precedence.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidProvider`] for empty or duplicate provider names.
    pub fn new(providers: Vec<Arc<dyn MetadataProvider>>) -> Result<Self, MetadataError> {
        let mut names = Vec::with_capacity(providers.len());
        for provider in &providers {
            if !valid_text(provider.name(), 128) || names.contains(&provider.name()) {
                return Err(MetadataError::InvalidProvider);
            }
            names.push(provider.name());
        }
        Ok(Self { providers })
    }

    #[must_use]
    pub async fn resolve(&self, lookup: &MetadataLookup) -> MetadataResolution {
        self.resolve_builder(lookup, ResolutionBuilder::default())
            .await
    }

    /// Resolves providers after an already-loaded higher-precedence candidate.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidCandidate`] when the initial candidate is invalid.
    pub async fn resolve_with_candidate(
        &self,
        lookup: &MetadataLookup,
        candidate: MetadataCandidate,
    ) -> Result<MetadataResolution, MetadataError> {
        if !candidate.is_valid() {
            return Err(MetadataError::InvalidCandidate);
        }
        let mut builder = ResolutionBuilder::default();
        builder.merge(candidate);
        Ok(self.resolve_builder(lookup, builder).await)
    }

    async fn resolve_builder(
        &self,
        lookup: &MetadataLookup,
        mut builder: ResolutionBuilder,
    ) -> MetadataResolution {
        let mut warnings = Vec::new();
        for provider in &self.providers {
            match provider.resolve(lookup).await {
                Ok(Some(candidate)) if candidate.is_valid() => builder.merge(candidate),
                Ok(Some(_)) => warnings.push(MetadataWarning {
                    provider: provider.name(),
                    error: MetadataProviderError::InvalidResponse,
                }),
                Ok(None) => {}
                Err(error) => warnings.push(MetadataWarning {
                    provider: provider.name(),
                    error,
                }),
            }
        }
        builder.with_fallback(lookup);
        builder.finish(lookup.kind, warnings)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataWarning {
    provider: &'static str,
    error: MetadataProviderError,
}

impl MetadataWarning {
    #[must_use]
    pub const fn provider(&self) -> &'static str {
        self.provider
    }

    #[must_use]
    pub const fn error(&self) -> MetadataProviderError {
        self.error
    }
}

#[derive(Clone, Debug)]
pub struct FieldProvenance {
    source: MetadataSource,
}

impl FieldProvenance {
    #[must_use]
    pub fn provider(&self) -> &str {
        self.source.provider()
    }

    #[must_use]
    pub fn reference(&self) -> Option<&str> {
        self.source.reference()
    }

    #[must_use]
    pub const fn confidence(&self) -> u16 {
        self.source.confidence()
    }
}

#[derive(Clone, Debug)]
pub struct MetadataResolution {
    item_kind: MetadataItemKind,
    title: String,
    original_title: Option<String>,
    production_year: Option<i32>,
    overview: Option<String>,
    provider_ids: BTreeMap<String, String>,
    primary_image: Option<MetadataImageReference>,
    genres: Option<Vec<String>>,
    studios: Option<Vec<String>>,
    people: Option<Vec<MetadataPerson>>,
    provenance: BTreeMap<String, FieldProvenance>,
    state: MetadataState,
    warnings: Vec<MetadataWarning>,
}

impl MetadataResolution {
    /// Resolves one already-loaded candidate with naming evidence as fallback.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidCandidate`] when candidate fields exceed their contract.
    pub fn from_candidate(
        lookup: &MetadataLookup,
        candidate: MetadataCandidate,
    ) -> Result<Self, MetadataError> {
        if !candidate.is_valid() {
            return Err(MetadataError::InvalidCandidate);
        }
        let mut builder = ResolutionBuilder::default();
        builder.merge(candidate);
        builder.with_fallback(lookup);
        Ok(builder.finish(lookup.kind, Vec::new()))
    }

    #[must_use]
    pub const fn item_kind(&self) -> MetadataItemKind {
        self.item_kind
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
    pub const fn production_year(&self) -> Option<i32> {
        self.production_year
    }

    #[must_use]
    pub fn overview(&self) -> Option<&str> {
        self.overview.as_deref()
    }

    #[must_use]
    pub fn provider_ids(&self) -> &BTreeMap<String, String> {
        &self.provider_ids
    }

    #[must_use]
    pub fn primary_image(&self) -> Option<&MetadataImageReference> {
        self.primary_image.as_ref()
    }

    #[must_use]
    pub fn genres(&self) -> Option<&[String]> {
        self.genres.as_deref()
    }

    #[must_use]
    pub fn studios(&self) -> Option<&[String]> {
        self.studios.as_deref()
    }

    #[must_use]
    pub fn people(&self) -> Option<&[MetadataPerson]> {
        self.people.as_deref()
    }

    #[must_use]
    pub fn provenance(&self, field: &str) -> Option<&FieldProvenance> {
        self.provenance.get(field)
    }

    pub fn provenance_entries(&self) -> impl Iterator<Item = (&str, &FieldProvenance)> {
        self.provenance
            .iter()
            .map(|(field, provenance)| (field.as_str(), provenance))
    }

    #[must_use]
    pub const fn state(&self) -> MetadataState {
        self.state
    }

    #[must_use]
    pub fn warnings(&self) -> &[MetadataWarning] {
        &self.warnings
    }
}

struct SourcedValue<T> {
    value: T,
    source: MetadataSource,
}

#[derive(Default)]
struct ResolutionBuilder {
    title: Option<SourcedValue<String>>,
    original_title: Option<SourcedValue<String>>,
    production_year: Option<SourcedValue<i32>>,
    overview: Option<SourcedValue<String>>,
    provider_ids: BTreeMap<String, SourcedValue<String>>,
    primary_image: Option<MetadataImageReference>,
    genres: Option<Vec<String>>,
    studios: Option<Vec<String>>,
    people: Option<Vec<MetadataPerson>>,
}

impl ResolutionBuilder {
    fn merge(&mut self, candidate: MetadataCandidate) {
        merge_value(&mut self.title, candidate.title, &candidate.source);
        merge_value(
            &mut self.original_title,
            candidate.original_title,
            &candidate.source,
        );
        merge_value(
            &mut self.production_year,
            candidate.production_year,
            &candidate.source,
        );
        merge_value(&mut self.overview, candidate.overview, &candidate.source);
        for (provider, value) in candidate.provider_ids {
            self.provider_ids.entry(provider).or_insert(SourcedValue {
                value,
                source: candidate.source.clone(),
            });
        }
        if self.primary_image.is_none() {
            self.primary_image = candidate.primary_image;
        }
        if self.genres.is_none() {
            self.genres = candidate.genres;
        }
        if self.studios.is_none() {
            self.studios = candidate.studios;
        }
        if self.people.is_none() {
            self.people = candidate.people;
        }
    }

    fn with_fallback(&mut self, lookup: &MetadataLookup) {
        let naming = MetadataSource::new("Naming", Option::<String>::None, 4_000)
            .expect("static naming source is valid");
        self.title.get_or_insert_with(|| SourcedValue {
            value: lookup.fallback_title.clone(),
            source: naming.clone(),
        });
        if let Some(year) = lookup.fallback_year {
            self.production_year.get_or_insert(SourcedValue {
                value: year,
                source: naming,
            });
        }
    }

    fn finish(
        self,
        item_kind: MetadataItemKind,
        warnings: Vec<MetadataWarning>,
    ) -> MetadataResolution {
        let title = self.title.expect("naming fallback always supplies a title");
        let mut provenance = BTreeMap::new();
        provenance.insert(
            "title".to_owned(),
            FieldProvenance {
                source: title.source,
            },
        );
        let original_title = sourced_field("original_title", self.original_title, &mut provenance);
        let production_year =
            sourced_field("production_year", self.production_year, &mut provenance);
        let overview = sourced_field("overview", self.overview, &mut provenance);
        let mut provider_ids = BTreeMap::new();
        for (provider, sourced) in self.provider_ids {
            provenance.insert(
                format!("provider_id:{provider}"),
                FieldProvenance {
                    source: sourced.source,
                },
            );
            provider_ids.insert(provider, sourced.value);
        }
        let state = completeness(
            Some(&title.value),
            production_year,
            overview.as_deref(),
            &provider_ids,
        );
        MetadataResolution {
            item_kind,
            title: title.value,
            original_title,
            production_year,
            overview,
            provider_ids,
            primary_image: self.primary_image,
            genres: self.genres,
            studios: self.studios,
            people: self.people,
            provenance,
            state,
            warnings,
        }
    }
}

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("metadata input is empty or exceeds its byte limit")]
    InputTooLarge,
    #[error("metadata XML is malformed")]
    MalformedXml,
    #[error("metadata XML contains a document type or unsupported entity")]
    UnsafeXml,
    #[error("metadata XML nesting exceeds its limit")]
    NestingTooDeep,
    #[error("metadata field exceeds its limit")]
    FieldTooLarge,
    #[error("metadata document type is unsupported")]
    UnsupportedDocument,
    #[error("metadata document has no title")]
    MissingTitle,
    #[error("metadata year is invalid")]
    InvalidYear,
    #[error("metadata association is invalid")]
    InvalidAssociation,
    #[error("metadata document has too many associations")]
    TooManyAssociations,
    #[error("metadata source is invalid")]
    InvalidSource,
    #[error("metadata lookup is invalid")]
    InvalidLookup,
    #[error("metadata provider chain is invalid")]
    InvalidProvider,
    #[error("metadata candidate is invalid")]
    InvalidCandidate,
}

fn tag_name(raw: &[u8]) -> Result<String, MetadataError> {
    let name = std::str::from_utf8(raw).map_err(|_| MetadataError::MalformedXml)?;
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(MetadataError::MalformedXml);
    }
    Ok(name.to_ascii_lowercase())
}

fn attribute(
    element: &quick_xml::events::BytesStart<'_>,
    key: &[u8],
) -> Result<Option<String>, MetadataError> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| MetadataError::MalformedXml)?;
        if attribute.key.as_ref() == key {
            let value = std::str::from_utf8(attribute.value.as_ref())
                .map_err(|_| MetadataError::MalformedXml)?;
            if !valid_text(value, 128) || value.contains('&') {
                return Err(MetadataError::UnsafeXml);
            }
            return Ok(Some(value.to_ascii_lowercase()));
        }
    }
    Ok(None)
}

fn safe_entity(reference: &str) -> Result<String, MetadataError> {
    match reference {
        "amp" => Ok("&".to_owned()),
        "lt" => Ok("<".to_owned()),
        "gt" => Ok(">".to_owned()),
        "apos" => Ok("'".to_owned()),
        "quot" => Ok("\"".to_owned()),
        _ => {
            let (digits, radix) = reference.strip_prefix("#x").map_or_else(
                || (reference.strip_prefix('#'), 10),
                |digits| (Some(digits), 16),
            );
            let scalar = digits
                .filter(|digits| !digits.is_empty())
                .and_then(|digits| u32::from_str_radix(digits, radix).ok())
                .and_then(char::from_u32)
                .filter(|character| !character.is_control())
                .ok_or(MetadataError::UnsafeXml)?;
            Ok(scalar.to_string())
        }
    }
}

fn xml_error(_: quick_xml::encoding::EncodingError) -> MetadataError {
    MetadataError::MalformedXml
}

fn set_once(
    target: &mut Option<String>,
    value: &str,
    max_chars: usize,
) -> Result<(), MetadataError> {
    if value.is_empty() || target.is_some() {
        return Ok(());
    }
    if !valid_text(value, max_chars) {
        return Err(MetadataError::FieldTooLarge);
    }
    *target = Some(value.to_owned());
    Ok(())
}

fn insert_provider_id(
    target: &mut BTreeMap<String, String>,
    provider: &str,
    value: &str,
) -> Result<(), MetadataError> {
    let provider = provider.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok(());
    }
    if !valid_text(&provider, 128) || !valid_text(value, 2048) {
        return Err(MetadataError::InvalidAssociation);
    }
    target.entry(provider).or_insert_with(|| value.to_owned());
    Ok(())
}

fn push_unique(target: &mut Vec<String>, value: &str) -> Result<(), MetadataError> {
    if value.is_empty() || target.iter().any(|existing| existing == value) {
        return Ok(());
    }
    if !valid_text(value, 512) {
        return Err(MetadataError::InvalidAssociation);
    }
    if target.len() >= MAX_ASSOCIATIONS {
        return Err(MetadataError::TooManyAssociations);
    }
    target.push(value.to_owned());
    Ok(())
}

fn merge_value<T>(target: &mut Option<SourcedValue<T>>, value: Option<T>, source: &MetadataSource) {
    if target.is_none() {
        *target = value.map(|value| SourcedValue {
            value,
            source: source.clone(),
        });
    }
}

fn sourced_field<T>(
    field: &str,
    sourced: Option<SourcedValue<T>>,
    provenance: &mut BTreeMap<String, FieldProvenance>,
) -> Option<T> {
    sourced.map(|sourced| {
        provenance.insert(
            field.to_owned(),
            FieldProvenance {
                source: sourced.source,
            },
        );
        sourced.value
    })
}

fn completeness(
    title: Option<&str>,
    year: Option<i32>,
    overview: Option<&str>,
    provider_ids: &BTreeMap<String, String>,
) -> MetadataState {
    if title.is_some_and(|value| !value.is_empty())
        && year.is_some()
        && overview.is_some_and(|value| !value.is_empty())
        && !provider_ids.is_empty()
    {
        MetadataState::Ready
    } else {
        MetadataState::Partial
    }
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}
