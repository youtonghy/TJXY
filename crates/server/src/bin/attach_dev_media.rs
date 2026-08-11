use std::{
    collections::{HashMap, HashSet},
    env,
    error::Error,
    path::{Path, PathBuf},
};

use chrono::Duration;
use sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use tjxy_common::{
    CatalogItemId, MediaLocationId, MediaSourceId, PresentationKey, StorageObjectRecordId,
    StorageRootId, SubtitleId,
};
use tjxy_db::{
    CatalogPublicationRepository, MediaLocationPublicationRow, MediaSourcePublicationRow,
    SourcePublicationManifest, SubtitlePublicationRow, WorkJobRepository, WorkJobSpec, WorkScope,
    WorkTaskKind, migrate_database,
};
use tjxy_storage::{IdentityQuality, ObjectType, StorageBackend, StorageObject};
use tjxy_storage_filesystem::FilesystemBackend;
use tokio::fs;
use uuid::Uuid;

const REPRESENTATIVE_ITEM_COUNT: usize = 12;
const FIXTURE_NAMESPACE: Uuid = Uuid::from_u128(0x5f9d_8664_d8c7_4f80_91c1_1dad_b5d2_229c);
const FIXTURE_VIDEO: &[u8] = include_bytes!(
    "../../tests/fixtures/jellyfin-smoke/Smoke Show/Season 01/Smoke Show S01E01.mp4"
);
const FIXTURE_VTT: &[u8] = b"WEBVTT\n\n00:00:00.000 --> 00:00:00.850\nTJXY playback fixture\n";

type AnyError = Box<dyn Error + Send + Sync>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlayableKind {
    Movie,
    Episode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlayableItem {
    id: Uuid,
    kind: PlayableKind,
    name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlannedSource {
    item_id: Uuid,
    edition: String,
    valid_video: bool,
    subtitle_languages: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RegisteredStorage {
    root_id: StorageRootId,
}

fn fixture_plan(items: &[PlayableItem]) -> Vec<PlannedSource> {
    let mut representatives = items
        .iter()
        .filter(|item| item.kind == PlayableKind::Movie)
        .take(REPRESENTATIVE_ITEM_COUNT / 2)
        .map(|item| item.id)
        .chain(
            items
                .iter()
                .filter(|item| item.kind == PlayableKind::Episode)
                .take(REPRESENTATIVE_ITEM_COUNT / 2)
                .map(|item| item.id),
        )
        .collect::<HashSet<_>>();
    for item in items {
        if representatives.len() == REPRESENTATIVE_ITEM_COUNT {
            break;
        }
        representatives.insert(item.id);
    }

    items
        .iter()
        .flat_map(|item| {
            let representative = representatives.contains(&item.id);
            let subtitles = representative.then(|| vec!["zh-CN".to_owned(), "en".to_owned()]);
            let mut sources = vec![PlannedSource {
                item_id: item.id,
                edition: "1080p".to_owned(),
                valid_video: true,
                subtitle_languages: subtitles.clone().unwrap_or_default(),
            }];
            if representative {
                sources.push(PlannedSource {
                    item_id: item.id,
                    edition: "720p".to_owned(),
                    valid_video: true,
                    subtitle_languages: subtitles.unwrap_or_default(),
                });
                sources.push(PlannedSource {
                    item_id: item.id,
                    edition: "Damaged".to_owned(),
                    valid_video: false,
                    subtitle_languages: Vec::new(),
                });
            }
            sources
        })
        .collect()
}

fn stable_uuid(key: &str) -> Uuid {
    Uuid::new_v5(&FIXTURE_NAMESPACE, key.as_bytes())
}

fn source_filename(source: &PlannedSource) -> String {
    format!(
        "{}-{}.mp4",
        source.item_id,
        source.edition.to_ascii_lowercase()
    )
}

fn subtitle_filename(source: &PlannedSource, language: &str) -> String {
    format!(
        "{}-{}-{language}.vtt",
        source.item_id,
        source.edition.to_ascii_lowercase()
    )
}

async fn playable_items(database: &DatabaseConnection) -> Result<Vec<PlayableItem>, AnyError> {
    let backend = database.get_database_backend();
    let rows = database
        .query_all(
            backend.build(
                Query::select()
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                    ])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("item_type")).is_in(["Movie", "Episode"]))
                    .and_where(Expr::col(Alias::new("is_present")).eq(true))
                    .order_by(Alias::new("item_type"), sea_orm::sea_query::Order::Asc)
                    .order_by(Alias::new("name"), sea_orm::sea_query::Order::Asc)
                    .order_by(Alias::new("id"), sea_orm::sea_query::Order::Asc),
            ),
        )
        .await?;
    rows.into_iter()
        .map(|row| {
            let kind = match row.try_get::<String>("", "item_type")?.as_str() {
                "Movie" => PlayableKind::Movie,
                "Episode" => PlayableKind::Episode,
                _ => return Err("query returned a non-playable catalog item".into()),
            };
            Ok(PlayableItem {
                id: row.try_get("", "id")?,
                kind,
                name: row.try_get("", "name")?,
            })
        })
        .collect()
}

