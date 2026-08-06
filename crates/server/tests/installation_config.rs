use std::fs;

use tempfile::tempdir;
use tjxy_server::{
    DatabaseConfiguration, DatabaseTlsMode, InstallationConfigError, InstallationConfigStore,
    InstallationProfile, InstallationState, NetworkConfiguration, PendingInstallation,
    SecretString, ServerIdentity, StartupOptions,
};
use uuid::Uuid;

const COMPLETED_CONFIG: &str = r#"
format_version = 1
state = "completed"
installation_id = "11111111-1111-4111-8111-111111111111"
server_id = "22222222-2222-4222-8222-222222222222"
credential_keyring = '{"active_version":1,"keys":{"1":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}}'

[database]
backend = "postgresql"
host = "db.internal"
port = 5432
database = "tjxy"
username = "tjxy"
password = "database-secret"
tls = "prefer"

[network]
listen_host = "0.0.0.0"
port = 8096
public_url = "https://media.example.test"

[profile]
site_title = "TJXY"
site_subtitle = "Private media"
locale = "en-US"
logo_url = "/brand/tjxy-mark.webp"
icon_url = "/brand/favicon.svg"
administrator_username = "admin"
"#;

const TEST_KEYRING: &str =
    r#"{"active_version":1,"keys":{"1":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}}"#;

#[test]
fn absent_file_is_unconfigured_and_completed_file_is_strict_and_redacted() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("tjxy.toml");
    let store = InstallationConfigStore::at(&path);

    assert_eq!(store.load().unwrap(), InstallationState::Unconfigured);

    fs::write(&path, COMPLETED_CONFIG).unwrap();
    let state = store.load().unwrap();
    assert!(matches!(state, InstallationState::Completed(_)));
    let debug = format!("{state:?}");
    assert!(!debug.contains("database-secret"));
    assert!(!debug.contains("secret-key-material"));

    fs::write(&path, format!("{COMPLETED_CONFIG}\nunknown = true\n")).unwrap();
    assert!(store.load().is_err());

    fs::write(&path, "x".repeat(64 * 1024 + 1)).unwrap();
    assert!(store.load().is_err());
}

#[test]
fn pending_configuration_is_atomic_private_and_completable() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nested").join("tjxy.toml");
    let store = InstallationConfigStore::at(&path);
    let pending = PendingInstallation::new(
        Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
        Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
        SecretString::new(
            r#"{"active_version":1,"keys":{"1":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}}"#,
        ),
        DatabaseConfiguration::PostgreSql {
            host: "db.internal".to_owned(),
            port: 5432,
            database: "tjxy".to_owned(),
            username: "tjxy".to_owned(),
            password: SecretString::new("database-secret"),
            tls: DatabaseTlsMode::Prefer,
        },
        NetworkConfiguration::new(
            "0.0.0.0",
            8096,
            Some("https://media.example.test".to_owned()),
        ),
        profile("admin"),
    );

    store.write_pending(&pending).unwrap();
    assert!(matches!(
        store.load().unwrap(),
        InstallationState::Pending(_)
    ));
    assert!(
        directory
            .path()
            .join("nested")
            .read_dir()
            .unwrap()
            .all(|entry| { entry.unwrap().file_name() == "tjxy.toml" })
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    store.complete(&pending.complete()).unwrap();
    assert!(matches!(
        store.load().unwrap(),
        InstallationState::Completed(_)
    ));
}

