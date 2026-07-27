use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr, QueryResult, TransactionTrait,
    sea_query::{
        Alias, Cond, Expr, ExprTrait, JoinType, LockType, OnConflict, Order, Query, SelectStatement,
    },
};
use thiserror::Error;
use tjxy_common::UserId;

const MAX_DEVICE_ROWS: u64 = 256;

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceRecord {
    device_id: String,
    device_name: String,
    custom_name: Option<String>,
    user_id: UserId,
    user_name: String,
    client_name: String,
    client_version: String,
    last_activity_at: DateTime<Utc>,
    playable_media_types: Vec<String>,
    supported_commands: Vec<String>,
    supports_media_control: bool,
    supports_persistent_identifier: bool,
    device_profile: Option<serde_json::Value>,
    app_store_url: Option<String>,
    icon_url: Option<String>,
}

impl DeviceRecord {
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    #[must_use]
    pub fn custom_name(&self) -> Option<&str> {
        self.custom_name.as_deref()
    }

    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    #[must_use]
    pub fn user_name(&self) -> &str {
        &self.user_name
    }

    #[must_use]
    pub fn client_name(&self) -> &str {
        &self.client_name
    }

    #[must_use]
    pub fn client_version(&self) -> &str {
        &self.client_version
    }

    #[must_use]
    pub const fn last_activity_at(&self) -> DateTime<Utc> {
        self.last_activity_at
    }

    #[must_use]
    pub fn playable_media_types(&self) -> &[String] {
        &self.playable_media_types
    }

    #[must_use]
    pub fn supported_commands(&self) -> &[String] {
        &self.supported_commands
    }

    #[must_use]
    pub const fn supports_media_control(&self) -> bool {
        self.supports_media_control
    }

    #[must_use]
    pub const fn supports_persistent_identifier(&self) -> bool {
        self.supports_persistent_identifier
    }

    #[must_use]
    pub fn device_profile(&self) -> Option<&serde_json::Value> {
        self.device_profile.as_ref()
    }

    #[must_use]
    pub fn app_store_url(&self) -> Option<&str> {
        self.app_store_url.as_deref()
    }

