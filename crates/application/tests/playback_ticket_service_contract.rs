use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use sea_orm_migration::MigratorTrait;
use tjxy_application::{AuthClock, AuthService, ClientIdentity, PlaybackTicketService};
use tjxy_common::{CatalogItemId, PresentationKey};
use tjxy_test_support::test_database;
use uuid::Uuid;

#[derive(Clone)]
struct FixedClock(chrono::DateTime<Utc>);

impl AuthClock for FixedClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        self.0
    }
}

async fn fixture() -> (
    PlaybackTicketService<FixedClock>,
    tjxy_application::AuthenticatedPrincipal,
    chrono::DateTime<Utc>,
) {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    let now = Utc.with_ymd_and_hms(2026, 7, 30, 14, 0, 0).unwrap();
    let clock = FixedClock(now);
    let auth = AuthService::new(database.clone(), clock.clone(), Some(Duration::days(30)), 1)
        .await
        .unwrap();
    auth.create_user("Alice", "correct horse", false)
        .await
        .unwrap();
    let authentication = auth
        .authenticate(
            "Alice",
            "correct horse",
            ClientIdentity::new("TJXY Web", "Browser", "web-device", "0.1.0").unwrap(),
        )
        .await
        .unwrap();
    let principal = auth
        .authenticate_token(authentication.access_token().expose_secret())
        .await
        .unwrap();
    (PlaybackTicketService::new(database, clock), principal, now)
}

#[tokio::test]
async fn issue_returns_a_redacted_scoped_ticket_with_a_six_hour_expiry() {
    let (service, principal, now) = fixture().await;
    let item_id = CatalogItemId::new();
    let source_id = PresentationKey::new();
    let play_session_id = Uuid::new_v4();

    let issued = service
        .issue(&principal, item_id, source_id, play_session_id)
        .await
        .unwrap();

    assert_eq!(issued.secret().expose_secret().len(), 64);
    assert!(
        issued
            .secret()
            .expose_secret()
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );
    assert_eq!(
        format!("{:?}", issued.secret()),
        "SecretPlaybackTicket([REDACTED])"
    );
    assert_eq!(issued.expires_at(), now + Duration::hours(6));
    let grant = service
        .authorize(issued.secret().expose_secret(), item_id, source_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(grant.ticket_id(), issued.id());
    assert_eq!(grant.user_id(), principal.user().id());
    assert_eq!(grant.play_session_id(), play_session_id);
}

#[tokio::test]
async fn revoke_only_accepts_a_ticket_owned_by_the_current_login_session() {
    let (service, principal, _now) = fixture().await;
    let item_id = CatalogItemId::new();
    let source_id = PresentationKey::new();
    let issued = service
        .issue(&principal, item_id, source_id, Uuid::new_v4())
        .await
        .unwrap();

    assert!(service.revoke(&principal, issued.id()).await.unwrap());
    assert!(
        service
            .authorize(issued.secret().expose_secret(), item_id, source_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[test]
fn service_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PlaybackTicketService<FixedClock>>();
    assert_send_sync::<Arc<PlaybackTicketService<FixedClock>>>();
}
