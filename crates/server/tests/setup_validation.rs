use tempfile::tempdir;
use tjxy_server::{
    DatabaseBackend, DatabaseDraft, DatabaseTlsMode, SecretString, SetupErrorCode, SetupValidator,
};

#[tokio::test]
async fn sqlite_database_test_is_root_confined_and_reports_safe_metadata() {
    let allowed = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let validator = SetupValidator::new(vec![allowed.path().to_path_buf()]).unwrap();

    let database_path = allowed.path().join("tjxy.db");
    let result = validator
        .test_database(&DatabaseDraft::Sqlite {
            path: database_path.clone(),
        })
        .await
        .unwrap();
    assert_eq!(result.backend(), DatabaseBackend::Sqlite);
    assert!(result.version().starts_with("3."));
    assert!(database_path.is_file());

    let escaped = validator
        .test_database(&DatabaseDraft::Sqlite {
            path: outside.path().join("tjxy.db"),
        })
        .await;
    assert_eq!(
        escaped.unwrap_err().code(),
        SetupErrorCode::UnsafeDatabasePath
    );
}

#[test]
fn remote_database_draft_debug_redacts_password() {
    let draft = DatabaseDraft::PostgreSql {
        host: "db.internal".to_owned(),
        port: 5432,
        database: "tjxy".to_owned(),
        username: "tjxy".to_owned(),
        password: SecretString::new("database-secret"),
        tls: DatabaseTlsMode::Require,
    };
    let debug = format!("{draft:?}");
    assert!(!debug.contains("database-secret"));

    let mysql = DatabaseDraft::Mysql {
        host: "mysql.internal".to_owned(),
        port: 3306,
        database: "tjxy".to_owned(),
        username: "tjxy".to_owned(),
        password: SecretString::new("mysql-secret"),
        tls: DatabaseTlsMode::Prefer,
    };
    let debug = format!("{mysql:?}");
    assert!(!debug.contains("mysql-secret"));
}