async fn write_fixture_files(root: &Path, plan: &[PlannedSource]) -> Result<(), AnyError> {
    fs::create_dir_all(root).await?;
    for source in plan {
        let video_path = root.join(source_filename(source));
        if !fs::try_exists(&video_path).await? {
            fs::write(
                video_path,
                if source.valid_video {
                    FIXTURE_VIDEO
                } else {
                    &[]
                },
            )
            .await?;
        }
        for language in &source.subtitle_languages {
            let subtitle_path = root.join(subtitle_filename(source, language));
            if !fs::try_exists(&subtitle_path).await? {
                fs::write(subtitle_path, FIXTURE_VTT).await?;
            }
        }
    }
    Ok(())
}

fn storage_record_id(provider_object_id: &str) -> StorageObjectRecordId {
    StorageObjectRecordId::from_uuid(stable_uuid(&format!("object:{provider_object_id}")))
}

fn identity_quality(value: IdentityQuality) -> &'static str {
    match value {
        IdentityQuality::StableFileId => "StableFileId",
        IdentityQuality::PathWeak => "PathWeak",
        IdentityQuality::ProviderStableId => "ProviderStableId",
    }
}

fn object_type(value: ObjectType) -> &'static str {
    match value {
        ObjectType::File => "File",
        ObjectType::Directory => "Directory",
    }
}

