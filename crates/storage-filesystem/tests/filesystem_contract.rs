use std::fs;

use futures_util::StreamExt;
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
