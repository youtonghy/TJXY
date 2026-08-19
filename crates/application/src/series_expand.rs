use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::{
    CatalogItemId, MediaLocationId, MediaNameError, MediaSourceId, NumberRange, ParsedMediaName,
    PresentationKey, StorageObjectRecordId, SubtitleId, parse_media_name,
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
const MAX_SCAN_WARNINGS: usize = 100;

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
            &snapshot,
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
            .publish_structure_with_warnings(
                &WorkJobRepository::new(&self.database),
                claimed,
                publication,
                graph.warnings,
            )
            .await
            .map_err(Into::into)
    }
}

struct SeriesGraph {
    rows: Vec<StructurePublicationRow>,
    sources: Vec<SeriesSourcePublication>,
    warnings: Vec<String>,
}

struct SeasonGroup<'object> {
    id: CatalogItemId,
    scope: StorageObjectRecordId,
    episodes: Vec<(&'object SeriesStorageObject, ParsedMediaName, NumberRange)>,
}

fn build_graph(
    owner: CatalogItemId,
    root: StorageObjectRecordId,
    storage_root: tjxy_common::StorageRootId,
    sync_revision: i64,
    objects: &[SeriesStorageObject],
    snapshot: &tjxy_db::SeriesExpandSnapshot,
) -> Result<SeriesGraph, SeriesExpandError> {
    let children = child_map(objects);
    let by_id = objects
        .iter()
        .map(|object| (object.id(), object))
        .collect::<HashMap<_, _>>();
    let mut videos = descendant_videos(root, &children);
    videos.sort_unstable_by_key(|object| object.name().to_lowercase());
    let mut seasons = BTreeMap::<u32, SeasonGroup<'_>>::new();
    let mut warnings = Vec::new();
    for video in videos {
        let (parsed, season_scope) = parse_episode_path(video, root, &by_id)?;
        let (Some(season), Some(episode)) = (parsed.season(), parsed.episode()) else {
            if warnings.len() < MAX_SCAN_WARNINGS {
                warnings.push(format!(
                    "Skipped series video {:?}: could not determine both season and episode",
                    video.name()
                ));
            }
            continue;
        };
        let season_number = season.start();
        let scope = season_scope.unwrap_or(root);
        let season_id = snapshot
            .existing_season_id(season_number)
            .unwrap_or_else(|| derived_season_index(owner, season_number));
        seasons
            .entry(season_number)
            .or_insert_with(|| SeasonGroup {
                id: season_id,
                scope,
                episodes: Vec::new(),
            })
            .episodes
            .push((video, parsed, episode));
    }

    let mut rows = Vec::new();
    let mut sources = Vec::new();
    for (season_number, mut group) in seasons {
        group
            .episodes
            .sort_unstable_by_key(|(_, _, episode)| (episode.start(), episode.end()));
        let season_name = format!("Season {season_number}");
        rows.push(
            StructurePublicationRow::new(
                group.id,
                owner,
                storage_root,
                group.scope,
                "Season",
                &season_name,
                season_name.to_lowercase(),
                None,
                None,
            )?
            .with_index_number(Some(number_to_i32(season_number)?))?,
        );
        for (video, parsed, episode) in group.episodes {
            let episode_id = derived_item(owner, "episode", video.id());
            let episode_scope = video.parent().ok_or(SeriesExpandError::InvalidMedia)?;
            let episode_name = episode_display_name(season_number, episode);
            rows.push(
                StructurePublicationRow::new(
                    episode_id,
                    group.id,
                    storage_root,
                    episode_scope,
                    "Episode",
                    &episode_name,
                    episode_name.to_lowercase(),
                    parsed.year(),
                    None,
                )?
                .with_index_number(Some(number_to_i32(episode.start())?))?,
            );
            let (stem, container) =
                video_parts(video.name()).ok_or(SeriesExpandError::InvalidMedia)?;
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
                &parsed,
            )?);
        }
    }
    if sources.is_empty() {
        return Err(SeriesExpandError::NoEpisodes);
    }
    Ok(SeriesGraph {
        rows,
        sources,
        warnings,
    })
}

