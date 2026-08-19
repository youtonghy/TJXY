use std::{fs, time::Duration};

use futures_util::{StreamExt, TryStreamExt};
use tempfile::tempdir;
use tjxy_storage::{ByteRange, IdentityQuality, StorageBackend};
use tjxy_storage_filesystem::FilesystemBackend;

#[tokio::test]
async fn lists_objects_without_classifying_or_opening_media() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("movie.mkv"), b"abcdefgh").unwrap();
    fs::create_dir(root.path().join("Series")).unwrap();
    let backend = FilesystemBackend::new(root.path()).await.unwrap();

    let page = backend
        .list_children(backend.root_id(), None)
        .await
        .unwrap();

    assert_eq!(page.objects.len(), 2);
    let movie = page
        .objects
        .iter()
        .find(|object| object.name() == "movie.mkv")
        .unwrap();
    assert_eq!(movie.size(), Some(8));
    assert!(matches!(
        movie.identity_quality(),
        IdentityQuality::StableFileId | IdentityQuality::PathWeak
    ));
    assert!(movie.remote_revision().is_some());
}

#[tokio::test]
async fn content_stat_changes_revision_without_replacing_stable_identity() {
    let root = tempdir().unwrap();
    let path = root.path().join("movie.mkv");
    fs::write(&path, b"first").unwrap();
    let backend = FilesystemBackend::new(root.path()).await.unwrap();
    let first = backend
        .list_children(backend.root_id(), None)
        .await
        .unwrap()
        .objects
        .into_iter()
        .next()
        .unwrap();
    fs::write(&path, b"second version").unwrap();
    let second = backend
        .list_children(backend.root_id(), None)
        .await
        .unwrap()
        .objects
        .into_iter()
        .next()
        .unwrap();

    assert_eq!(first.id(), second.id());
    assert_ne!(first.remote_revision(), second.remote_revision());
}

#[cfg(unix)]
#[tokio::test]
async fn rename_keeps_stable_identity_and_updates_the_indexed_path() {
    let root = tempdir().unwrap();
    let old_path = root.path().join("old.mkv");
    let new_path = root.path().join("new.mkv");
    fs::write(&old_path, b"movie").unwrap();
    let backend = FilesystemBackend::new(root.path()).await.unwrap();
    let before = backend
        .list_children(backend.root_id(), None)
        .await
        .unwrap()
        .objects
        .into_iter()
        .next()
        .unwrap();

    fs::rename(old_path, new_path).unwrap();
    let after = backend
        .list_children(backend.root_id(), None)
        .await
        .unwrap()
        .objects
        .into_iter()
        .next()
        .unwrap();

    assert_eq!(before.id(), after.id());
    assert_eq!(after.name(), "new.mkv");
}

