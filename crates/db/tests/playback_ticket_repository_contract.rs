use chrono::{Duration, TimeZone, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::{CatalogItemId, PresentationKey, Username};
use tjxy_db::{
    AuthRepository, Migrator, PlaybackTicketDraft, PlaybackTicketRepository,
    PlaybackTicketRepositoryError, SessionDraft,
};
use tjxy_test_support::test_database;
use uuid::Uuid;

struct Fixture {
    database: DatabaseConnection,
    session_id: Uuid,
    user_id: tjxy_common::UserId,
    now: chrono::DateTime<Utc>,
}

async fn fixture() -> Fixture {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let repository = AuthRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 30, 12, 0, 0).unwrap();
    let user = repository
        .create_user(
            &Username::parse("Alice").unwrap(),
            "$argon2id$test-only",
            true,
            false,
            now,
        )
        .await
        .unwrap();
    let credential = repository
        .find_credential(&Username::parse("Alice").unwrap())
        .await
        .unwrap()
        .unwrap();
    let session = repository
        .issue_session(
            &credential,
            SessionDraft {
                id: Uuid::new_v4(),
                token_digest: [1_u8; 32],
                device_id: "browser-device".to_owned(),
                device_name: "Browser".to_owned(),
                client_name: "TJXY Web".to_owned(),
                client_version: "0.1.0".to_owned(),
                created_at: now,
                expires_at: Some(now + Duration::days(30)),
            },
        )
        .await
        .unwrap();
    Fixture {
        database,
        session_id: session.id(),
        user_id: user.id(),
        now,
    }
}

fn draft(fixture: &Fixture, digest: [u8; 32]) -> PlaybackTicketDraft {
    PlaybackTicketDraft {
        id: Uuid::new_v4(),
        auth_session_id: fixture.session_id,
        user_id: fixture.user_id,
        item_id: CatalogItemId::new(),
        media_source_id: PresentationKey::new(),
        play_session_id: Uuid::new_v4(),
        token_digest: digest,
        expires_at: fixture.now + Duration::hours(6),
        created_at: fixture.now,
    }
}