fn parse_episode_path(
    video: &SeriesStorageObject,
    root: StorageObjectRecordId,
    by_id: &HashMap<StorageObjectRecordId, &SeriesStorageObject>,
) -> Result<(ParsedMediaName, Option<StorageObjectRecordId>), SeriesExpandError> {
    let mut parsed = parse_media_name(video.name())?;
    let mut season_scope = None;
    for ancestor in ancestor_directories(video, root, by_id) {
        let context = parse_media_name(ancestor.name())?;
        if season_scope.is_none() && context.season().is_some() {
            season_scope = Some(ancestor.id());
        }
        parsed.merge_path_context(&context);
    }
    Ok((parsed, season_scope))
}

fn ancestor_directories<'object>(
    video: &'object SeriesStorageObject,
    root: StorageObjectRecordId,
    by_id: &HashMap<StorageObjectRecordId, &'object SeriesStorageObject>,
) -> Vec<&'object SeriesStorageObject> {
    let mut ancestors = Vec::new();
    let mut current = video.parent();
    while let Some(id) = current {
        let Some(object) = by_id.get(&id).copied() else {
            break;
        };
        ancestors.push(object);
        if id == root {
            break;
        }
        current = object.parent();
    }
    ancestors
}

fn episode_display_name(season: u32, episode: NumberRange) -> String {
    if episode.start() == episode.end() {
        format!("S{season:02}E{:02}", episode.start())
    } else {
        format!("S{season:02}E{:02}-E{:02}", episode.start(), episode.end())
    }
}

fn number_to_i32(number: u32) -> Result<i32, SeriesExpandError> {
    i32::try_from(number).map_err(|_| SeriesExpandError::InvalidMedia)
}

fn derived_season_index(owner: CatalogItemId, season: u32) -> CatalogItemId {
    CatalogItemId::from_uuid(Uuid::new_v5(
        &owner.as_uuid(),
        format!("season-index:{season}").as_bytes(),
    ))
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
    parsed: &ParsedMediaName,
) -> Result<SeriesSourcePublication, SeriesExpandError> {
    let source = MediaSourceId::from_uuid(derived_uuid(episode.as_uuid(), "source", video.id()));
    let presentation =
        PresentationKey::from_uuid(derived_uuid(episode.as_uuid(), "presentation", video.id()));
    let naming_hints =
        serde_json::to_value(parsed).map_err(|_| SeriesExpandError::InvalidNamingHints)?;
    let locator_kind = if container == "strm" {
        "strm"
    } else {
        "storage"
    };
    let stored_container = (container != "strm").then_some(container);
    let source_row = MediaSourcePublicationRow::new(source, presentation, None, stored_container)?
        .with_locator_kind(locator_kind)?
        .with_naming_hints(naming_hints)?;
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
    if !matches!(
        extension.as_str(),
        "mkv" | "mp4" | "m4v" | "webm" | "avi" | "mov" | "ts" | "m2ts" | "strm"
    ) {
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
    #[error("Series storage tree contains an invalid media name: {0}")]
    InvalidMediaName(#[from] MediaNameError),
    #[error("Series naming hints could not be serialized")]
    InvalidNamingHints,
    #[error("Series storage snapshot failed: {0}")]
    Repository(#[from] SeriesExpandRepositoryError),
    #[error("Series expansion work scheduling failed: {0}")]
    Work(#[from] WorkJobRepositoryError),
    #[error("Series publication failed: {0}")]
    Publication(#[from] CatalogPublicationError),
}

#[cfg(test)]
mod tests {
    use super::video_parts;

    #[test]
    fn strm_participates_in_episode_aggregation() {
        assert_eq!(
            video_parts("Dark.S01E01.strm"),
            Some(("Dark.S01E01".to_owned(), "strm".to_owned()))
        );
    }
}
