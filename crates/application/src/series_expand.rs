use std::{collections::HashMap, path::Path};

use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::{
    CatalogItemId, MediaLocationId, MediaSourceId, PresentationKey, StorageObjectRecordId,
    SubtitleId,
};
use tjxy_db::{
    CatalogPublicationError, CatalogPublicationRepository, ClaimedWorkJob,
    MediaLocationPublicationRow, MediaSourcePublicationRow, SeriesExpandRepository,
    SeriesExpandRepositoryError, SeriesSourcePublication, SeriesStorageObject,
    StructurePublicationManifest, StructurePublicationRow, SubtitlePublicationRow,
    WorkJobRepository, WorkJobRepositoryError, WorkJobSpec, WorkScope, WorkTaskKind,
};
use uuid::Uuid;

const MAX_SUBTITLES_PER_EPISODE: usize = 32;

pub struct SeriesExpandService {
    database: DatabaseConnection,
}

impl SeriesExpandService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Builds and atomically publishes a Series projection from reconciled SQL inventory.
    ///
    /// # Errors
    ///
    /// Returns [`SeriesExpandError`] for incomplete trees, invalid media graphs, or publication failures.
    pub async fn execute(&self, claimed: &ClaimedWorkJob) -> Result<i64, SeriesExpandError> {
        let snapshot = SeriesExpandRepository::new(&self.database)
            .snapshot(claimed)
            .await?;
        let pending = unindexed_directories(snapshot.root_object(), snapshot.objects());
        if !pending.is_empty() {
            let jobs = WorkJobRepository::new(&self.database);
            for directory in &pending {
                jobs.enqueue_or_join(
                    &WorkJobSpec::new(
                        WorkTaskKind::ScopedStorageSync,
                        WorkScope::StorageObject(directory.id()),
                        directory.children_revision(),
                        claimed.job().priority(),
                    )?
                    .with_storage_root_affinity(snapshot.storage_root())?,
                )
                .await?;
            }
            return Err(SeriesExpandError::InventoryPending {
                scheduled: pending.len(),
            });
        }
        let graph = build_graph(
            snapshot.owner(),
            snapshot.root_object(),
            snapshot.storage_root(),
            snapshot.sync_revision(),
            snapshot.objects(),
        )?;
        let manifest = StructurePublicationManifest::from_series(&graph.rows, &graph.sources)?;
        let publications = CatalogPublicationRepository::new(&self.database);
        let publication = publications.begin_structure(claimed, &manifest).await?;
        for rows in graph.rows.chunks(5_000) {
            publications
                .stage_structure_batch(claimed, publication, rows)
                .await?;
        }
        for groups in graph.sources.chunks(500) {
            publications
                .stage_structure_source_batch(claimed, publication, groups)
                .await?;
        }
        publications.seal_structure(claimed, publication).await?;
        publications
            .publish_structure(
                &WorkJobRepository::new(&self.database),
                claimed,
                publication,
            )
            .await
            .map_err(Into::into)
    }
}

struct SeriesGraph {
    rows: Vec<StructurePublicationRow>,
    sources: Vec<SeriesSourcePublication>,
}

fn build_graph(
    owner: CatalogItemId,
    root: StorageObjectRecordId,
    storage_root: tjxy_common::StorageRootId,
    sync_revision: i64,
    objects: &[SeriesStorageObject],
) -> Result<SeriesGraph, SeriesExpandError> {
    let children = child_map(objects);
    let mut rows = Vec::new();
    let mut sources = Vec::new();
    let mut seasons = children
        .get(&root)
        .into_iter()
        .flatten()
        .filter(|object| object.object_type() == "Directory")
        .copied()
        .collect::<Vec<_>>();
    seasons.sort_unstable_by_key(|object| object.name().to_lowercase());
    for season_object in seasons {
        if !season_object.children_indexed() {
            return Err(SeriesExpandError::IncompleteTree);
        }
        let season_id = derived_item(owner, "season", season_object.id());
        rows.push(StructurePublicationRow::new(
            season_id,
            owner,
            storage_root,
            season_object.id(),
            "Season",
            season_object.name(),
            season_object.name().to_lowercase(),
            None,
            None,
        )?);
        let mut videos = descendant_videos(season_object.id(), &children);
        videos.sort_unstable_by_key(|object| object.name().to_lowercase());
        for video in videos {
            let (stem, container) =
                video_parts(video.name()).ok_or(SeriesExpandError::InvalidMedia)?;
            let episode_id = derived_item(owner, "episode", video.id());
            let episode_scope = video.parent().ok_or(SeriesExpandError::InvalidMedia)?;
            rows.push(StructurePublicationRow::new(
                episode_id,
                season_id,
                storage_root,
                episode_scope,
                "Episode",
                &stem,
                stem.to_lowercase(),
                None,
                None,
            )?);
            sources.push(episode_source(
                episode_id,
                video,
                video
                    .parent()
                    .and_then(|parent| children.get(&parent))
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                &stem,
                container,
                sync_revision,
            )?);
        }
    }
    if sources.is_empty() {
        return Err(SeriesExpandError::NoEpisodes);
    }
    Ok(SeriesGraph { rows, sources })
}

