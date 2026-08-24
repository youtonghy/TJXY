use std::path::Path;

use tempfile::TempDir;
use tjxy_application::{FilesystemBrowser, FilesystemBrowserError};

#[tokio::test]
async fn browser_lists_only_directories_in_stable_order_without_exposing_the_root_path() {
    let root = TempDir::new().unwrap();
    tokio::fs::create_dir(root.path().join("Zoo"))
        .await
        .unwrap();
    tokio::fs::create_dir(root.path().join("alpha"))
        .await
        .unwrap();
    tokio::fs::write(root.path().join("movie.mkv"), b"video")
        .await
        .unwrap();
    let browser = FilesystemBrowser::from_roots([root.path()]).await.unwrap();

    let roots = browser.roots();
    assert_eq!(roots.len(), 1);
    assert!(!format!("{roots:?}").contains(root.path().to_str().unwrap()));
    let page = browser.list(roots[0].id(), Path::new("")).await.unwrap();

    assert_eq!(
        page.entries()
            .iter()
            .map(|entry| (entry.name(), entry.relative_path()))
            .collect::<Vec<_>>(),
        vec![("alpha", "alpha"), ("Zoo", "Zoo")]
    );
}

#[tokio::test]
async fn browser_rejects_paths_outside_the_allowed_root() {
    let root = TempDir::new().unwrap();
    let browser = FilesystemBrowser::from_roots([root.path()]).await.unwrap();
    let root_id = browser.roots()[0].id();

    assert!(matches!(
        browser.list(root_id, Path::new("../outside")).await,
        Err(FilesystemBrowserError::InvalidRelativePath)
    ));
    assert!(matches!(
        browser.list(root_id, Path::new("/private")).await,
        Err(FilesystemBrowserError::InvalidRelativePath)
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn browser_does_not_list_or_resolve_a_symlink_that_escapes_the_allowed_root() {
    use std::os::unix::fs::symlink;

    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    symlink(outside.path(), root.path().join("escape")).unwrap();
    let browser = FilesystemBrowser::from_roots([root.path()]).await.unwrap();
    let root_id = browser.roots()[0].id();

    let page = browser.list(root_id, Path::new("")).await.unwrap();
    assert!(page.entries().is_empty());
    assert!(matches!(
        browser.resolve(root_id, Path::new("escape")).await,
        Err(FilesystemBrowserError::EscapedRoot)
    ));
}

#[tokio::test]
async fn available_roots_skip_missing_and_duplicate_entries() {
    let root = TempDir::new().unwrap();
    let missing = root.path().join("missing");

    let (browser, invalid) =
        FilesystemBrowser::from_available_roots([missing.as_path(), root.path(), root.path()])
            .await;

    assert_eq!(invalid, [0, 2]);
    assert_eq!(browser.unwrap().roots().len(), 1);
}

#[tokio::test]
async fn available_roots_return_no_browser_when_every_entry_is_invalid() {
    let root = TempDir::new().unwrap();

    let (browser, invalid) =
        FilesystemBrowser::from_available_roots([root.path().join("missing")]).await;

    assert!(browser.is_none());
    assert_eq!(invalid, [0]);
}
