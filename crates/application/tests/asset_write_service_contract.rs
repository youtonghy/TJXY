use std::io::Cursor;

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tempfile::TempDir;
use tjxy_application::{AssetReadService, AssetWriteError, AssetWriteService};
use tjxy_common::ImageType;
use tjxy_test_support::test_database;
use tokio::io::AsyncReadExt;

mod support {
    include!("support/asset_write.rs");
}

#[tokio::test]
async fn content_addressed_write_deduplicates_bytes_and_publishes_readable_references() {
    let database = database().await;
    let first = support::seed_visible_item(&database, "First").await;
    let second = support::seed_visible_item(&database, "Second").await;
    let root = TempDir::new().unwrap();
    let bytes = png_bytes();
    let writer = AssetWriteService::new(database.clone(), root.path())
        .await
        .unwrap();

    let first_write = writer
        .store_original(
            first,
            ImageType::Primary,
            0,
            "fixture",
            Some("image-1"),
            "image/png",
            &bytes,
        )
        .await
        .unwrap();
    let second_write = writer
        .store_original(
            second,
            ImageType::Primary,
            0,
            "fixture",
            Some("image-2"),
            "image/png",
            &bytes,
        )
        .await
        .unwrap();
    let replay = writer
        .store_original(
            second,
            ImageType::Primary,
            0,
            "fixture",
            Some("image-2"),
            "image/png",
            &bytes,
        )
        .await
        .unwrap();

    assert_eq!(first_write.sha256(), second_write.sha256());
    assert!(!first_write.reused_blob());
    assert!(second_write.reused_blob());
    assert!(replay.reused_blob());
    assert!(!replay.reference_changed());
    assert_eq!(count(&database, "asset_blobs").await, 1);
    assert_eq!(count(&database, "item_assets").await, 2);
    assert_eq!(generation(&database).await, 2);
    assert_eq!(count(&database, "cache_invalidation_outbox").await, 2);
    let reader = AssetReadService::new(database, root.path()).await.unwrap();
    let mut opened = reader
        .original(first, ImageType::Primary, 0)
        .await
        .unwrap()
        .unwrap();
    let mut stored = Vec::new();
    opened.file_mut().read_to_end(&mut stored).await.unwrap();
    assert_eq!(stored, bytes);
    assert_eq!(opened.mime_type(), "image/png");
}

#[tokio::test]
async fn format_mismatch_is_rejected_before_file_or_sql_publication() {
    let database = database().await;
    let item = support::seed_visible_item(&database, "Mismatch").await;
    let root = TempDir::new().unwrap();
    let writer = AssetWriteService::new(database.clone(), root.path())
        .await
        .unwrap();

    let error = writer
        .store_original(
            item,
            ImageType::Primary,
            0,
            "fixture",
            None,
            "image/jpeg",
            &png_bytes(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, AssetWriteError::FormatMismatch));
    assert_eq!(count(&database, "asset_blobs").await, 0);
    assert_eq!(count(&database, "item_assets").await, 0);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

fn png_bytes() -> Vec<u8> {
    let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 3, Rgba([1, 2, 3, 255])));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

async fn count(database: &DatabaseConnection, table: &str) -> i64 {
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

async fn generation(database: &DatabaseConnection) -> i64 {
    let backend = database.get_database_backend();
    database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("generation"))
                    .from(Alias::new("catalog_state"))
                    .and_where(Expr::col(Alias::new("id")).eq(1_i32)),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "generation")
        .unwrap()
}
