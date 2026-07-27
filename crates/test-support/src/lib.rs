use std::env;

use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, DbErr, Statement,
};
use tempfile::TempDir;
use url::Url;
use uuid::Uuid;

pub const TEST_DATABASE_URL: &str = "TJXY_TEST_DATABASE_URL";

/// An isolated test database that can be opened again through the same URL.
pub struct ReconnectableTestDatabase {
    database_url: String,
    connection: DatabaseConnection,
    _directory: Option<TempDir>,
}

impl ReconnectableTestDatabase {
    /// Returns the URL that reconnects to this fixture's isolated database or schema.
    #[must_use]
    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    /// Returns the fixture's initial connection for seeding and assertions.
    #[must_use]
    pub const fn connection(&self) -> &DatabaseConnection {
        &self.connection
    }
}

/// Connects to an isolated test database selected by [`TEST_DATABASE_URL`].
///
/// `SQLite` remains the zero-configuration default. `PostgreSQL` connections receive a fresh
/// schema, while `MySQL` connections receive a fresh database, so independently running tests
/// cannot observe each other's migrations or fixtures. External test databases are expected to
/// be disposable; isolated schemas and databases are intentionally retained for inspection.
///
/// # Errors
///
/// Returns a database error when the environment value is invalid, the server cannot be reached,
/// the `PostgreSQL` schema or `MySQL` database cannot be created, or `SQLite` foreign keys
/// cannot be enabled.
pub async fn test_database() -> Result<DatabaseConnection, DbErr> {
    let database_url = match env::var(TEST_DATABASE_URL) {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => "sqlite::memory:".to_owned(),
        Err(env::VarError::NotUnicode(_)) => {
            return Err(DbErr::Custom(format!(
                "{TEST_DATABASE_URL} must contain valid Unicode"
            )));
        }
    };

    if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
        return postgres_test_database(database_url).await;
    }
    if database_url.starts_with("mysql://") {
        return mysql_test_database(database_url).await;
    }

    let database = Database::connect(database_url).await?;
    enable_sqlite_foreign_keys(&database).await?;
    Ok(database)
}

/// Creates an isolated test database that code under test may reconnect to by URL.
///
/// The zero-configuration `SQLite` fixture uses a temporary file whose lifetime is
/// owned by the returned handle. `PostgreSQL` uses the database selected by
/// [`TEST_DATABASE_URL`] and assigns every connection to one fresh schema.
///
/// # Errors
///
/// Returns a database error when the environment value is invalid, the temporary
/// `SQLite` database cannot be created, or the `PostgreSQL` schema or `MySQL` database cannot
/// be created.
pub async fn reconnectable_test_database() -> Result<ReconnectableTestDatabase, DbErr> {
    let database_url = match env::var(TEST_DATABASE_URL) {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => return sqlite_test_database().await,
        Err(env::VarError::NotUnicode(_)) => {
            return Err(DbErr::Custom(format!(
                "{TEST_DATABASE_URL} must contain valid Unicode"
            )));
        }
    };

    if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
        let schema = create_postgres_test_schema(&database_url).await?;
        let database_url = postgres_schema_database_url(&database_url, &schema);
        let connection = Database::connect(&database_url).await?;
        return Ok(ReconnectableTestDatabase {
            database_url,
            connection,
            _directory: None,
        });
    }
    if database_url.starts_with("mysql://") {
        return reconnectable_mysql_test_database(database_url).await;
    }

    let connection = Database::connect(&database_url).await?;
    enable_sqlite_foreign_keys(&connection).await?;
    Ok(ReconnectableTestDatabase {
        database_url,
        connection,
        _directory: None,
    })
}

async fn postgres_test_database(database_url: String) -> Result<DatabaseConnection, DbErr> {
    let schema = create_postgres_test_schema(&database_url).await?;

    let mut options = ConnectOptions::new(database_url);
    options
        .max_connections(5)
        .min_connections(1)
        .set_schema_search_path(schema);
    Database::connect(options).await
}

async fn mysql_test_database(database_url: String) -> Result<DatabaseConnection, DbErr> {
    let fixture = reconnectable_mysql_test_database(database_url).await?;
    Ok(fixture.connection)
}

async fn reconnectable_mysql_test_database(
    database_url: String,
) -> Result<ReconnectableTestDatabase, DbErr> {
    let database = mysql_test_database_name();
    let administration = Database::connect(&database_url).await?;
    administration
        .execute(Statement::from_string(
            DbBackend::MySql,
            format!("CREATE DATABASE `{database}`"),
        ))
        .await?;
    administration.close().await?;
    let database_url = mysql_database_url(&database_url, &database)?;
    let connection = Database::connect(&database_url).await?;
    Ok(ReconnectableTestDatabase {
        database_url,
        connection,
        _directory: None,
    })
}