#[test]
fn pending_configuration_cannot_replace_another_or_downgrade_completed_state() {
    let directory = tempdir().unwrap();
    let store = InstallationConfigStore::at(directory.path().join("tjxy.toml"));
    let first = PendingInstallation::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        SecretString::new(TEST_KEYRING),
        DatabaseConfiguration::Sqlite {
            path: directory.path().join("first.db"),
        },
        NetworkConfiguration::new("127.0.0.1", 8096, None),
        profile("first-admin"),
    );
    let different = PendingInstallation::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        SecretString::new(TEST_KEYRING),
        DatabaseConfiguration::Sqlite {
            path: directory.path().join("different.db"),
        },
        NetworkConfiguration::new("127.0.0.1", 8097, None),
        profile("different-admin"),
    );

    store.write_pending(&first).unwrap();
    store.write_pending(&first).unwrap();
    assert_eq!(
        store.write_pending(&different),
        Err(InstallationConfigError::Conflict)
    );
    store.complete(&first.complete()).unwrap();
    assert_eq!(
        store.write_pending(&different),
        Err(InstallationConfigError::Conflict)
    );
    assert!(matches!(
        store.load().unwrap(),
        InstallationState::Completed(_)
    ));
}

#[cfg(unix)]
#[test]
fn configuration_target_must_not_be_a_symbolic_link() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    let target = directory.path().join("target.toml");
    let link = directory.path().join("tjxy.toml");
    fs::write(&target, "owned by another file").unwrap();
    symlink(&target, &link).unwrap();

    let result = InstallationConfigStore::at(&link).write_pending(&PendingInstallation::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        SecretString::new(TEST_KEYRING),
        DatabaseConfiguration::Sqlite {
            path: directory.path().join("tjxy.db"),
        },
        NetworkConfiguration::new("127.0.0.1", 8096, None),
        profile("admin"),
    ));

    assert_eq!(result, Err(InstallationConfigError::UnsafeTarget));
    assert_eq!(fs::read_to_string(target).unwrap(), "owned by another file");
}

fn profile(administrator_username: &str) -> InstallationProfile {
    InstallationProfile::new(
        "TJXY",
        "Private media",
        "en-US",
        "/brand/tjxy-mark.webp",
        "/brand/favicon.svg",
        administrator_username,
    )
}

#[test]
fn application_startup_debug_never_exposes_database_credentials() {
    let options = StartupOptions::new(
        "postgresql://database-user:database-secret@db.internal:5432/tjxy",
        ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
    );

    let debug = format!("{options:?}");
    assert!(debug.contains("postgresql"));
    assert!(!debug.contains("database-user"));
    assert!(!debug.contains("database-secret"));
    assert!(!debug.contains("db.internal"));
}

#[test]
fn remote_database_urls_preserve_backend_tls_and_escape_credentials() {
    let postgres = DatabaseConfiguration::PostgreSql {
        host: "postgres.internal".to_owned(),
        port: 5432,
        database: "tjxy".to_owned(),
        username: "setup user".to_owned(),
        password: SecretString::new("postgres@secret"),
        tls: DatabaseTlsMode::Require,
    };
    let mysql = DatabaseConfiguration::Mysql {
        host: "mysql.internal".to_owned(),
        port: 3306,
        database: "tjxy".to_owned(),
        username: "setup user".to_owned(),
        password: SecretString::new("mysql@secret"),
        tls: DatabaseTlsMode::Disable,
    };

    let postgres_url = postgres.connection_url().unwrap();
    assert_eq!(
        postgres_url.as_str(),
        "postgresql://setup%20user:postgres%40secret@postgres.internal:5432/tjxy?sslmode=require"
    );
    let mysql_url = mysql.connection_url().unwrap();
    assert_eq!(
        mysql_url.as_str(),
        "mysql://setup%20user:mysql%40secret@mysql.internal:3306/tjxy?ssl-mode=DISABLED"
    );
}

#[test]
fn mysql_configuration_debug_never_exposes_database_credentials() {
    let configuration = DatabaseConfiguration::Mysql {
        host: "mysql.internal".to_owned(),
        port: 3306,
        database: "tjxy".to_owned(),
        username: "mysql-user".to_owned(),
        password: SecretString::new("mysql-secret"),
        tls: DatabaseTlsMode::Prefer,
    };

    let debug = format!("{configuration:?}");
    assert!(debug.contains("Mysql"));
    assert!(debug.contains("mysql.internal"));
    assert!(debug.contains("mysql-user"));
    assert!(!debug.contains("mysql-secret"));
}
