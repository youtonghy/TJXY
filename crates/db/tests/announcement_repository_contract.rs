use chrono::Utc;
use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::Username;
use tjxy_db::{
    AnnouncementDraftInput, AnnouncementKind, AnnouncementRecord, AnnouncementRepository,
    AnnouncementRepositoryError, AnnouncementStatus, AuthRepository, AuthUser, Migrator,
};
use tjxy_test_support::test_database;

#[tokio::test]
async fn published_content_versions_control_visibility_and_acknowledgement() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let user = test_user(&AuthRepository::new(&database), "announcement-reader").await;
    let repository = AnnouncementRepository::new(&database);

    let draft = repository
        .create_draft(&AnnouncementDraftInput::new(
            "Library maintenance",
            "**Playback** will be unavailable briefly.",
            AnnouncementKind::Popup,
        ))
        .await
        .unwrap();
    assert_eq!(draft.status(), AnnouncementStatus::Draft);
    assert_eq!(draft.content_version(), 0);
    assert_eq!(
        repository
            .visible_page(user.id(), 20, 0)
            .await
            .unwrap()
            .total(),
        0
    );

    let published = repository
        .publish(draft.id(), draft.revision())
        .await
        .unwrap();
    assert_eq!(published.status(), AnnouncementStatus::Published);
    assert_eq!(published.content_version(), 1);
    let visible = repository.visible_page(user.id(), 20, 0).await.unwrap();
    assert_eq!(visible.total(), 1);
    assert_eq!(visible.unread_count(), 1);
    assert!(!visible.items()[0].is_read());
    assert_eq!(
        repository
            .next_popup(user.id())
            .await
            .unwrap()
            .unwrap()
            .id(),
        published.id()
    );

    repository
        .acknowledge(user.id(), published.id(), published.content_version())
        .await
        .unwrap();
    let acknowledged = repository.visible_page(user.id(), 20, 0).await.unwrap();
    assert_eq!(acknowledged.unread_count(), 0);
    assert_eq!(acknowledged.items().len(), 1);
    assert!(acknowledged.items()[0].is_read());
    assert!(repository.next_popup(user.id()).await.unwrap().is_none());

    let republished = repository
        .update(
            published.id(),
            &AnnouncementDraftInput::new(
                "Library maintenance",
                "**Playback and downloads** will be unavailable briefly.",
                AnnouncementKind::Popup,
            ),
            published.revision(),
        )
        .await
        .unwrap();
    assert_eq!(republished.status(), AnnouncementStatus::Published);
    assert_eq!(republished.content_version(), 2);
    assert_eq!(
        repository
            .visible_page(user.id(), 20, 0)
            .await
            .unwrap()
            .unread_count(),
        1
    );
    assert_eq!(
        repository
            .next_popup(user.id())
            .await
            .unwrap()
            .unwrap()
            .id(),
        republished.id()
    );

    let archived = repository
        .archive(republished.id(), republished.revision())
        .await
        .unwrap();
    assert_eq!(archived.status(), AnnouncementStatus::Archived);
    assert_eq!(
        repository
            .visible_page(user.id(), 20, 0)
            .await
            .unwrap()
            .total(),
        0
    );
}

#[tokio::test]
async fn receipts_are_user_scoped_idempotent_and_cascade_with_their_owner() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let auth = AuthRepository::new(&database);
    let alice = test_user(&auth, "announcement-alice").await;
    let bob = test_user(&auth, "announcement-bob").await;
    let repository = AnnouncementRepository::new(&database);
    let popup = published_announcement(
        &repository,
        AnnouncementDraftInput::new(
            "Important change",
            "Please review this change.",
            AnnouncementKind::Popup,
        ),
    )
    .await;
    let standard = published_announcement(
        &repository,
        AnnouncementDraftInput::new(
            "Release notes",
            "A normal announcement.",
            AnnouncementKind::Standard,
        ),
    )
    .await;

    assert_eq!(
        repository
            .visible_page(alice.id(), 20, 0)
            .await
            .unwrap()
            .unread_count(),
        2
    );
    assert_eq!(
        repository
            .next_popup(alice.id())
            .await
            .unwrap()
            .unwrap()
            .id(),
        popup.id()
    );
    assert_ne!(standard.id(), popup.id());

    repository
        .acknowledge(alice.id(), popup.id(), popup.content_version())
        .await
        .unwrap();
    repository
        .acknowledge(alice.id(), popup.id(), popup.content_version())
        .await
        .unwrap();
    assert!(repository.next_popup(alice.id()).await.unwrap().is_none());
    assert_eq!(
        repository.next_popup(bob.id()).await.unwrap().unwrap().id(),
        popup.id()
    );
    assert!(matches!(
        repository
            .acknowledge(bob.id(), popup.id(), popup.content_version() + 1)
            .await,
        Err(AnnouncementRepositoryError::StaleVersion)
    ));
    assert!(matches!(
        repository
            .update(
                popup.id(),
                &AnnouncementDraftInput::new("Stale", "Stale update.", AnnouncementKind::Popup),
                popup.revision() + 10,
            )
            .await,
        Err(AnnouncementRepositoryError::RevisionConflict)
    ));

    auth.delete_user(alice.id()).await.unwrap();
    assert_eq!(receipt_count(&database, popup.id()).await, 0);
    repository
        .acknowledge(bob.id(), popup.id(), popup.content_version())
        .await
        .unwrap();
    assert_eq!(receipt_count(&database, popup.id()).await, 1);
    repository
        .delete(popup.id(), popup.revision())
        .await
        .unwrap();
    assert_eq!(receipt_count(&database, popup.id()).await, 0);
}

async fn test_user(repository: &AuthRepository<'_>, username: &str) -> AuthUser {
    repository
        .create_user(
            &Username::parse(username).unwrap(),
            "test-only",
            false,
            false,
            Utc::now(),
        )
        .await
        .unwrap()
}

async fn published_announcement(
    repository: &AnnouncementRepository<'_>,
    input: AnnouncementDraftInput,
) -> AnnouncementRecord {
    let draft = repository.create_draft(&input).await.unwrap();
    repository
        .publish(draft.id(), draft.revision())
        .await
        .unwrap()
}

async fn receipt_count(database: &sea_orm::DatabaseConnection, announcement_id: uuid::Uuid) -> i64 {
    let statement = Query::select()
        .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
        .from(Alias::new("user_announcement_receipts"))
        .and_where(Expr::col(Alias::new("announcement_id")).eq(announcement_id))
        .to_owned();
    database
        .query_one(database.get_database_backend().build(&statement))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "count")
        .unwrap()
}