#[tokio::test]
async fn issued_ticket_authorizes_only_its_item_and_media_source() {
    let fixture = fixture().await;
    let repository = PlaybackTicketRepository::new(&fixture.database);
    let draft = draft(&fixture, [7_u8; 32]);
    let ticket_id = draft.id;
    let item_id = draft.item_id;
    let media_source_id = draft.media_source_id;
    let play_session_id = draft.play_session_id;

    repository.issue(draft).await.unwrap();

    let grant = repository
        .authorize(&[7_u8; 32], fixture.now, item_id, media_source_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(grant.ticket_id(), ticket_id);
    assert_eq!(grant.auth_session_id(), fixture.session_id);
    assert_eq!(grant.user_id(), fixture.user_id);
    assert_eq!(grant.item_id(), item_id);
    assert_eq!(grant.media_source_id(), media_source_id);
    assert_eq!(grant.play_session_id(), play_session_id);

    assert!(
        repository
            .authorize(
                &[7_u8; 32],
                fixture.now,
                CatalogItemId::new(),
                media_source_id,
            )
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        repository
            .authorize(&[7_u8; 32], fixture.now, item_id, PresentationKey::new(),)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn expired_ticket_is_not_authorized() {
    let fixture = fixture().await;
    let repository = PlaybackTicketRepository::new(&fixture.database);
    let mut draft = draft(&fixture, [8_u8; 32]);
    draft.expires_at = fixture.now + Duration::seconds(1);
    let item_id = draft.item_id;
    let media_source_id = draft.media_source_id;
    repository.issue(draft).await.unwrap();

    assert!(
        repository
            .authorize(
                &[8_u8; 32],
                fixture.now + Duration::seconds(1),
                item_id,
                media_source_id,
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn ticket_revocation_is_scoped_to_its_login_session() {
    let fixture = fixture().await;
    let repository = PlaybackTicketRepository::new(&fixture.database);
    let draft = draft(&fixture, [9_u8; 32]);
    let ticket_id = draft.id;
    let item_id = draft.item_id;
    let media_source_id = draft.media_source_id;
    repository.issue(draft).await.unwrap();

    assert!(
        !repository
            .revoke(Uuid::new_v4(), ticket_id, fixture.now)
            .await
            .unwrap()
    );
    assert!(
        repository
            .authorize(&[9_u8; 32], fixture.now, item_id, media_source_id)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        repository
            .revoke(fixture.session_id, ticket_id, fixture.now)
            .await
            .unwrap()
    );
    assert!(
        repository
            .authorize(&[9_u8; 32], fixture.now, item_id, media_source_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn revoked_login_session_invalidates_issued_ticket() {
    let fixture = fixture().await;
    let repository = PlaybackTicketRepository::new(&fixture.database);
    let draft = draft(&fixture, [10_u8; 32]);
    let item_id = draft.item_id;
    let media_source_id = draft.media_source_id;
    repository.issue(draft).await.unwrap();
    AuthRepository::new(&fixture.database)
        .revoke_session(
            fixture.user_id,
            fixture.session_id,
            fixture.now + Duration::seconds(1),
            "logout",
        )
        .await
        .unwrap();

    assert!(
        repository
            .authorize(
                &[10_u8; 32],
                fixture.now + Duration::seconds(1),
                item_id,
                media_source_id,
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn changed_user_authorization_revision_invalidates_issued_ticket() {
    let fixture = fixture().await;
    let repository = PlaybackTicketRepository::new(&fixture.database);
    let draft = draft(&fixture, [11_u8; 32]);
    let item_id = draft.item_id;
    let media_source_id = draft.media_source_id;
    repository.issue(draft).await.unwrap();
    AuthRepository::new(&fixture.database)
        .rename_user(
            fixture.user_id,
            &Username::parse("Alice Renamed").unwrap(),
            fixture.now + Duration::seconds(1),
        )
        .await
        .unwrap();

    assert!(
        repository
            .authorize(
                &[11_u8; 32],
                fixture.now + Duration::seconds(1),
                item_id,
                media_source_id,
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn issue_rejects_revoked_login_session() {
    let fixture = fixture().await;
    AuthRepository::new(&fixture.database)
        .revoke_session(
            fixture.user_id,
            fixture.session_id,
            fixture.now + Duration::seconds(1),
            "logout",
        )
        .await
        .unwrap();
    let error = PlaybackTicketRepository::new(&fixture.database)
        .issue(draft(&fixture, [12_u8; 32]))
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        PlaybackTicketRepositoryError::SessionRejected
    ));
}

#[tokio::test]
async fn issue_caps_each_login_session_at_thirty_two_active_tickets() {
    let fixture = fixture().await;
    let repository = PlaybackTicketRepository::new(&fixture.database);
    for marker in 20_u8..52_u8 {
        repository
            .issue(draft(&fixture, [marker; 32]))
            .await
            .unwrap();
    }

    let error = repository
        .issue(draft(&fixture, [52_u8; 32]))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        PlaybackTicketRepositoryError::CapacityReached
    ));
}

#[tokio::test]
async fn issue_caps_ticket_expiry_at_the_login_session_expiry() {
    let fixture = fixture().await;
    let session_expiry = fixture.now + Duration::hours(1);
    let update = Query::update()
        .table(Alias::new("auth_sessions"))
        .value(Alias::new("expires_at"), session_expiry)
        .and_where(sea_orm::sea_query::Expr::col(Alias::new("id")).eq(fixture.session_id))
        .to_owned();
    fixture
        .database
        .execute(fixture.database.get_database_backend().build(&update))
        .await
        .unwrap();

    let actual_expiry = PlaybackTicketRepository::new(&fixture.database)
        .issue(draft(&fixture, [53_u8; 32]))
        .await
        .unwrap();

    assert_eq!(actual_expiry, session_expiry);
}
