use std::{fmt::Write, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde::Deserialize;

use crate::{
    MetadataCandidate, MetadataError, MetadataImageReference, MetadataItemKind, MetadataLookup,
    MetadataPerson, MetadataProvider, MetadataProviderError, MetadataSource, valid_text,
};

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MUSICBRAINZ_MIN_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, Eq, PartialEq)]
struct AudioSearch {
    artist: Option<String>,
    title: String,
}

fn audio_search(value: &str) -> AudioSearch {
    let mut parts = value
        .split(" - ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() >= 2
        && parts[0]
            .trim_end_matches(['.', '-'])
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        parts.remove(0);
    }
    if parts.len() >= 2 {
        AudioSearch {
            artist: Some(parts.remove(0).to_owned()),
            title: parts.join(" - "),
        }
    } else {
        AudioSearch {
            artist: None,
            title: parts.first().copied().unwrap_or(value).trim().to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AudioDbTrack {
    id: String,
    title: String,
    artist: String,
    year: Option<i32>,
    overview: Option<String>,
    genre: Option<String>,
    musicbrainz_id: Option<String>,
    image_url: Option<String>,
}

#[async_trait]
trait AudioDbTransport: Send + Sync {
    async fn search_track(
        &self,
        artist: &str,
        title: &str,
    ) -> Result<Vec<AudioDbTrack>, MetadataProviderError>;
}

pub struct TheAudioDbProvider {
    transport: Arc<dyn AudioDbTransport>,
}

impl TheAudioDbProvider {
    /// Creates the production `TheAudioDB` provider.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidProvider`] for an invalid API key.
    pub fn new(api_key: impl Into<String>) -> Result<Self, MetadataError> {
        let api_key = api_key.into();
        if api_key.is_empty()
            || api_key.len() > 256
            || !api_key.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
            })
        {
            return Err(MetadataError::InvalidProvider);
        }
        Ok(Self {
            transport: Arc::new(ReqwestAudioDbTransport::new(api_key)?),
        })
    }

    #[cfg(test)]
    fn with_transport(transport: Arc<dyn AudioDbTransport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl MetadataProvider for TheAudioDbProvider {
    fn name(&self) -> &'static str {
        "TheAudioDB"
    }

    async fn resolve(
        &self,
        lookup: &MetadataLookup,
    ) -> Result<Option<MetadataCandidate>, MetadataProviderError> {
        if lookup.kind() != MetadataItemKind::Audio {
            return Ok(None);
        }
        let search = audio_search(lookup.fallback_title());
        let Some(artist) = search.artist.as_deref() else {
            return Ok(None);
        };
        let tracks = self.transport.search_track(artist, &search.title).await?;
        let selected = tracks
            .iter()
            .find(|track| {
                track.title.eq_ignore_ascii_case(&search.title)
                    && track.artist.eq_ignore_ascii_case(artist)
            })
            .or_else(|| tracks.first());
        let Some(selected) = selected else {
            return Ok(None);
        };
        if !valid_text(&selected.id, 2048)
            || !valid_text(&selected.title, 512)
            || !valid_text(&selected.artist, 512)
        {
            return Err(MetadataProviderError::InvalidResponse);
        }
        let source =
            MetadataSource::new("TheAudioDB", Some(format!("track:{}", selected.id)), 8_500)
                .map_err(|_| MetadataProviderError::InvalidResponse)?;
        let artist_credit = MetadataPerson::new(selected.artist.clone(), Some("Artist"), Some(0))
            .map_err(|_| MetadataProviderError::InvalidResponse)?;
        let mut candidate = MetadataCandidate::new(source)
            .with_title(selected.title.clone())
            .with_provider_id("theaudiodb", selected.id.clone())
            .with_people(vec![artist_credit]);
        if let Some(year) = selected.year {
            candidate = candidate.with_year(year);
        }
        if let Some(overview) = selected
            .overview
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            candidate = candidate.with_overview(overview.to_owned());
        }
        if let Some(genre) = selected
            .genre
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            candidate = candidate.with_genres(vec![genre.to_owned()]);
        }
        if let Some(id) = selected
            .musicbrainz_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            candidate = candidate.with_provider_id("musicbrainz:recording", id.to_owned());
        }
        if let Some(image) = selected
            .image_url
            .as_deref()
            .and_then(MetadataImageReference::the_audio_db)
        {
            candidate = candidate.with_image_reference(image);
        }
        Ok(Some(candidate))
    }
}

struct ReqwestAudioDbTransport {
    client: reqwest::Client,
    api_key: String,
}

impl ReqwestAudioDbTransport {
    fn new(api_key: String) -> Result<Self, MetadataError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|_| MetadataError::InvalidProvider)?;
        Ok(Self { client, api_key })
    }
}