#[allow(clippy::too_many_lines)]
async fn register_storage(
    database: &DatabaseConnection,
    root_path: &Path,
    backend_root: StorageObject,
    objects: &[StorageObject],
) -> Result<RegisteredStorage, AnyError> {
    let account_id = stable_uuid("storage-account");
    let root_id = StorageRootId::from_uuid(stable_uuid("storage-root"));
    let root_record_id = storage_record_id(backend_root.id().provider_object_id());
    let transaction = database.begin().await?;
    let backend = transaction.get_database_backend();
    transaction
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_accounts"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("provider"),
                        Alias::new("display_name"),
                        Alias::new("account_identity"),
                        Alias::new("credential_ref"),
                        Alias::new("status"),
                    ])
                    .values_panic([
                        account_id.into(),
                        "filesystem".into(),
                        "TJXY Media Fixtures".into(),
                        "tjxy-media-fixtures".into(),
                        format!("filesystem-config:{account_id}").into(),
                        "Active".into(),
                    ])
                    .on_conflict(
                        OnConflict::column(Alias::new("id"))
                            .update_columns([
                                Alias::new("display_name"),
                                Alias::new("credential_ref"),
                                Alias::new("status"),
                            ])
                            .to_owned(),
                    ),
            ),
        )
        .await?;
    transaction
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("filesystem_storage_configs"))
                    .columns([Alias::new("storage_account_id"), Alias::new("root_path")])
                    .values_panic([
                        account_id.into(),
                        root_path.to_string_lossy().into_owned().into(),
                    ])
                    .on_conflict(
                        OnConflict::column(Alias::new("storage_account_id"))
                            .update_column(Alias::new("root_path"))
                            .to_owned(),
                    ),
            ),
        )
        .await?;
    transaction
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("storage_roots"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_account_id"),
                        Alias::new("provider_root_id"),
                        Alias::new("sync_revision"),
                        Alias::new("reconciled_sync_revision"),
                    ])
                    .values_panic([
                        root_id.as_uuid().into(),
                        account_id.into(),
                        backend_root.id().provider_object_id().into(),
                        1_i64.into(),
                        1_i64.into(),
                    ])
                    .on_conflict(
                        OnConflict::column(Alias::new("id"))
                            .update_columns([
                                Alias::new("provider_root_id"),
                                Alias::new("sync_revision"),
                                Alias::new("reconciled_sync_revision"),
                            ])
                            .to_owned(),
                    ),
            ),
        )
        .await?;
    for library_name in ["Movies", "TV Shows"] {
        let library_id: Uuid = transaction
            .query_one(
                backend.build(
                    Query::select()
                        .column(Alias::new("id"))
                        .from(Alias::new("libraries"))
                        .and_where(Expr::col(Alias::new("name")).eq(library_name))
                        .limit(1),
                ),
            )
            .await?
            .ok_or_else(|| format!("library {library_name} is missing"))?
            .try_get("", "id")?;
        transaction
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("library_storage_roots"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("library_id"),
                            Alias::new("storage_root_id"),
                        ])
                        .values_panic([
                            stable_uuid(&format!("library-root:{library_id}")).into(),
                            library_id.into(),
                            root_id.as_uuid().into(),
                        ])
                        .on_conflict(
                            OnConflict::columns([
                                Alias::new("library_id"),
                                Alias::new("storage_root_id"),
                            ])
                            .do_nothing()
                            .to_owned(),
                        ),
                ),
            )
            .await?;
    }
    for object in std::iter::once(&backend_root).chain(objects) {
        let record_id = storage_record_id(object.id().provider_object_id());
        let mime_type = match Path::new(object.name())
            .extension()
            .and_then(|extension| extension.to_str())
        {
            Some("mp4") => Some("video/mp4"),
            Some("vtt") => Some("text/vtt"),
            _ => None,
        };
        transaction
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_drive_id"),
                            Alias::new("provider_object_id"),
                            Alias::new("name"),
                            Alias::new("normalized_name"),
                            Alias::new("object_type"),
                            Alias::new("size"),
                            Alias::new("remote_revision"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("facts_observed_storage_root_id"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("identity_quality"),
                            Alias::new("presence_state"),
                            Alias::new("mime_type"),
                        ])
                        .values_panic([
                            record_id.as_uuid().into(),
                            account_id.into(),
                            "local".into(),
                            object.id().provider_object_id().into(),
                            object.name().into(),
                            object.name().to_lowercase().into(),
                            object_type(object.object_type()).into(),
                            object
                                .size()
                                .and_then(|size| i64::try_from(size).ok())
                                .into(),
                            object.remote_revision().into(),
                            1_i64.into(),
                            root_id.as_uuid().into(),
                            (object.object_type() == ObjectType::Directory).into(),
                            1_i64.into(),
                            identity_quality(object.identity_quality()).into(),
                            "Present".into(),
                            mime_type.into(),
                        ])
                        .on_conflict(
                            OnConflict::column(Alias::new("id"))
                                .update_columns([
                                    Alias::new("name"),
                                    Alias::new("normalized_name"),
                                    Alias::new("size"),
                                    Alias::new("remote_revision"),
                                    Alias::new("observed_sync_revision"),
                                    Alias::new("facts_observed_storage_root_id"),
                                    Alias::new("presence_state"),
                                    Alias::new("mime_type"),
                                ])
                                .to_owned(),
                        ),
                ),
            )
            .await?;
        transaction
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_root_objects"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_root_id"),
                            Alias::new("storage_object_id"),
                            Alias::new("parent_storage_object_id"),
                            Alias::new("observed_sync_revision"),
                            Alias::new("children_indexed"),
                            Alias::new("children_index_revision"),
                            Alias::new("presence_state"),
                        ])
                        .values_panic([
                            stable_uuid(&format!("root-object:{}", record_id.as_uuid())).into(),
                            root_id.as_uuid().into(),
                            record_id.as_uuid().into(),
                            (record_id != root_record_id)
                                .then_some(root_record_id.as_uuid())
                                .into(),
                            1_i64.into(),
                            (object.object_type() == ObjectType::Directory).into(),
                            1_i64.into(),
                            "Present".into(),
                        ])
                        .on_conflict(
                            OnConflict::columns([
                                Alias::new("storage_root_id"),
                                Alias::new("storage_object_id"),
                            ])
                            .update_columns([
                                Alias::new("parent_storage_object_id"),
                                Alias::new("observed_sync_revision"),
                                Alias::new("presence_state"),
                            ])
                            .to_owned(),
                        ),
                ),
            )
            .await?;
    }
    transaction.commit().await?;
    Ok(RegisteredStorage { root_id })
}