fn descendant_videos<'a>(
    root: StorageObjectRecordId,
    children: &HashMap<StorageObjectRecordId, Vec<&'a SeriesStorageObject>>,
) -> Vec<&'a SeriesStorageObject> {
    let mut videos = Vec::new();
    let mut frontier = vec![root];
    while let Some(parent) = frontier.pop() {
        for child in children.get(&parent).into_iter().flatten() {
            if child.object_type() == "Directory" {
                frontier.push(child.id());
            } else if video_parts(child.name()).is_some() {
                videos.push(*child);
            }
        }
    }
    videos
}

fn child_map(
    objects: &[SeriesStorageObject],
) -> HashMap<StorageObjectRecordId, Vec<&SeriesStorageObject>> {
    let mut children = HashMap::<StorageObjectRecordId, Vec<_>>::new();
    for object in objects {
        if let Some(parent) = object.parent() {
            children.entry(parent).or_default().push(object);
        }
    }
    children
}

fn unindexed_directories(
    root: StorageObjectRecordId,
    objects: &[SeriesStorageObject],
) -> Vec<&SeriesStorageObject> {
    let children = child_map(objects);
    let mut pending = Vec::new();
    let mut frontier = vec![root];
    while let Some(parent) = frontier.pop() {
        for child in children.get(&parent).into_iter().flatten() {
            if child.object_type() != "Directory" {
                continue;
            }
            if child.children_indexed() {
                frontier.push(child.id());
            } else {
                pending.push(*child);
            }
        }
    }
    pending.sort_unstable_by_key(|object| object.id().as_uuid());
    pending
}

fn episode_source(
    episode: CatalogItemId,
    video: &SeriesStorageObject,
    siblings: &[&SeriesStorageObject],
    stem: &str,
    container: String,
    sync_revision: i64,
) -> Result<SeriesSourcePublication, SeriesExpandError> {
    let source = MediaSourceId::from_uuid(derived_uuid(episode.as_uuid(), "source", video.id()));
    let presentation =
        PresentationKey::from_uuid(derived_uuid(episode.as_uuid(), "presentation", video.id()));
    let source_row = MediaSourcePublicationRow::new(source, presentation, None, Some(container))?;
    let (identity, kind) = video.checksum().map_or((None, None), |checksum| {
        (
            Some(checksum.to_owned()),
            Some("provider_checksum".to_owned()),
        )
    });
    let location = MediaLocationPublicationRow::new(
        MediaLocationId::from_uuid(derived_uuid(episode.as_uuid(), "location", video.id())),
        source,
        video.id(),
        identity,
        kind,
        0,
    )?;
    let normalized_stem = stem.to_lowercase();
    let mut subtitles = Vec::new();
    for sidecar in siblings {
        let Some((sidecar_stem, format)) = subtitle_parts(sidecar.name()) else {
            continue;
        };
        let normalized = sidecar_stem.to_lowercase();
        if normalized != normalized_stem && !normalized.starts_with(&format!("{normalized_stem}."))
        {
            continue;
        }
        if subtitles.len() >= MAX_SUBTITLES_PER_EPISODE {
            return Err(SeriesExpandError::InvalidMedia);
        }
        let tokens = normalized
            .strip_prefix(&normalized_stem)
            .unwrap_or_default()
            .trim_start_matches('.')
            .split('.')
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        subtitles.push(SubtitlePublicationRow::new(
            SubtitleId::from_uuid(derived_uuid(episode.as_uuid(), "subtitle", sidecar.id())),
            source,
            sidecar.id(),
            format,
            tokens
                .iter()
                .copied()
                .find(|token| !matches!(*token, "default" | "forced"))
                .map(str::to_owned),
            None,
            tokens.contains(&"default"),
            tokens.contains(&"forced"),
        )?);
    }
    SeriesSourcePublication::new(episode, vec![source_row], vec![location], subtitles)?
        .with_source_revision(sync_revision)
        .map_err(Into::into)
}

fn derived_item(owner: CatalogItemId, kind: &str, object: StorageObjectRecordId) -> CatalogItemId {
    CatalogItemId::from_uuid(derived_uuid(owner.as_uuid(), kind, object))
}

fn derived_uuid(namespace: Uuid, kind: &str, object: StorageObjectRecordId) -> Uuid {
    Uuid::new_v5(
        &namespace,
        format!("{kind}:{}", object.as_uuid()).as_bytes(),
    )
}

fn video_parts(name: &str) -> Option<(String, String)> {
    let path = Path::new(name);
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(extension.as_str(), "mkv" | "mp4" | "m4v" | "webm") {
        return None;
    }
    Some((path.file_stem()?.to_str()?.to_owned(), extension))
}

fn subtitle_parts(name: &str) -> Option<(String, String)> {
    let path = Path::new(name);
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(extension.as_str(), "srt" | "ass" | "ssa" | "vtt" | "sub") {
        return None;
    }
    Some((path.file_stem()?.to_str()?.to_owned(), extension))
}

#[derive(Debug, Error)]
pub enum SeriesExpandError {
    #[error("Series expansion scheduled {scheduled} scoped inventory jobs")]
    InventoryPending { scheduled: usize },
    #[error("Series storage tree contains unindexed directories")]
    IncompleteTree,
    #[error("Series storage tree contains no supported episodes")]
    NoEpisodes,
    #[error("Series storage tree contains an invalid media graph")]
    InvalidMedia,
    #[error("Series storage snapshot failed: {0}")]
    Repository(#[from] SeriesExpandRepositoryError),
    #[error("Series expansion work scheduling failed: {0}")]
    Work(#[from] WorkJobRepositoryError),
    #[error("Series publication failed: {0}")]
    Publication(#[from] CatalogPublicationError),
}
