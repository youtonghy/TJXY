use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Query},
};
use sea_orm_migration::MigratorTrait;
use tempfile::TempDir;
use tjxy_application::{AssetReadError, AssetReadService};
use tjxy_common::{CatalogItemId, ImageType, SortKey};
use tjxy_test_support::test_database;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn seed_visible_item(database: &DatabaseConnection) -> CatalogItemId {
    let backend = database.get_database_backend();
    let library_id = Uuid::new_v4();
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
                        library_id.into(),
                        "Movies".into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text("Movies").into_bytes().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    let item_id = CatalogItemId::new();
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
                        Alias::new("sort_key"),
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item_id.as_uuid().into(),
                        "Movie".into(),
                        "Arrival".into(),
                        "arrival".into(),
                        SortKey::from_text("Arrival").into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        "NotApplicable".into(),
                        "Indexed".into(),
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
                    .values_panic([
                        Uuid::new_v4().into(),
                        library_id.into(),
                        item_id.as_uuid().into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    item_id
}

async fn seed_asset(
    database: &DatabaseConnection,
    item_id: CatalogItemId,
    sha256: &str,
    byte_size: i64,
    relative_path: &str,
) {
    let backend = database.get_database_backend();
    let blob_id = Uuid::new_v4();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("asset_blobs"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("sha256"),
                        Alias::new("mime_type"),
                        Alias::new("byte_size"),
                        Alias::new("local_relative_path"),
                    ])
                    .values_panic([
                        blob_id.into(),
                        sha256.into(),
                        "image/jpeg".into(),
                        byte_size.into(),
                        relative_path.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("item_assets"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_id"),
                        Alias::new("asset_blob_id"),
                        Alias::new("image_type"),
                        Alias::new("priority"),
                        Alias::new("source_provider"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        item_id.as_uuid().into(),
                        blob_id.into(),
                        "Primary".into(),
                        0.into(),
                        "fixture".into(),
                    ]),
            ),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn opens_an_authorized_asset_inside_the_configured_root() {
    let database = database().await;
    let item_id = seed_visible_item(&database).await;
    let root = TempDir::new().unwrap();
    tokio::fs::create_dir(root.path().join("posters"))
        .await
        .unwrap();
    tokio::fs::write(root.path().join("posters/arrival.jpg"), b"jpeg")
        .await
        .unwrap();
    let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    seed_asset(&database, item_id, sha256, 4, "posters/arrival.jpg").await;
    let service = AssetReadService::new(database, root.path()).await.unwrap();

    let mut asset = service
        .original(item_id, ImageType::Primary, 0)
        .await
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    asset.file_mut().read_to_end(&mut bytes).await.unwrap();

    assert_eq!(asset.sha256(), sha256);
    assert_eq!(asset.mime_type(), "image/jpeg");
    assert_eq!(asset.byte_size(), 4);
    assert_eq!(bytes, b"jpeg");
}

#[tokio::test]
async fn rejects_database_paths_that_escape_the_asset_root() {
    let database = database().await;
    let item_id = seed_visible_item(&database).await;
    let root = TempDir::new().unwrap();
    let sha256 = "1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    seed_asset(&database, item_id, sha256, 4, "../secret.jpg").await;
    let service = AssetReadService::new(database, root.path()).await.unwrap();

    let result = service.original(item_id, ImageType::Primary, 0).await;

    assert!(matches!(result, Err(AssetReadError::InvalidStoredPath)));
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_symlinks_that_resolve_outside_the_asset_root() {
    let database = database().await;
    let item_id = seed_visible_item(&database).await;
    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    std::fs::write(outside.path().join("secret.jpg"), b"jpeg").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.jpg"),
        root.path().join("link.jpg"),
    )
    .unwrap();
    let sha256 = "3123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    seed_asset(&database, item_id, sha256, 4, "link.jpg").await;
    let service = AssetReadService::new(database, root.path()).await.unwrap();

    let result = service.original(item_id, ImageType::Primary, 0).await;

    assert!(matches!(result, Err(AssetReadError::InvalidStoredPath)));
}

#[tokio::test]
async fn reports_missing_or_size_mismatched_files_as_integrity_errors() {
    for (relative_path, expected) in [
        ("missing.jpg", "missing"),
        ("wrong-size.jpg", "size mismatch"),
    ] {
        let database = database().await;
        let item_id = seed_visible_item(&database).await;
        let root = TempDir::new().unwrap();
        if relative_path == "wrong-size.jpg" {
            tokio::fs::write(root.path().join(relative_path), b"too long")
                .await
                .unwrap();
        }
        let sha256 = "2123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        seed_asset(&database, item_id, sha256, 4, relative_path).await;
        let service = AssetReadService::new(database, root.path()).await.unwrap();

        let error = service
            .original(item_id, ImageType::Primary, 0)
            .await
            .unwrap_err();

        assert!(error.to_string().contains(expected));
    }
}