type SourcePublicationRows = (
    Vec<MediaSourcePublicationRow>,
    Vec<MediaLocationPublicationRow>,
    Vec<SubtitlePublicationRow>,
);

fn rows_for_item(
    item: &PlayableItem,
    sources: &[PlannedSource],
    objects: &HashMap<String, StorageObjectRecordId>,
) -> Result<SourcePublicationRows, AnyError> {
    let mut source_rows = Vec::with_capacity(sources.len());
    let mut location_rows = Vec::with_capacity(sources.len());
    let mut subtitle_rows = Vec::new();
    for source in sources {
        let source_id = MediaSourceId::from_uuid(stable_uuid(&format!(
            "source:{}:{}",
            item.id, source.edition
        )));
        let presentation_key = PresentationKey::from_uuid(stable_uuid(&format!(
            "presentation:{}:{}",
            item.id, source.edition
        )));
        let video_name = source_filename(source);
        let video_object = *objects
            .get(&video_name)
            .ok_or_else(|| format!("storage object for {video_name} is missing"))?;
        source_rows.push(MediaSourcePublicationRow::new(
            source_id,
            presentation_key,
            Some(source.edition.clone()),
            Some("mp4".to_owned()),
        )?);
        location_rows.push(MediaLocationPublicationRow::new(
            MediaLocationId::from_uuid(stable_uuid(&format!(
                "location:{}:{}",
                item.id, source.edition
            ))),
            source_id,
            video_object,
            None,
            None,
            if source.valid_video { 100 } else { -100 },
        )?);
        for (index, language) in source.subtitle_languages.iter().enumerate() {
            let subtitle_name = subtitle_filename(source, language);
            let subtitle_object = *objects
                .get(&subtitle_name)
                .ok_or_else(|| format!("storage object for {subtitle_name} is missing"))?;
            subtitle_rows.push(SubtitlePublicationRow::new(
                SubtitleId::from_uuid(stable_uuid(&format!(
                    "subtitle:{}:{}:{language}",
                    item.id, source.edition
                ))),
                source_id,
                subtitle_object,
                "vtt",
                Some(language.clone()),
                Some(i32::try_from(index)?),
                index == 0,
                false,
            )?);
        }
    }
    Ok((source_rows, location_rows, subtitle_rows))
}