    #[must_use]
    pub fn icon_url(&self) -> Option<&str> {
        self.icon_url.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceOptionsRecord {
    id: i64,
    device_id: String,
    custom_name: Option<String>,
}

impl DeviceOptionsRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    #[must_use]
    pub fn custom_name(&self) -> Option<&str> {
        self.custom_name.as_deref()
    }
}

pub struct DeviceRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> DeviceRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Lists a bounded, latest-per-device view of active authentication sessions.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceRepositoryError`] when SQL or stored capability data is invalid.
    pub async fn list_active(
        &self,
        user_id: Option<UserId>,
        now: DateTime<Utc>,
    ) -> Result<Vec<DeviceRecord>, DeviceRepositoryError> {
        self.query_active(user_id, None, now).await
    }

    /// Returns the latest active record for one device identifier.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceRepositoryError`] when SQL or stored capability data is invalid.
    pub async fn device(
        &self,
        device_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<DeviceRecord>, DeviceRepositoryError> {
        Ok(self
            .query_active(None, Some(device_id), now)
            .await?
            .into_iter()
            .next())
    }

    /// Returns persisted custom options for one device.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceRepositoryError`] when SQL or row decoding fails.
    pub async fn options(
        &self,
        device_id: &str,
    ) -> Result<Option<DeviceOptionsRecord>, DeviceRepositoryError> {
        let device_key = device_key(device_id);
        let query = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("device_id"),
                Alias::new("custom_name"),
            ])
            .from(Alias::new("device_options"))
            .and_where(Expr::col(Alias::new("device_key")).eq(device_key))
            .limit(1)
            .to_owned();
        self.database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
            .as_ref()
            .map(device_options_from_row)
            .transpose()
    }

    /// Creates or replaces custom options when the device currently exists.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceRepositoryError`] when SQL rejects the update.
    pub async fn update_options(
        &self,
        device_id: &str,
        custom_name: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<bool, DeviceRepositoryError> {
        let transaction = self.database.begin().await?;
        let device_key = device_key(device_id);
        if !lock_active_device_sessions(&transaction, &device_key, now).await? {
            transaction.rollback().await?;
            return Ok(false);
        }
        let statement = Query::insert()
            .into_table(Alias::new("device_options"))
            .columns([
                Alias::new("device_key"),
                Alias::new("device_id"),
                Alias::new("custom_name"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                device_key.into(),
                device_id.into(),
                custom_name.into(),
                now.into(),
                now.into(),
            ])
            .on_conflict(
                OnConflict::column(Alias::new("device_key"))
                    .update_columns([
                        Alias::new("device_id"),
                        Alias::new("custom_name"),
                        Alias::new("updated_at"),
                    ])
                    .to_owned(),
            )
            .to_owned();
        transaction
            .execute(transaction.get_database_backend().build(&statement))
            .await?;
        transaction.commit().await?;
        Ok(true)
    }

    /// Atomically validates devices, revokes every matching session, and removes options.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceRepositoryError`] when SQL rejects the transaction.
    pub async fn delete_active(
        &self,
        device_ids: &[&str],
        now: DateTime<Utc>,
    ) -> Result<bool, DeviceRepositoryError> {
        if device_ids.is_empty() {
            return Ok(false);
        }
        let transaction = self.database.begin().await?;
        let mut device_keys = device_ids
            .iter()
            .map(|device_id| device_key(device_id))
            .collect::<Vec<_>>();
        device_keys.sort_unstable();
        device_keys.dedup();
        for device_key in &device_keys {
            if !lock_active_device_sessions(&transaction, device_key, now).await? {
                transaction.rollback().await?;
                return Ok(false);
            }
        }
        let backend = transaction.get_database_backend();
        let revoke_sessions = Query::update()
            .table(Alias::new("auth_sessions"))
            .value(Alias::new("revoked_at"), now)
            .value(Alias::new("revoke_reason"), "device_deleted")
            .and_where(Expr::col(Alias::new("device_key")).is_in(device_keys.clone()))
            .and_where(Expr::col(Alias::new("revoked_at")).is_null())
            .to_owned();
        transaction.execute(backend.build(&revoke_sessions)).await?;
        let delete_options = Query::delete()
            .from_table(Alias::new("device_options"))
            .and_where(Expr::col(Alias::new("device_key")).is_in(device_keys))
            .to_owned();
        transaction.execute(backend.build(&delete_options)).await?;
        transaction.commit().await?;
        Ok(true)
    }

    #[allow(clippy::too_many_lines)] // Keeps the correlated latest-per-device SQL aliases together.
    async fn query_active(
        &self,
        user_id: Option<UserId>,
        device_id: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Vec<DeviceRecord>, DeviceRepositoryError> {
        let sessions = Alias::new("device_session");
        let users = Alias::new("device_user");
        let options = Alias::new("device_option");
        let newer_sessions = Alias::new("newer_device_session");
        let newer_users = Alias::new("newer_device_user");
        let activity = Expr::col((sessions.clone(), Alias::new("last_seen_at")))
            .if_null(Expr::col((sessions.clone(), Alias::new("created_at"))));
        let newer_activity = Expr::col((newer_sessions.clone(), Alias::new("last_seen_at")))
            .if_null(Expr::col((
                newer_sessions.clone(),
                Alias::new("created_at"),
            )));
        let mut newer = Query::select();
        newer
            .expr(Expr::val(1))
            .from_as(Alias::new("auth_sessions"), newer_sessions.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("users"),
                newer_users.clone(),
                Expr::col((newer_users.clone(), Alias::new("id")))
                    .equals((newer_sessions.clone(), Alias::new("user_id"))),
            )
            .and_where(
                Expr::col((newer_sessions.clone(), Alias::new("device_key")))
                    .equals((sessions.clone(), Alias::new("device_key"))),
            )
            .and_where(Expr::col((newer_sessions.clone(), Alias::new("revoked_at"))).is_null())
            .cond_where(
                Cond::any()
                    .add(Expr::col((newer_sessions.clone(), Alias::new("expires_at"))).is_null())
                    .add(Expr::col((newer_sessions.clone(), Alias::new("expires_at"))).gt(now)),
            )
            .and_where(
                Expr::col((newer_sessions.clone(), Alias::new("auth_revision")))
                    .equals((newer_users.clone(), Alias::new("auth_revision"))),
            )
            .and_where(Expr::col((newer_users.clone(), Alias::new("disabled_at"))).is_null())
            .cond_where(
                Cond::any()
                    .add(newer_activity.clone().gt(activity.clone()))
                    .add(
                        Cond::all().add(newer_activity.eq(activity.clone())).add(
                            Expr::col((newer_sessions.clone(), Alias::new("id")))
                                .gt(Expr::col((sessions.clone(), Alias::new("id")))),
                        ),
                    ),
            );
        if let Some(user_id) = user_id {
            newer.and_where(
                Expr::col((newer_sessions.clone(), Alias::new("user_id"))).eq(user_id.as_uuid()),
            );
        }
        let mut query = Query::select();
        query
            .columns([
                (sessions.clone(), Alias::new("device_id")),
                (sessions.clone(), Alias::new("device_name")),
                (sessions.clone(), Alias::new("client_name")),
                (sessions.clone(), Alias::new("client_version")),
                (sessions.clone(), Alias::new("created_at")),
                (sessions.clone(), Alias::new("last_seen_at")),
                (sessions.clone(), Alias::new("playable_media_types")),
                (sessions.clone(), Alias::new("supported_commands")),
                (sessions.clone(), Alias::new("supports_media_control")),
                (
                    sessions.clone(),
                    Alias::new("supports_persistent_identifier"),
                ),
                (sessions.clone(), Alias::new("device_profile")),
                (sessions.clone(), Alias::new("app_store_url")),
                (sessions.clone(), Alias::new("icon_url")),
                (users.clone(), Alias::new("username")),
                (options.clone(), Alias::new("custom_name")),
            ])
            .expr_as(
                Expr::col((users.clone(), Alias::new("id"))),
                Alias::new("device_user_id"),
            )
            .from_as(Alias::new("auth_sessions"), sessions.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("users"),
                users.clone(),
                Expr::col((users.clone(), Alias::new("id")))
                    .equals((sessions.clone(), Alias::new("user_id"))),
            )
            .join_as(
                JoinType::LeftJoin,
                Alias::new("device_options"),
                options.clone(),
                Expr::col((sessions.clone(), Alias::new("device_key")))
                    .equals((options, Alias::new("device_key"))),
            )
            .and_where(Expr::col((sessions.clone(), Alias::new("revoked_at"))).is_null())
            .and_where(
                Expr::col((sessions.clone(), Alias::new("expires_at")))
                    .is_null()
                    .or(Expr::col((sessions.clone(), Alias::new("expires_at"))).gt(now)),
            )
            .and_where(
                Expr::col((sessions.clone(), Alias::new("auth_revision")))
                    .equals((users.clone(), Alias::new("auth_revision"))),
            )
            .and_where(Expr::col((users.clone(), Alias::new("disabled_at"))).is_null())
            .and_where(Expr::exists(newer).not())
            .order_by_expr(activity, Order::Desc)
            .order_by((sessions.clone(), Alias::new("id")), Order::Desc)
            .limit(MAX_DEVICE_ROWS);
        if let Some(user_id) = user_id {
            query.and_where(
                Expr::col((sessions.clone(), Alias::new("user_id"))).eq(user_id.as_uuid()),
            );
        }
        if let Some(device_id) = device_id {
            query.and_where(
                Expr::col((sessions, Alias::new("device_key"))).eq(device_key(device_id)),
            );
        }
        let rows = self
            .database
            .query_all(self.database.get_database_backend().build(&query))
            .await?;
        rows.iter().map(device_from_row).collect()
    }
}

#[derive(Debug, Error)]
pub enum DeviceRepositoryError {
    #[error("stored device capabilities are invalid")]
    InvalidStoredCapabilities,
    #[error("device repository operation failed: {0}")]
    Database(#[from] DbErr),
}

fn device_from_row(row: &QueryResult) -> Result<DeviceRecord, DeviceRepositoryError> {
    let created_at: DateTime<Utc> = row.try_get("", "created_at")?;
    let last_seen_at: Option<DateTime<Utc>> = row.try_get("", "last_seen_at")?;
    Ok(DeviceRecord {
        device_id: row.try_get("", "device_id")?,
        device_name: row.try_get("", "device_name")?,
        custom_name: row.try_get("", "custom_name")?,
        user_id: UserId::from_uuid(row.try_get("", "device_user_id")?),
        user_name: row.try_get("", "username")?,
        client_name: row.try_get("", "client_name")?,
        client_version: row.try_get("", "client_version")?,
        last_activity_at: last_seen_at.unwrap_or(created_at),
        playable_media_types: string_array(row, "playable_media_types")?,
        supported_commands: string_array(row, "supported_commands")?,
        supports_media_control: row.try_get("", "supports_media_control")?,
        supports_persistent_identifier: row.try_get("", "supports_persistent_identifier")?,
        device_profile: row.try_get("", "device_profile")?,
        app_store_url: row.try_get("", "app_store_url")?,
        icon_url: row.try_get("", "icon_url")?,
    })
}

fn device_options_from_row(
    row: &QueryResult,
) -> Result<DeviceOptionsRecord, DeviceRepositoryError> {
    Ok(DeviceOptionsRecord {
        id: row.try_get("", "id")?,
        device_id: row.try_get("", "device_id")?,
        custom_name: row.try_get("", "custom_name")?,
    })
}

fn string_array(row: &QueryResult, column: &str) -> Result<Vec<String>, DeviceRepositoryError> {
    match row.try_get::<Option<serde_json::Value>>("", column)? {
        None => Ok(Vec::new()),
        Some(serde_json::Value::Array(values)) => values
            .into_iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or(DeviceRepositoryError::InvalidStoredCapabilities)
            })
            .collect(),
        Some(_) => Err(DeviceRepositoryError::InvalidStoredCapabilities),
    }
}

async fn lock_active_device_sessions<Connection>(
    connection: &Connection,
    device_key: &str,
    now: DateTime<Utc>,
) -> Result<bool, DeviceRepositoryError>
where
    Connection: ConnectionTrait,
{
    let query = locked_sessions_query(device_key, now);
    let rows = connection
        .query_all(connection.get_database_backend().build(&query))
        .await?;
    Ok(!rows.is_empty())
}

fn locked_sessions_query(device_key: &str, now: DateTime<Utc>) -> SelectStatement {
    let sessions = Alias::new("delete_device_session");
    let users = Alias::new("delete_device_user");
    Query::select()
        .expr(Expr::col((sessions.clone(), Alias::new("id"))))
        .from_as(Alias::new("auth_sessions"), sessions.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("users"),
            users.clone(),
            Expr::col((users.clone(), Alias::new("id")))
                .equals((sessions.clone(), Alias::new("user_id"))),
        )
        .and_where(Expr::col((sessions.clone(), Alias::new("device_key"))).eq(device_key))
        .and_where(Expr::col((sessions.clone(), Alias::new("revoked_at"))).is_null())
        .cond_where(
            Cond::any()
                .add(Expr::col((sessions.clone(), Alias::new("expires_at"))).is_null())
                .add(Expr::col((sessions.clone(), Alias::new("expires_at"))).gt(now)),
        )
        .and_where(
            Expr::col((sessions, Alias::new("auth_revision")))
                .equals((users.clone(), Alias::new("auth_revision"))),
        )
        .and_where(Expr::col((users, Alias::new("disabled_at"))).is_null())
        .lock(LockType::Update)
        .to_owned()
}

fn device_key(device_id: &str) -> String {
    crate::natural_key::hash(&["device", device_id])
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use sea_orm::sea_query::{MysqlQueryBuilder, PostgresQueryBuilder, SqliteQueryBuilder};

    use super::locked_sessions_query;

    #[test]
    fn device_mutations_lock_active_sessions_on_locking_backends() {
        let query = locked_sessions_query(
            "exact-device-key",
            Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap(),
        );
        assert!(
            query
                .to_string(PostgresQueryBuilder)
                .ends_with("FOR UPDATE")
        );
        assert!(query.to_string(MysqlQueryBuilder).ends_with("FOR UPDATE"));
        assert!(!query.to_string(SqliteQueryBuilder).contains("FOR UPDATE"));
    }
}
