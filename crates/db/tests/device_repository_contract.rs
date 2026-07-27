use chrono::{Duration, TimeZone, Utc};
use sea_orm::DatabaseConnection;
use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use sea_orm_migration::MigratorTrait;
use tjxy_common::Username;
use tjxy_db::{AuthRepository, DeviceRepository, SessionCapabilitiesDraft, SessionDraft};
use tjxy_test_support::test_database;
use uuid::Uuid;

async fn database() -> DatabaseConnection {
    let database = test_database().await.unwrap();
    tjxy_db::Migrator::up(&database, None).await.unwrap();
    database
}

fn session(
    now: chrono::DateTime<Utc>,
    digest: u8,
    device_id: &str,
    device_name: &str,
    client: &str,
) -> SessionDraft {
    session_with_digest(now, [digest; 32], device_id, device_name, client)
}

fn numbered_session(now: chrono::DateTime<Utc>, digest: u32, device_id: &str) -> SessionDraft {
    let mut token_digest = [0_u8; 32];
    token_digest[..4].copy_from_slice(&digest.to_be_bytes());
    session_with_digest(now, token_digest, device_id, "Device", "Client")
}

fn session_with_digest(
    now: chrono::DateTime<Utc>,
    token_digest: [u8; 32],
    device_id: &str,
    device_name: &str,
    client: &str,
) -> SessionDraft {
    SessionDraft {
        id: Uuid::new_v4(),
        token_digest,
        device_id: device_id.to_owned(),
        device_name: device_name.to_owned(),
        client_name: client.to_owned(),
        client_version: "1.0".to_owned(),
        created_at: now,
        expires_at: Some(now + Duration::days(30)),
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps latest identity, filtering, capabilities, and options in one lifecycle.
async fn devices_use_the_latest_active_identity_and_persist_options() {
    let database = database().await;
    let auth = AuthRepository::new(&database);
    let devices = DeviceRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let alice = auth
        .create_user(
            &Username::parse("Alice").unwrap(),
            "$argon2id$alice",
            true,
            true,
            now,
        )
        .await
        .unwrap();
    let bob = auth
        .create_user(
            &Username::parse("Bob").unwrap(),
            "$argon2id$bob",
            true,
            false,
            now,
        )
        .await
        .unwrap();
    let alice_credential = auth
        .find_credential(&Username::parse("Alice").unwrap())
        .await
        .unwrap()
        .unwrap();
    let bob_credential = auth
        .find_credential(&Username::parse("Bob").unwrap())
        .await
        .unwrap()
        .unwrap();
    auth.issue_session(
        &alice_credential,
        session(now, 31, "shared-device", "Alice Phone", "Jellyfin Web"),
    )
    .await
    .unwrap();
    let bob_session = auth
        .issue_session(
            &bob_credential,
            session(
                now + Duration::minutes(5),
                32,
                "shared-device",
                "Bob Tablet",
                "Findroid",
            ),
        )
        .await
        .unwrap();
    auth.update_session_capabilities(
        bob.id(),
        bob_session.id(),
        SessionCapabilitiesDraft {
            playable_media_types: vec!["Video".to_owned(), "Audio".to_owned()],
            supported_commands: vec!["Play".to_owned()],
            supports_media_control: true,
            supports_persistent_identifier: true,
            device_profile: Some(serde_json::json!({"Name": "Findroid"})),
            app_store_url: Some("https://example.invalid/app".to_owned()),
            icon_url: Some("https://example.invalid/icon".to_owned()),
        },
    )
    .await
    .unwrap();

    let all = devices
        .list_active(None, now + Duration::minutes(6))
        .await
        .unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].device_id(), "shared-device");
    assert_eq!(all[0].device_name(), "Bob Tablet");
    assert_eq!(all[0].user_id(), bob.id());
    assert_eq!(all[0].user_name(), "Bob");
    assert_eq!(all[0].client_name(), "Findroid");
    assert_eq!(all[0].playable_media_types(), ["Video", "Audio"]);
    assert_eq!(
        all[0].device_profile(),
        Some(&serde_json::json!({"Name": "Findroid"}))
    );

    let alice_devices = devices
        .list_active(Some(alice.id()), now + Duration::minutes(6))
        .await
        .unwrap();
    assert_eq!(alice_devices.len(), 1);
    assert_eq!(alice_devices[0].device_name(), "Alice Phone");
    assert_eq!(alice_devices[0].user_id(), alice.id());

    assert!(
        devices
            .update_options(
                "shared-device",
                Some("Living room"),
                now + Duration::minutes(6),
            )
            .await
            .unwrap()
    );
    let options = devices.options("shared-device").await.unwrap().unwrap();
    assert!(options.id() > 0);
    assert_eq!(options.device_id(), "shared-device");
    assert_eq!(options.custom_name(), Some("Living room"));
    assert_eq!(
        devices
            .device("shared-device", now + Duration::minutes(6))
            .await
            .unwrap()
            .unwrap()
            .custom_name(),
        Some("Living room")
    );
}