async fn publish_item_sources(
    database: &DatabaseConnection,
    storage: RegisteredStorage,
    item: &PlayableItem,
    planned: &[PlannedSource],
    objects: &HashMap<String, StorageObjectRecordId>,
) -> Result<(), AnyError> {
    let backend = database.get_database_backend();
    let revision: i64 = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("source_index_revision"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item.id))
                    .limit(1),
            ),
        )
        .await?
        .ok_or("catalog item disappeared during fixture publication")?
        .try_get("", "source_index_revision")?;
    let owner = CatalogItemId::from_uuid(item.id);
    let jobs = WorkJobRepository::new(database);
    jobs.enqueue_or_join(
        &WorkJobSpec::new(
            WorkTaskKind::IndexMediaSources,
            WorkScope::CatalogItem(owner),
            revision,
            10_000,
        )?
        .with_input_sync_revision(1)?
        .with_storage_root_affinity(storage.root_id)?,
    )
    .await?;
    let claimed = jobs
        .claim_next(
            &[WorkTaskKind::IndexMediaSources],
            "development-media-fixture",
            Duration::minutes(30),
        )
        .await?
        .ok_or("fixture source-index job could not be claimed")?;
    if claimed.job().scope() != WorkScope::CatalogItem(owner) {
        return Err("an unrelated source-index job was claimed".into());
    }
    let (source_rows, location_rows, subtitle_rows) = rows_for_item(item, planned, objects)?;
    let manifest =
        SourcePublicationManifest::from_rows(&source_rows, &location_rows, &subtitle_rows)?;
    let publications = CatalogPublicationRepository::new(database);
    let publication = publications.begin_sources(&claimed, &manifest).await?;
    publications
        .stage_source_batch(
            &claimed,
            publication,
            &source_rows,
            &location_rows,
            &subtitle_rows,
        )
        .await?;
    publications.seal_sources(&claimed, publication).await?;
    publications
        .publish_sources(&jobs, &claimed, publication)
        .await?;
    apply_source_facts(database, item, planned).await
}

#[allow(clippy::too_many_lines)]
async fn apply_source_facts(
    database: &DatabaseConnection,
    item: &PlayableItem,
    planned: &[PlannedSource],
) -> Result<(), AnyError> {
    let transaction = database.begin().await?;
    let backend = transaction.get_database_backend();
    for source in planned {
        let source_id = stable_uuid(&format!("source:{}:{}", item.id, source.edition));
        let (resolution, bitrate, priority, dimensions) = match source.edition.as_str() {
            "1080p" => (Some("1920x1080"), Some(2_000_000_i64), 100, (1920, 1080)),
            "720p" => (Some("1280x720"), Some(1_000_000_i64), 50, (1280, 720)),
            _ => (Some("640x360"), Some(250_000_i64), -100, (640, 360)),
        };
        transaction
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("media_sources"))
                        .values([
                            (Alias::new("probe_state"), "Probed".into()),
                            (Alias::new("video_codec"), "h264".into()),
                            (Alias::new("resolution"), resolution.into()),
                            (Alias::new("bitrate"), bitrate.into()),
                            (Alias::new("runtime_ticks"), 10_000_000_i64.into()),
                            (Alias::new("admin_priority"), priority.into()),
                            (Alias::new("is_default"), (source.edition == "1080p").into()),
                            (Alias::new("is_hidden"), false.into()),
                            (
                                Alias::new("last_probe_error"),
                                Option::<String>::None.into(),
                            ),
                        ])
                        .and_where(Expr::col(Alias::new("id")).eq(source_id)),
                ),
            )
            .await?;
        transaction
            .execute(
                backend.build(
                    Query::delete()
                        .from_table(Alias::new("media_streams"))
                        .and_where(Expr::col(Alias::new("media_source_id")).eq(source_id)),
                ),
            )
            .await?;
        for (stream_type, stream_index, codec, width, height, channels) in [
            (
                "Video",
                0,
                "h264",
                Some(dimensions.0),
                Some(dimensions.1),
                None,
            ),
            ("Audio", 1, "aac", None, None, Some(1)),
        ] {
            transaction
                .execute(
                    backend.build(
                        Query::insert()
                            .into_table(Alias::new("media_streams"))
                            .columns([
                                Alias::new("id"),
                                Alias::new("media_source_id"),
                                Alias::new("stream_type"),
                                Alias::new("stream_index"),
                                Alias::new("codec"),
                                Alias::new("language"),
                                Alias::new("stream_identity"),
                                Alias::new("delivery_index"),
                                Alias::new("container_stream_index"),
                                Alias::new("width"),
                                Alias::new("height"),
                                Alias::new("channels"),
                                Alias::new("is_default"),
                                Alias::new("is_forced"),
                                Alias::new("is_external"),
                                Alias::new("is_text"),
                            ])
                            .values_panic([
                                stable_uuid(&format!(
                                    "stream:{source_id}:{stream_type}:{stream_index}"
                                ))
                                .into(),
                                source_id.into(),
                                stream_type.into(),
                                stream_index.into(),
                                codec.into(),
                                (stream_type == "Audio").then_some("en").into(),
                                format!("{stream_type}:{stream_index}").into(),
                                stream_index.into(),
                                stream_index.into(),
                                width.into(),
                                height.into(),
                                channels.into(),
                                true.into(),
                                false.into(),
                                false.into(),
                                false.into(),
                            ]),
                    ),
                )
                .await?;
        }
    }
    transaction.commit().await?;
    Ok(())
}