#[async_trait]
impl AudioDbTransport for ReqwestAudioDbTransport {
    async fn search_track(
        &self,
        artist: &str,
        title: &str,
    ) -> Result<Vec<AudioDbTrack>, MetadataProviderError> {
        let url = format!(
            "https://www.theaudiodb.com/api/v1/json/{}/searchtrack.php",
            self.api_key
        );
        let response = self
            .client
            .get(url)
            .query(&[("s", artist), ("t", title)])
            .send()
            .await
            .map_err(|_| MetadataProviderError::TemporarilyUnavailable)?;
        let bytes = read_response(response).await?;
        let wire: AudioDbSearchResponse =
            serde_json::from_slice(&bytes).map_err(|_| MetadataProviderError::InvalidResponse)?;
        wire.track
            .unwrap_or_default()
            .into_iter()
            .map(AudioDbTrack::try_from)
            .collect()
    }
}

#[derive(Deserialize)]
struct AudioDbSearchResponse {
    track: Option<Vec<AudioDbTrackWire>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AudioDbTrackWire {
    #[serde(rename = "idTrack")]
    id: String,
    #[serde(rename = "strTrack")]
    title: String,
    #[serde(rename = "strArtist")]
    artist: String,
    #[serde(rename = "intYearReleased")]
    year: Option<String>,
    #[serde(rename = "strDescriptionEN")]
    overview: Option<String>,
    #[serde(rename = "strGenre")]
    genre: Option<String>,
    #[serde(rename = "strMusicBrainzID")]
    musicbrainz_id: Option<String>,
    #[serde(rename = "strTrackThumb")]
    image_url: Option<String>,
}

impl TryFrom<AudioDbTrackWire> for AudioDbTrack {
    type Error = MetadataProviderError;