#[tokio::test]
async fn device_delete_is_atomic_and_revokes_every_matching_session() {
    let database = database().await;
    let auth = AuthRepository::new(&database);
    let devices = DeviceRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let user = auth
        .create_user(
            &Username::parse("Alice").unwrap(),
            "$argon2id$alice",
            true,
            true,
            now,
        )
        .await
        .unwrap();
    let credential = auth
        .find_credential(&Username::parse("Alice").unwrap())
        .await
        .unwrap()
        .unwrap();
    for (digest, device_id) in [(41, "phone-1"), (42, "phone-1"), (43, "tablet-2")] {
        auth.issue_session(
            &credential,
            session(now, digest, device_id, "Device", "Client"),
        )
        .await
        .unwrap();
    }

    assert!(
        !devices
            .delete_active(&["phone-1", "missing"], now + Duration::minutes(1))
            .await
            .unwrap()
    );
    assert!(
        auth.find_principal_by_token_digest(&[41; 32], now + Duration::minutes(1))
            .await
            .unwrap()
            .is_some()
    );

    assert!(
        devices
            .delete_active(&["phone-1"], now + Duration::minutes(1))
            .await
            .unwrap()
    );
    for digest in [41, 42] {
        assert!(
            auth.find_principal_by_token_digest(&[digest; 32], now + Duration::minutes(1))
                .await
                .unwrap()
                .is_none()
        );
    }
    assert!(
        auth.find_principal_by_token_digest(&[43; 32], now + Duration::minutes(1))
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        devices
            .list_active(Some(user.id()), now + Duration::minutes(1))
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn device_identity_is_case_sensitive_and_persisted_as_an_exact_key() {
    let database = database().await;
    let auth = AuthRepository::new(&database);
    let devices = DeviceRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let user = auth
        .create_user(
            &Username::parse("Alice").unwrap(),
            "$argon2id$alice",
            true,
            true,
            now,
        )
        .await
        .unwrap();
    let credential = auth
        .find_credential(&Username::parse("Alice").unwrap())
        .await
        .unwrap()
        .unwrap();
    auth.issue_session(&credential, session(now, 51, "Phone", "Upper", "Client"))
        .await
        .unwrap();
    auth.issue_session(&credential, session(now, 52, "phone", "Lower", "Client"))
        .await
        .unwrap();

    let listed = devices
        .list_active(None, now + Duration::minutes(1))
        .await
        .unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().any(|device| device.device_id() == "Phone"));
    assert!(listed.iter().any(|device| device.device_id() == "phone"));

    let query = Query::select()
        .columns([Alias::new("device_id"), Alias::new("device_key")])
        .from(Alias::new("auth_sessions"))
        .and_where(Expr::col(Alias::new("user_id")).eq(user.id().as_uuid()))
        .to_owned();
    let rows = database
        .query_all(database.get_database_backend().build(&query))
        .await
        .unwrap();
    let keys = rows
        .iter()
        .map(|row| row.try_get::<String>("", "device_key").unwrap())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(keys.len(), 2);

    assert!(
        devices
            .delete_active(&["phone"], now + Duration::minutes(1))
            .await
            .unwrap()
    );
    assert!(
        devices
            .device("Phone", now + Duration::minutes(1))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        devices
            .device("phone", now + Duration::minutes(1))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn repeated_sessions_for_one_device_cannot_hide_another_device() {
    let database = database().await;
    let auth = AuthRepository::new(&database);
    let devices = DeviceRepository::new(&database);
    let now = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    auth.create_user(
        &Username::parse("Alice").unwrap(),
        "$argon2id$alice",
        true,
        true,
        now,
    )
    .await
    .unwrap();
    let credential = auth
        .find_credential(&Username::parse("Alice").unwrap())
        .await
        .unwrap()
        .unwrap();
    auth.issue_session(&credential, numbered_session(now, 10_000, "device-b"))
        .await
        .unwrap();
    for digest in 0..256 {
        auth.issue_session(
            &credential,
            numbered_session(now + Duration::minutes(1), digest, "device-a"),
        )
        .await
        .unwrap();
    }

    let listed = devices
        .list_active(None, now + Duration::minutes(2))
        .await
        .unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().any(|device| device.device_id() == "device-a"));
    assert!(listed.iter().any(|device| device.device_id() == "device-b"));
}