async fn run() -> Result<(), AnyError> {
    let database_url = env::var("TJXY_DATABASE_URL")?;
    let root = PathBuf::from(env::var("TJXY_DEV_MEDIA_ROOT")?);
    let database = Database::connect(database_url).await?;
    migrate_database(&database).await?;
    let items = playable_items(&database).await?;
    if items.is_empty() {
        return Err("no Movies or Episodes exist; import metadata first".into());
    }
    let plan = fixture_plan(&items);
    write_fixture_files(&root, &plan).await?;
    let filesystem = FilesystemBackend::new(&root).await?;
    let backend_root = StorageObject::directory_with_identity(
        filesystem.root_id().clone(),
        "TJXY Media Fixtures",
        IdentityQuality::StableFileId,
    );
    let page = filesystem.list_children(filesystem.root_id(), None).await?;
    let storage = register_storage(&database, &root, backend_root, &page.objects).await?;
    let objects = page
        .objects
        .iter()
        .map(|object| {
            (
                object.name().to_owned(),
                storage_record_id(object.id().provider_object_id()),
            )
        })
        .collect::<HashMap<_, _>>();
    let grouped = plan.iter().fold(
        HashMap::<Uuid, Vec<PlannedSource>>::new(),
        |mut result, source| {
            result
                .entry(source.item_id)
                .or_default()
                .push(source.clone());
            result
        },
    );
    for (index, item) in items.iter().enumerate() {
        let sources = grouped
            .get(&item.id)
            .ok_or("fixture plan omitted a playable item")?;
        publish_item_sources(&database, storage, item, sources, &objects).await?;
        if (index + 1) % 100 == 0 || index + 1 == items.len() {
            println!(
                "Published playback fixtures for {}/{} items.",
                index + 1,
                items.len()
            );
        }
    }
    println!(
        "Attached {} media sources to {} playable items at {}.",
        plan.len(),
        items.len(),
        root.display()
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), AnyError> {
    run().await
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use sea_orm::{
        ConnectionTrait,
        sea_query::{Alias, Expr, Query},
    };
    use sea_orm_migration::MigratorTrait;
    use tjxy_common::SortKey;
    use tjxy_db::{Migrator, PublishedMediaSource};
    use tjxy_storage::{IdentityQuality, StorageObject, StorageObjectId};
    use tjxy_test_support::test_database;
    use uuid::Uuid;

    use super::{
        CatalogItemId, CatalogPublicationRepository, PlayableItem, PlayableKind,
        REPRESENTATIVE_ITEM_COUNT, fixture_plan, publish_item_sources, register_storage,
        source_filename, storage_record_id, subtitle_filename,
    };

    fn items() -> Vec<PlayableItem> {
        (0..16)
            .map(|index| PlayableItem {
                id: Uuid::from_u128(index + 1),
                kind: if index < 8 {
                    PlayableKind::Movie
                } else {
                    PlayableKind::Episode
                },
                name: format!("Title {index}"),
            })
            .collect()
    }

    #[test]
    fn plan_gives_every_item_a_valid_source_and_only_twelve_representatives_extra_tracks() {
        let items = items();
        let plan = fixture_plan(&items);
        let by_item = plan.iter().fold(
            HashMap::<Uuid, Vec<&super::PlannedSource>>::new(),
            |mut grouped, source| {
                grouped.entry(source.item_id).or_default().push(source);
                grouped
            },
        );

        assert_eq!(by_item.len(), items.len());
        assert!(
            by_item
                .values()
                .all(|sources| sources.iter().any(|source| source.valid_video))
        );
        let representatives = by_item
            .values()
            .filter(|sources| sources.len() == 3)
            .collect::<Vec<_>>();
        assert_eq!(representatives.len(), REPRESENTATIVE_ITEM_COUNT);
        for sources in representatives {
            assert_eq!(
                sources
                    .iter()
                    .map(|source| source.edition.as_str())
                    .collect::<HashSet<_>>(),
                HashSet::from(["1080p", "720p", "Damaged"])
            );
            assert!(sources.iter().any(|source| !source.valid_video));
            assert!(
                sources
                    .iter()
                    .filter(|source| source.valid_video)
                    .all(|source| source.subtitle_languages == ["zh-CN", "en"])
            );
        }
        assert!(
            plan.iter()
                .all(|source| !source.edition.to_ascii_lowercase().contains("demo"))
        );
    }

    async fn count(database: &sea_orm::DatabaseConnection, table: &str) -> i64 {
        let backend = database.get_database_backend();
        database
            .query_one(
                backend.build(
                    Query::select()
                        .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
                        .from(Alias::new(table)),
                ),
            )
            .await
            .unwrap()
            .unwrap()
            .try_get("", "count")
            .unwrap()
    }

    async fn seed_libraries(database: &sea_orm::DatabaseConnection) -> Uuid {
        let backend = database.get_database_backend();
        let mut movies = None;
        for (name, collection_type) in [("Movies", "movies"), ("TV Shows", "tvshows")] {
            let id = Uuid::new_v4();
            if name == "Movies" {
                movies = Some(id);
            }
            database
                .execute(
                    backend.build(
                        Query::insert()
                            .into_table(Alias::new("libraries"))
                            .columns([
                                Alias::new("id"),
                                Alias::new("name"),
                                Alias::new("scan_profile"),
                                Alias::new("object_selection_scope"),
                                Alias::new("metadata_policy"),
                                Alias::new("expansion_policy"),
                                Alias::new("probe_policy"),
                                Alias::new("profile_version"),
                                Alias::new("collection_type"),
                                Alias::new("sort_key"),
                                Alias::new("is_enabled"),
                            ])
                            .values_panic([
                                id.into(),
                                name.into(),
                                "Manual".into(),
                                "library_roots".into(),
                                "none".into(),
                                "manual".into(),
                                "on_playback".into(),
                                1_i32.into(),
                                collection_type.into(),
                                SortKey::from_text(name).into_bytes().into(),
                                true.into(),
                            ]),
                    ),
                )
                .await
                .unwrap();
        }
        movies.unwrap()
    }

    #[tokio::test]
    async fn storage_registration_is_idempotent_across_both_real_libraries() {
        let database = test_database().await.unwrap();
        Migrator::up(&database, None).await.unwrap();
        seed_libraries(&database).await;
        let root = tempfile::tempdir().unwrap();
        let root_object = StorageObject::directory_with_identity(
            StorageObjectId::new("filesystem", "root/root").unwrap(),
            "Fixtures",
            IdentityQuality::StableFileId,
        );
        let video = StorageObject::file_with_identity(
            StorageObjectId::new("filesystem", "root/video").unwrap(),
            "video.mp4",
            2_532,
            IdentityQuality::StableFileId,
        );

        let first = register_storage(
            &database,
            root.path(),
            root_object.clone(),
            std::slice::from_ref(&video),
        )
        .await
        .unwrap();
        let second = register_storage(&database, root.path(), root_object, &[video])
            .await
            .unwrap();

        assert_eq!(second, first);
        assert_eq!(count(&database, "storage_accounts").await, 1);
        assert_eq!(count(&database, "storage_roots").await, 1);
        assert_eq!(count(&database, "storage_objects").await, 2);
        assert_eq!(count(&database, "storage_root_objects").await, 2);
        assert_eq!(count(&database, "library_storage_roots").await, 2);
    }

    async fn seed_movie(database: &sea_orm::DatabaseConnection, library_id: Uuid) -> PlayableItem {
        let item = PlayableItem {
            id: Uuid::new_v4(),
            kind: PlayableKind::Movie,
            name: "Fixture Movie".to_owned(),
        };
        let backend = database.get_database_backend();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("catalog_items"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("item_type"),
                            Alias::new("name"),
                            Alias::new("sort_name"),
                            Alias::new("classification_state"),
                            Alias::new("metadata_state"),
                            Alias::new("structure_state"),
                            Alias::new("source_state"),
                            Alias::new("structure_expansion_revision"),
                            Alias::new("source_index_revision"),
                            Alias::new("is_present"),
                        ])
                        .values_panic([
                            item.id.into(),
                            "Movie".into(),
                            item.name.as_str().into(),
                            "fixture movie".into(),
                            "Matched".into(),
                            "Ready".into(),
                            "NotApplicable".into(),
                            "Missing".into(),
                            0_i64.into(),
                            0_i64.into(),
                            true.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("library_catalog_items"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("library_id"),
                            Alias::new("catalog_item_id"),
                        ])
                        .values_panic([Uuid::new_v4().into(), library_id.into(), item.id.into()]),
                ),
            )
            .await
            .unwrap();
        item
    }

    #[tokio::test]
    async fn fixture_sources_publish_through_the_real_authorized_projection() {
        let database = test_database().await.unwrap();
        Migrator::up(&database, None).await.unwrap();
        let movies = seed_libraries(&database).await;
        let item = seed_movie(&database, movies).await;
        let plan = fixture_plan(std::slice::from_ref(&item));
        let root = tempfile::tempdir().unwrap();
        let root_object = StorageObject::directory_with_identity(
            StorageObjectId::new("filesystem", "root/root").unwrap(),
            "Fixtures",
            IdentityQuality::StableFileId,
        );
        let mut storage_objects = Vec::new();
        for source in &plan {
            storage_objects.push(StorageObject::file_with_identity(
                StorageObjectId::new("filesystem", source_filename(source)).unwrap(),
                source_filename(source),
                if source.valid_video { 2_532 } else { 0 },
                IdentityQuality::StableFileId,
            ));
            for language in &source.subtitle_languages {
                let name = subtitle_filename(source, language);
                storage_objects.push(StorageObject::file_with_identity(
                    StorageObjectId::new("filesystem", name.clone()).unwrap(),
                    name,
                    64,
                    IdentityQuality::StableFileId,
                ));
            }
        }
        let storage = register_storage(&database, root.path(), root_object, &storage_objects)
            .await
            .unwrap();
        let objects = storage_objects
            .iter()
            .map(|object| {
                (
                    object.name().to_owned(),
                    storage_record_id(object.id().provider_object_id()),
                )
            })
            .collect::<HashMap<_, _>>();

        publish_item_sources(&database, storage, &item, &plan, &objects)
            .await
            .unwrap();

        let sources = CatalogPublicationRepository::new(&database)
            .active_sources(CatalogItemId::from_uuid(item.id))
            .await
            .unwrap();
        assert_eq!(sources.len(), 3);
        assert!(
            sources
                .iter()
                .all(|source| source.probe_state() == "Probed")
        );
        assert!(sources.iter().all(|source| source.streams().len() == 2));
        let damaged = sources
            .iter()
            .find(|source| source.edition() == Some("Damaged"))
            .expect("the intentionally invalid source must remain selectable");
        assert_eq!(damaged.locations()[0].priority(), -100);
        assert_eq!(
            sources
                .iter()
                .map(|source| source.subtitles().len())
                .sum::<usize>(),
            4
        );
        assert!(sources.iter().any(PublishedMediaSource::is_default));
    }
}