    fn try_from(value: AudioDbTrackWire) -> Result<Self, Self::Error> {
        let year = value.year.as_deref().and_then(parse_year);
        Ok(Self {
            id: value.id,
            title: value.title,
            artist: value.artist,
            year,
            overview: value.overview,
            genre: value.genre,
            musicbrainz_id: value.musicbrainz_id,
            image_url: value.image_url,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MusicBrainzRecording {
    id: String,
    title: String,
    artist_id: Option<String>,
    artist: Option<String>,
    release_group_id: Option<String>,
    year: Option<i32>,
    genres: Vec<String>,
}

#[async_trait]
trait MusicBrainzTransport: Send + Sync {
    async fn search_recording(
        &self,
        artist: Option<&str>,
        title: &str,
    ) -> Result<Vec<MusicBrainzRecording>, MetadataProviderError>;
}

pub struct MusicBrainzProvider {
    transport: Arc<dyn MusicBrainzTransport>,
}

impl MusicBrainzProvider {
    /// Creates the production `MusicBrainz` provider with the required identifying User-Agent.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError::InvalidProvider`] for an invalid User-Agent.
    pub fn new(user_agent: impl Into<String>) -> Result<Self, MetadataError> {
        let user_agent = user_agent.into();
        if !valid_text(&user_agent, 512) {
            return Err(MetadataError::InvalidProvider);
        }
        Ok(Self {
            transport: Arc::new(ReqwestMusicBrainzTransport::new(user_agent)?),
        })
    }

    #[cfg(test)]
    fn with_transport(transport: Arc<dyn MusicBrainzTransport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl MetadataProvider for MusicBrainzProvider {
    fn name(&self) -> &'static str {
        "MusicBrainz"
    }

    async fn resolve(
        &self,
        lookup: &MetadataLookup,
    ) -> Result<Option<MetadataCandidate>, MetadataProviderError> {
        if lookup.kind() != MetadataItemKind::Audio {
            return Ok(None);
        }
        let search = audio_search(lookup.fallback_title());
        let recordings = self
            .transport
            .search_recording(search.artist.as_deref(), &search.title)
            .await?;
        let selected = recordings
            .iter()
            .find(|recording| {
                recording.title.eq_ignore_ascii_case(&search.title)
                    && search.artist.as_deref().is_none_or(|artist| {
                        recording
                            .artist
                            .as_deref()
                            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(artist))
                    })
            })
            .or_else(|| recordings.first());
        let Some(selected) = selected else {
            return Ok(None);
        };
        if uuid::Uuid::parse_str(&selected.id).is_err() || !valid_text(&selected.title, 512) {
            return Err(MetadataProviderError::InvalidResponse);
        }
        let source = MetadataSource::new(
            "MusicBrainz",
            Some(format!("recording:{}", selected.id)),
            7_500,
        )
        .map_err(|_| MetadataProviderError::InvalidResponse)?;
        let mut candidate = MetadataCandidate::new(source)
            .with_title(selected.title.clone())
            .with_provider_id("musicbrainz:recording", selected.id.clone());
        if let Some(id) = selected.artist_id.as_deref() {
            candidate = candidate.with_provider_id("musicbrainz:artist", id.to_owned());
        }
        if let Some(id) = selected.release_group_id.as_deref() {
            candidate = candidate.with_provider_id("musicbrainz:release_group", id.to_owned());
        }
        if let Some(year) = selected.year {
            candidate = candidate.with_year(year);
        }
        if !selected.genres.is_empty() {
            candidate = candidate.with_genres(selected.genres.clone());
        }
        if let Some(artist) = selected.artist.as_deref() {
            let credit = MetadataPerson::new(artist.to_owned(), Some("Artist"), Some(0))
                .map_err(|_| MetadataProviderError::InvalidResponse)?;
            candidate = candidate.with_people(vec![credit]);
        }
        Ok(Some(candidate))
    }
}

struct ReqwestMusicBrainzTransport {
    client: reqwest::Client,
    next_request: tokio::sync::Mutex<tokio::time::Instant>,
}

impl ReqwestMusicBrainzTransport {
    fn new(user_agent: String) -> Result<Self, MetadataError> {
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|_| MetadataError::InvalidProvider)?;
        Ok(Self {
            client,
            next_request: tokio::sync::Mutex::new(tokio::time::Instant::now()),
        })
    }

    async fn throttle(&self) {
        let mut next = self.next_request.lock().await;
        let now = tokio::time::Instant::now();
        if *next > now {
            tokio::time::sleep_until(*next).await;
        }
        *next = tokio::time::Instant::now() + MUSICBRAINZ_MIN_INTERVAL;
    }
}

#[async_trait]
impl MusicBrainzTransport for ReqwestMusicBrainzTransport {
    async fn search_recording(
        &self,
        artist: Option<&str>,
        title: &str,
    ) -> Result<Vec<MusicBrainzRecording>, MetadataProviderError> {
        self.throttle().await;
        let mut query = format!("recording:\"{}\"", lucene_escape(title));
        if let Some(artist) = artist {
            write!(query, " AND artist:\"{}\"", lucene_escape(artist))
                .expect("writing to a String cannot fail");
        }
        let response = self
            .client
            .get("https://musicbrainz.org/ws/2/recording/")
            .query(&[("query", query.as_str()), ("fmt", "json"), ("limit", "5")])
            .send()
            .await
            .map_err(|_| MetadataProviderError::TemporarilyUnavailable)?;
        let bytes = read_response(response).await?;
        let wire: MusicBrainzSearchResponse =
            serde_json::from_slice(&bytes).map_err(|_| MetadataProviderError::InvalidResponse)?;
        wire.recordings
            .into_iter()
            .map(MusicBrainzRecording::try_from)
            .collect()
    }
}

#[derive(Deserialize)]
struct MusicBrainzSearchResponse {
    recordings: Vec<MusicBrainzRecordingWire>,
}

#[derive(Deserialize)]
struct MusicBrainzRecordingWire {
    id: String,
    title: String,
    #[serde(rename = "first-release-date")]
    first_release_date: Option<String>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<MusicBrainzArtistCreditWire>,
    #[serde(default)]
    releases: Vec<MusicBrainzReleaseWire>,
    #[serde(default)]
    genres: Vec<MusicBrainzGenreWire>,
}

#[derive(Deserialize)]
struct MusicBrainzArtistCreditWire {
    name: String,
    artist: MusicBrainzArtistWire,
}

#[derive(Deserialize)]
struct MusicBrainzArtistWire {
    id: String,
}

#[derive(Deserialize)]
struct MusicBrainzReleaseWire {
    #[serde(rename = "release-group")]
    release_group: Option<MusicBrainzReleaseGroupWire>,
}

#[derive(Deserialize)]
struct MusicBrainzReleaseGroupWire {
    id: String,
}

#[derive(Deserialize)]
struct MusicBrainzGenreWire {
    name: String,
}

impl TryFrom<MusicBrainzRecordingWire> for MusicBrainzRecording {
    type Error = MetadataProviderError;

    fn try_from(value: MusicBrainzRecordingWire) -> Result<Self, Self::Error> {
        let artist = value.artist_credit.first();
        let release_group_id = value
            .releases
            .iter()
            .find_map(|release| release.release_group.as_ref())
            .map(|group| group.id.clone());
        Ok(Self {
            id: value.id,
            title: value.title,
            artist_id: artist.map(|credit| credit.artist.id.clone()),
            artist: artist.map(|credit| credit.name.clone()),
            release_group_id,
            year: value.first_release_date.as_deref().and_then(parse_year),
            genres: value
                .genres
                .into_iter()
                .map(|genre| genre.name)
                .filter(|name| !name.trim().is_empty())
                .take(32)
                .collect(),
        })
    }
}

async fn read_response(mut response: reqwest::Response) -> Result<Vec<u8>, MetadataProviderError> {
    if !response.status().is_success() {
        return Err(
            if matches!(
                response.status(),
                reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
            ) {
                MetadataProviderError::Rejected
            } else if response.status().is_server_error()
                || matches!(
                    response.status(),
                    reqwest::StatusCode::TOO_MANY_REQUESTS | reqwest::StatusCode::REQUEST_TIMEOUT
                )
            {
                MetadataProviderError::TemporarilyUnavailable
            } else {
                MetadataProviderError::InvalidResponse
            },
        );
    }
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

fn parse_year(value: &str) -> Option<i32> {
    value
        .get(..4)?
        .parse::<i32>()
        .ok()
        .filter(|year| (1..=9999).contains(year))
}

fn lucene_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::{
        AudioDbTrack, AudioDbTransport, MusicBrainzProvider, MusicBrainzRecording,
        MusicBrainzTransport, TheAudioDbProvider, audio_search,
    };
    use crate::{MetadataItemKind, MetadataLookup, MetadataProvider};

    struct AudioDbFixture;

    #[async_trait]
    impl AudioDbTransport for AudioDbFixture {
        async fn search_track(
            &self,
            _artist: &str,
            _title: &str,
        ) -> Result<Vec<AudioDbTrack>, crate::MetadataProviderError> {
            Ok(vec![AudioDbTrack {
                id: "32778411".to_owned(),
                title: "Yellow".to_owned(),
                artist: "Coldplay".to_owned(),
                year: Some(2000),
                overview: Some("A single by Coldplay.".to_owned()),
                genre: Some("Alternative Rock".to_owned()),
                musicbrainz_id: Some("a1f8f8e1-1d21-4b82-9e6f-1f6020480173".to_owned()),
                image_url: Some(
                    "https://www.theaudiodb.com/images/media/track/thumb/example.jpg".to_owned(),
                ),
            }])
        }
    }

    struct MusicBrainzFixture;

    #[async_trait]
    impl MusicBrainzTransport for MusicBrainzFixture {
        async fn search_recording(
            &self,
            _artist: Option<&str>,
            _title: &str,
        ) -> Result<Vec<MusicBrainzRecording>, crate::MetadataProviderError> {
            Ok(vec![MusicBrainzRecording {
                id: "a1f8f8e1-1d21-4b82-9e6f-1f6020480173".to_owned(),
                title: "Yellow".to_owned(),
                artist_id: Some("cc197bad-dc9c-440d-a5b5-d52ba2e14234".to_owned()),
                artist: Some("Coldplay".to_owned()),
                release_group_id: Some("8f4d6df8-4b06-3a53-9ae9-8a4ec9ab3200".to_owned()),
                year: Some(2000),
                genres: vec!["alternative rock".to_owned()],
            }])
        }
    }

    #[test]
    fn parses_common_artist_track_names() {
        assert_eq!(
            audio_search("01 - Coldplay - Yellow"),
            super::AudioSearch {
                artist: Some("Coldplay".to_owned()),
                title: "Yellow".to_owned(),
            }
        );
    }

    #[tokio::test]
    async fn the_audio_db_resolves_audio_metadata_and_artwork() {
        let provider = TheAudioDbProvider::with_transport(Arc::new(AudioDbFixture));
        let lookup =
            MetadataLookup::new(MetadataItemKind::Audio, "Coldplay - Yellow", None).unwrap();
        let candidate = provider.resolve(&lookup).await.unwrap().unwrap();
        let resolution = crate::MetadataResolution::from_candidate(&lookup, candidate).unwrap();
        assert_eq!(resolution.title(), "Yellow");
        assert_eq!(resolution.production_year(), Some(2000));
        assert_eq!(resolution.provider_ids()["theaudiodb"], "32778411");
        assert_eq!(resolution.primary_image().unwrap().provider(), "TheAudioDB");
    }

    #[tokio::test]
    async fn musicbrainz_resolves_recording_artist_and_release_group_ids() {
        let provider = MusicBrainzProvider::with_transport(Arc::new(MusicBrainzFixture));
        let lookup =
            MetadataLookup::new(MetadataItemKind::Audio, "Coldplay - Yellow", None).unwrap();
        let candidate = provider.resolve(&lookup).await.unwrap().unwrap();
        let resolution = crate::MetadataResolution::from_candidate(&lookup, candidate).unwrap();
        assert_eq!(resolution.title(), "Yellow");
        assert_eq!(
            resolution.provider_ids()["musicbrainz:artist"],
            "cc197bad-dc9c-440d-a5b5-d52ba2e14234"
        );
        assert_eq!(
            resolution.provider_ids()["musicbrainz:release_group"],
            "8f4d6df8-4b06-3a53-9ae9-8a4ec9ab3200"
        );
    }
}