#[tokio::test]
#[cfg_attr(
    target_os = "macos",
    ignore = "requires the host FSEvents service to admit a new stream"
)]
async fn native_event_monitor_observes_a_repeated_heartbeat() {
    let root = tempdir().unwrap();
    let backend = FilesystemBackend::new(root.path()).await.unwrap();
    let mut monitor = backend
        .watch_events(Duration::from_millis(50))
        .expect("filesystem watcher should start");
    let root_path = root.path().to_owned();
    let writer = tokio::spawn(async move {
        for sequence in 0..100 {
            fs::write(
                root_path.join(format!("watcher-heartbeat-{sequence}.tmp")),
                b"heartbeat",
            )
            .unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    let batch = tokio::time::timeout(Duration::from_secs(15), monitor.next_batch())
        .await
        .expect("filesystem event batch timed out")
        .expect("filesystem event batch failed");
    writer.abort();
    let scopes = backend.inventory_scopes_for(&batch).await.unwrap();

    assert_eq!(scopes, vec![backend.root_id().clone()]);
    assert!(backend.capabilities().file_events());
    assert!(!backend.capabilities().changes());
}

#[tokio::test]
async fn stable_identity_recovers_its_path_after_backend_restart() {
    let root = tempdir().unwrap();
    tokio::fs::create_dir(root.path().join("nested"))
        .await
        .unwrap();
    tokio::fs::write(root.path().join("nested/movie.mkv"), b"restart-safe")
        .await
        .unwrap();
    let first = FilesystemBackend::new(root.path()).await.unwrap();
    let folders = first.list_children(first.root_id(), None).await.unwrap();
    let file_parent = folders.objects[0].id().clone();
    let files = first.list_children(&file_parent, None).await.unwrap();
    let stable_id = files.objects[0].id().clone();
    drop(first);

    let restarted = FilesystemBackend::new(root.path()).await.unwrap();
    let stream = restarted
        .open_range(&stable_id, ByteRange::new(0, 12).unwrap())
        .await
        .unwrap();
    let bytes = stream.try_collect::<Vec<_>>().await.unwrap().concat();
    assert_eq!(bytes, b"restart-safe");
}

#[cfg(unix)]
#[tokio::test]
async fn indexed_file_cannot_be_replaced_with_an_outside_symlink_before_open() {
    use std::os::unix::fs::symlink;

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(root.path().join("movie.mkv"), b"inside").unwrap();
    fs::write(outside.path().join("secret.mkv"), b"outside-secret").unwrap();
    let backend = FilesystemBackend::new(root.path()).await.unwrap();
    let page = backend
        .list_children(backend.root_id(), None)
        .await
        .unwrap();
    let stable_id = page.objects[0].id().clone();
    fs::remove_file(root.path().join("movie.mkv")).unwrap();
    symlink(
        outside.path().join("secret.mkv"),
        root.path().join("movie.mkv"),
    )
    .unwrap();

    let Err(error) = backend
        .open_range(&stable_id, ByteRange::new(0, 6).unwrap())
        .await
    else {
        panic!("outside symlink was opened");
    };
    assert!(matches!(
        error,
        tjxy_storage::BackendError::TemporarilyUnavailable { .. }
            | tjxy_storage::BackendError::NotFound
    ));
}

#[tokio::test]
async fn streams_only_the_requested_half_open_range() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("movie.mkv"), b"abcdefgh").unwrap();
    let backend = FilesystemBackend::new(root.path()).await.unwrap();
    let page = backend
        .list_children(backend.root_id(), None)
        .await
        .unwrap();
    let movie = page.objects.first().unwrap();

    let chunks = backend
        .open_range(movie.id(), ByteRange::new(2, 7).unwrap())
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;
    let bytes = chunks
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .concat();

    assert_eq!(bytes, b"cdefg");
}

#[tokio::test]
async fn resolves_relative_and_absolute_local_references_inside_the_root() {
    let root = tempdir().unwrap();
    fs::create_dir(root.path().join("show")).unwrap();
    fs::write(root.path().join("show/episode.strm"), b"episode.mkv").unwrap();
    fs::write(root.path().join("show/episode.mkv"), b"abcdefgh").unwrap();
    let backend = FilesystemBackend::new(root.path()).await.unwrap();
    let folders = backend
        .list_children(backend.root_id(), None)
        .await
        .unwrap();
    let show = folders.objects.first().unwrap();
    let files = backend.list_children(show.id(), None).await.unwrap();
    let descriptor = files
        .objects
        .iter()
        .find(|object| object.name() == "episode.strm")
        .unwrap();

    let relative = backend
        .resolve_local_reference(descriptor.id(), "episode.mkv")
        .await
        .unwrap();
    let absolute = backend
        .resolve_local_reference(
            descriptor.id(),
            fs::canonicalize(root.path())
                .unwrap()
                .join("show/episode.mkv")
                .to_str()
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = backend
        .open_range(relative.id(), ByteRange::new(2, 6).unwrap())
        .await
        .unwrap()
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .concat();

    assert_eq!(relative.id(), absolute.id());
    assert_eq!(bytes, b"cdef");
}

#[cfg(unix)]
#[tokio::test]
async fn local_references_reject_dot_components_and_symlink_targets() {
    use std::os::unix::fs::symlink;

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(root.path().join("episode.strm"), b"escape.mkv").unwrap();
    fs::write(outside.path().join("secret.mkv"), b"secret").unwrap();
    symlink(
        outside.path().join("secret.mkv"),
        root.path().join("escape.mkv"),
    )
    .unwrap();
    let backend = FilesystemBackend::new(root.path()).await.unwrap();
    let files = backend
        .list_children(backend.root_id(), None)
        .await
        .unwrap();
    let descriptor = files
        .objects
        .iter()
        .find(|object| object.name() == "episode.strm")
        .unwrap();

    assert!(matches!(
        backend
            .resolve_local_reference(descriptor.id(), "../secret.mkv")
            .await,
        Err(tjxy_storage::BackendError::InvalidValue { .. })
    ));
    assert!(
        backend
            .resolve_local_reference(descriptor.id(), "escape.mkv")
            .await
            .is_err()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn directory_listing_does_not_follow_symlinks_outside_the_root() {
    use std::os::unix::fs::symlink;

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("secret.mkv"), b"secret").unwrap();
    symlink(
        outside.path().join("secret.mkv"),
        root.path().join("escape.mkv"),
    )
    .unwrap();
    let backend = FilesystemBackend::new(root.path()).await.unwrap();

    let page = backend
        .list_children(backend.root_id(), None)
        .await
        .unwrap();

    assert!(page.objects.is_empty());
}