async fn create_postgres_test_schema(database_url: &str) -> Result<String, DbErr> {
    let schema = postgres_test_schema_name();
    let administration = Database::connect(database_url).await?;
    administration
        .execute(Statement::from_string(
            DbBackend::Postgres,
            format!(r#"CREATE SCHEMA "{schema}""#),
        ))
        .await?;
    administration.close().await?;
    Ok(schema)
}

async fn sqlite_test_database() -> Result<ReconnectableTestDatabase, DbErr> {
    let directory = TempDir::new()
        .map_err(|error| DbErr::Custom(format!("failed to create test directory: {error}")))?;
    let database_path = directory.path().join("tjxy.db");
    let database_url = format!("sqlite://{}?mode=rwc", database_path.display());
    let connection = Database::connect(&database_url).await?;
    enable_sqlite_foreign_keys(&connection).await?;
    Ok(ReconnectableTestDatabase {
        database_url,
        connection,
        _directory: Some(directory),
    })
}

async fn enable_sqlite_foreign_keys(database: &DatabaseConnection) -> Result<(), DbErr> {
    if database.get_database_backend() == DbBackend::Sqlite {
        database
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys = ON",
            ))
            .await?;
    }
    Ok(())
}

fn postgres_schema_database_url(database_url: &str, schema: &str) -> String {
    let separator = if database_url.ends_with(['?', '&']) {
        ""
    } else if database_url.contains('?') {
        "&"
    } else {
        "?"
    };
    format!("{database_url}{separator}options=-csearch_path%3D{schema}")
}

fn mysql_database_url(database_url: &str, database: &str) -> Result<String, DbErr> {
    let mut url = Url::parse(database_url)
        .map_err(|error| DbErr::Custom(format!("invalid MySQL test database URL: {error}")))?;
    url.set_path(&format!("/{database}"));
    Ok(url.into())
}

fn postgres_test_schema_name() -> String {
    format!("tjxy_test_{}", Uuid::new_v4().simple())
}

fn mysql_test_database_name() -> String {
    format!("tjxy_test_{}", Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use std::env;

    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    use super::{
        TEST_DATABASE_URL, mysql_database_url, postgres_schema_database_url,
        postgres_test_schema_name, reconnectable_test_database, sqlite_test_database,
    };

    #[test]
    fn postgres_test_schema_names_are_unique_safe_identifiers() {
        let first = postgres_test_schema_name();
        let second = postgres_test_schema_name();

        assert_ne!(first, second);
        for schema in [first, second] {
            assert!(schema.starts_with("tjxy_test_"));
            assert!(
                schema
                    .chars()
                    .all(|character| character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '_')
            );
        }
    }

    #[test]
    fn postgres_schema_url_preserves_existing_parameters() {
        assert_eq!(
            postgres_schema_database_url(
                "postgresql://postgres@localhost/tjxy?sslmode=disable",
                "tjxy_test_123",
            ),
            "postgresql://postgres@localhost/tjxy?sslmode=disable&options=-csearch_path%3Dtjxy_test_123"
        );
    }

    #[test]
    fn mysql_database_url_replaces_the_database_and_preserves_parameters() {
        assert_eq!(
            mysql_database_url(
                "mysql://root:secret@localhost:3306/tjxy?ssl-mode=DISABLED",
                "tjxy_test_123",
            )
            .unwrap(),
            "mysql://root:secret@localhost:3306/tjxy_test_123?ssl-mode=DISABLED"
        );
    }

    #[tokio::test]
    async fn sqlite_test_database_can_be_reconnected_without_losing_state() {
        let fixture = sqlite_test_database().await.unwrap();
        fixture
            .connection()
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "CREATE TABLE reconnect_check (value INTEGER NOT NULL)",
            ))
            .await
            .unwrap();
        fixture
            .connection()
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO reconnect_check (value) VALUES (17)",
            ))
            .await
            .unwrap();

        let reconnected = Database::connect(fixture.database_url()).await.unwrap();
        let value = reconnected
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT value FROM reconnect_check",
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get::<i64>("", "value")
            .unwrap();

        assert_eq!(value, 17);
    }

    #[tokio::test]
    async fn postgres_test_database_reconnects_to_the_same_isolated_schema() {
        let Ok(database_url) = env::var(TEST_DATABASE_URL) else {
            return;
        };
        if !database_url.starts_with("postgres://") && !database_url.starts_with("postgresql://") {
            return;
        }

        let fixture = reconnectable_test_database().await.unwrap();
        let first_schema = current_postgres_schema(fixture.connection()).await;
        let reconnected = Database::connect(fixture.database_url()).await.unwrap();
        let second_schema = current_postgres_schema(&reconnected).await;

        assert!(first_schema.starts_with("tjxy_test_"));
        assert_eq!(first_schema, second_schema);
    }

    async fn current_postgres_schema(database: &sea_orm::DatabaseConnection) -> String {
        database
            .query_one(Statement::from_string(
                DbBackend::Postgres,
                "SELECT current_schema() AS current_schema",
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get::<String>("", "current_schema")
            .unwrap()
    }
}
