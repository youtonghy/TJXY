use std::{collections::HashSet, path::Path};

use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::{MediaLocationId, MediaSourceId, PresentationKey, SubtitleId};
use tjxy_db::{
    CatalogPublicationError, CatalogPublicationRepository, ClaimedWorkJob,
    MediaLocationPublicationRow, MediaSourcePublicationRow, SourceIndexObject,
    SourceIndexRepository, SourceIndexRepositoryError, SourcePublicationManifest,
    SubtitlePublicationRow, WorkJobRepository,
};

pub struct SourceIndexService {
    database: DatabaseConnection,
}

impl SourceIndexService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Classifies reconciled SQL objects and atomically publishes one source projection.
    ///
    /// # Errors
    ///
    /// Returns [`SourceIndexError`] for missing media, invalid inventory, or publication failure.
    pub async fn execute(&self, claimed: &ClaimedWorkJob) -> Result<i64, SourceIndexError> {
        let snapshot = SourceIndexRepository::new(&self.database)
            .snapshot(claimed)
            .await?;
        let graph = build_graph(
            snapshot.objects(),
            snapshot.restrict_to_stable_sources(),
            snapshot.is_audio(),
        )?;
        let manifest = SourcePublicationManifest::from_rows(
            &graph.sources,
            &graph.locations,
            &graph.subtitles,
        )?;
        let publications = CatalogPublicationRepository::new(&self.database);
        let publication = publications.begin_sources(claimed, &manifest).await?;
        publications
            .stage_source_batch(
                claimed,
                publication,
                &graph.sources,
                &graph.locations,
                &graph.subtitles,
            )
            .await?;
        publications.seal_sources(claimed, publication).await?;
        publications
            .publish_sources(
                &WorkJobRepository::new(&self.database),
                claimed,
                publication,
            )
            .await
            .map_err(Into::into)
    }
}

struct SourceGraph {
    sources: Vec<MediaSourcePublicationRow>,
    locations: Vec<MediaLocationPublicationRow>,
    subtitles: Vec<SubtitlePublicationRow>,
}

struct MediaFile {
    stem: String,
    source: MediaSourceId,
}

fn build_graph(
    objects: &[SourceIndexObject],
    restrict_to_stable_sources: bool,
    is_audio: bool,
) -> Result<SourceGraph, SourceIndexError> {
    let mut sources = Vec::new();
    let mut source_ids = HashSet::new();
    let mut locations = Vec::new();
    let mut media_files = Vec::new();
    for object in objects {
        let Some((stem, extension)) = file_parts(object.name()) else {
            continue;
        };
        if !supported_media_extension(&extension, is_audio) {
            continue;
        }
        if restrict_to_stable_sources && object.stable_source().is_none() {
            continue;
        }
        let (source, presentation, location) = object.stable_source().unwrap_or_else(|| {
            (
                MediaSourceId::new(),
                PresentationKey::new(),
                MediaLocationId::new(),
            )
        });
        if source_ids.insert(source) {
            sources.push(MediaSourcePublicationRow::new(
                source,
                presentation,
                None,
                Some(extension),
            )?);
        }
        let (identity, kind) = object.checksum().map_or((None, None), |value| {
            (Some(value.to_owned()), Some("provider_checksum".to_owned()))
        });
        locations.push(MediaLocationPublicationRow::new(
            location,
            source,
            object.id(),
            identity,
            kind,
            0,
        )?);
        media_files.push(MediaFile {
            stem: stem.to_lowercase(),
            source,
        });
    }
    if media_files.is_empty() {
        return Err(SourceIndexError::NoMedia);
    }
    let mut subtitles = Vec::new();
    if is_audio {
        return Ok(SourceGraph {
            sources,
            locations,
            subtitles,
        });
    }
    for object in objects {
        let Some((subtitle_stem, format)) = file_parts(object.name()) else {
            continue;
        };
        if !matches!(format.as_str(), "srt" | "ass" | "ssa" | "vtt" | "sub") {
            continue;
        }
        let normalized = subtitle_stem.to_lowercase();
        let Some(video) = media_files
            .iter()
            .filter(|video| {
                normalized == video.stem || normalized.starts_with(&format!("{}.", video.stem))
            })
            .max_by_key(|video| video.stem.len())
        else {
            continue;
        };
        let tokens = normalized
            .strip_prefix(&video.stem)
            .unwrap_or_default()
            .trim_start_matches('.')
            .split('.')
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        let is_default = tokens.contains(&"default");
        let is_forced = tokens.contains(&"forced");
        let language = tokens
            .iter()
            .copied()
            .find(|token| !matches!(*token, "default" | "forced"))
            .map(str::to_owned);
        let subtitle = object
            .stable_subtitle()
            .filter(|(_, source)| *source == video.source)
            .map_or_else(SubtitleId::new, |(id, _)| id);
        subtitles.push(SubtitlePublicationRow::new(
            subtitle,
            video.source,
            object.id(),
            format,
            language,
            None,
            is_default,
            is_forced,
        )?);
    }
    Ok(SourceGraph {
        sources,
        locations,
        subtitles,
    })
}

fn supported_media_extension(extension: &str, is_audio: bool) -> bool {
    if is_audio {
        matches!(
            extension,
            "aac" | "flac" | "m4a" | "mp3" | "oga" | "ogg" | "opus" | "wav" | "wave" | "webm"
        )
    } else {
        matches!(extension, "mkv" | "mp4" | "m4v" | "webm")
    }
}

fn file_parts(name: &str) -> Option<(String, String)> {
    let path = Path::new(name);
    Some((
        path.file_stem()?.to_str()?.to_owned(),
        path.extension()?.to_str()?.to_ascii_lowercase(),
    ))
}

#[derive(Debug, Error)]
pub enum SourceIndexError {
    #[error("source-index input has no supported media object")]
    NoMedia,
    #[error("source-index input query failed: {0}")]
    Repository(#[from] SourceIndexRepositoryError),
    #[error("source-index publication failed: {0}")]
    Publication(#[from] CatalogPublicationError),
}
