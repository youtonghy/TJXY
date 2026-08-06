use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Condition, Expr, JoinType, OnConflict, Order, Query, SelectStatement},
};
use thiserror::Error;
use tjxy_common::UserId;
use uuid::Uuid;

const MAX_PAGE_SIZE: u64 = 100;
const MAX_OFFSET: u64 = 100_000;
const MAX_TITLE_CHARS: usize = 200;
const MAX_BODY_CHARS: usize = 32_000;
const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnouncementKind {
    Popup,
    Standard,
}

impl AnnouncementKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Popup => "popup",
            Self::Standard => "standard",
        }
    }

    fn parse(value: &str) -> Result<Self, AnnouncementRepositoryError> {
        match value {
            "popup" => Ok(Self::Popup),
            "standard" => Ok(Self::Standard),
            _ => Err(AnnouncementRepositoryError::InvalidStoredValue),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnouncementStatus {
    Draft,
    Published,
    Archived,
}

impl AnnouncementStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }

    fn parse(value: &str) -> Result<Self, AnnouncementRepositoryError> {
        match value {
            "draft" => Ok(Self::Draft),
            "published" => Ok(Self::Published),
            "archived" => Ok(Self::Archived),
            _ => Err(AnnouncementRepositoryError::InvalidStoredValue),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnouncementDraftInput {
    title: String,
    body_markdown: String,
    kind: AnnouncementKind,
}

impl AnnouncementDraftInput {
    #[must_use]
    pub fn new(
        title: impl Into<String>,
        body_markdown: impl Into<String>,
        kind: AnnouncementKind,
    ) -> Self {
        Self {
            title: title.into(),
            body_markdown: body_markdown.into(),
            kind,
        }
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn body_markdown(&self) -> &str {
        &self.body_markdown
    }

    #[must_use]
    pub const fn kind(&self) -> AnnouncementKind {
        self.kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnouncementRecord {
    id: Uuid,
    title: String,
    body_markdown: String,
    kind: AnnouncementKind,
    status: AnnouncementStatus,
    content_version: i64,
    revision: i64,
    published_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AnnouncementRecord {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }
    #[must_use]
    pub fn body_markdown(&self) -> &str {
        &self.body_markdown
    }
    #[must_use]
    pub const fn kind(&self) -> AnnouncementKind {
        self.kind
    }
    #[must_use]
    pub const fn status(&self) -> AnnouncementStatus {
        self.status
    }
    #[must_use]
    pub const fn content_version(&self) -> i64 {
        self.content_version
    }
    #[must_use]
    pub const fn revision(&self) -> i64 {
        self.revision
    }
    #[must_use]
    pub const fn published_at(&self) -> Option<DateTime<Utc>> {
        self.published_at
    }
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnouncementView {
    record: AnnouncementRecord,
    is_read: bool,
}

impl AnnouncementView {
    #[must_use]
    pub const fn record(&self) -> &AnnouncementRecord {
        &self.record
    }
    #[must_use]
    pub const fn is_read(&self) -> bool {
        self.is_read
    }
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.record.id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnouncementList<T> {
    items: Vec<T>,
    total: u64,
    unread_count: u64,
}

impl<T> AnnouncementList<T> {
    #[must_use]
    pub fn items(&self) -> &[T] {
        &self.items
    }
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.total
    }
    #[must_use]
    pub const fn unread_count(&self) -> u64 {
        self.unread_count
    }
}

pub struct AnnouncementRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> AnnouncementRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Creates an unpublished announcement.
    ///
    /// # Errors
    ///
    /// Returns an input validation error for invalid content, or a database error when the draft
    /// cannot be persisted or read back.
    pub async fn create_draft(
        &self,
        input: &AnnouncementDraftInput,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        validate_input(input)?;
        let now = Utc::now();
        let id = Uuid::new_v4();
        let statement = Query::insert()
            .into_table(Alias::new("announcements"))
            .columns([
                Alias::new("id"),
                Alias::new("title"),
                Alias::new("body_markdown"),
                Alias::new("kind"),
                Alias::new("status"),
                Alias::new("content_version"),
                Alias::new("revision"),
                Alias::new("published_at"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                id.into(),
                input.title.clone().into(),
                input.body_markdown.clone().into(),
                input.kind.as_str().into(),
                AnnouncementStatus::Draft.as_str().into(),
                0_i64.into(),
                1_i64.into(),
                Option::<DateTime<Utc>>::None.into(),
                now.into(),
                now.into(),
            ])
            .to_owned();
        self.database
            .execute(self.database.get_database_backend().build(&statement))
            .await?;
        get_on(self.database, id)
            .await?
            .ok_or(AnnouncementRepositoryError::NotFound)
    }

    /// Lists announcements for administrators with bounded pagination.
    ///
    /// # Errors
    ///
    /// Returns an input validation error for an invalid page, a stored-value error for malformed
    /// persisted data, or a database error when the query fails.
    pub async fn admin_page(
        &self,
        status: Option<AnnouncementStatus>,
        kind: Option<AnnouncementKind>,
        offset: u64,
        limit: u64,
    ) -> Result<AnnouncementList<AnnouncementRecord>, AnnouncementRepositoryError> {
        validate_page(offset, limit)?;
        let total = count_admin(self.database, status, kind).await?;
        let mut query = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("title"),
                Alias::new("body_markdown"),
                Alias::new("kind"),
                Alias::new("status"),
                Alias::new("content_version"),
                Alias::new("revision"),
                Alias::new("published_at"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .from(Alias::new("announcements"))
            .to_owned();
        apply_admin_filters(&mut query, status, kind);
        query
            .order_by(Alias::new("updated_at"), Order::Desc)
            .order_by(Alias::new("id"), Order::Desc)
            .offset(offset)
            .limit(limit);
        let rows = self
            .database
            .query_all(self.database.get_database_backend().build(&query))
            .await?;
        let items = rows
            .iter()
            .map(record_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AnnouncementList {
            items,
            total,
            unread_count: 0,
        })
    }

    /// Loads one announcement for administrative use.
    ///
    /// # Errors
    ///
    /// Returns [`AnnouncementRepositoryError::NotFound`] when the ID does not exist, a stored-value
    /// error for malformed persisted data, or a database error when the query fails.
    pub async fn get_admin(
        &self,
        id: Uuid,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        get_on(self.database, id)
            .await?
            .ok_or(AnnouncementRepositoryError::NotFound)
    }

    /// Updates announcement content using optimistic concurrency.
    ///
    /// # Errors
    ///
    /// Returns an input or revision error for invalid values, a conflict when the expected revision
    /// is stale, not found for an unknown ID, or a database error when the transaction fails.
    pub async fn update(
        &self,
        id: Uuid,
        input: &AnnouncementDraftInput,
        expected_revision: i64,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        validate_input(input)?;
        let transaction = self.database.begin().await?;
        let result = update_on(&transaction, id, input, expected_revision).await;
        finish(transaction, result).await
    }

    /// Publishes an announcement and advances its content version.
    ///
    /// # Errors
    ///
    /// Returns a revision or transition error for invalid state, a conflict for a stale revision,
    /// not found for an unknown ID, or a database error when the transaction fails.
    pub async fn publish(
        &self,
        id: Uuid,
        expected_revision: i64,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = transition_on(
            &transaction,
            id,
            expected_revision,
            AnnouncementStatus::Published,
        )
        .await;
        finish(transaction, result).await
    }

    /// Archives a published or draft announcement.
    ///
    /// # Errors
    ///
    /// Returns a revision or transition error for invalid state, a conflict for a stale revision,
    /// not found for an unknown ID, or a database error when the transaction fails.
    pub async fn archive(
        &self,
        id: Uuid,
        expected_revision: i64,
    ) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = transition_on(
            &transaction,
            id,
            expected_revision,
            AnnouncementStatus::Archived,
        )
        .await;
        finish(transaction, result).await
    }

    /// Permanently deletes an announcement at an expected revision.
    ///
    /// # Errors
    ///
    /// Returns a revision error for an invalid value, a conflict when no row matches the expected
    /// revision, or a database error when deletion fails.
    pub async fn delete(
        &self,
        id: Uuid,
        expected_revision: i64,
    ) -> Result<(), AnnouncementRepositoryError> {
        if expected_revision <= 0 {
            return Err(AnnouncementRepositoryError::InvalidRevision);
        }
        let statement = Query::delete()
            .from_table(Alias::new("announcements"))
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .and_where(Expr::col(Alias::new("revision")).eq(expected_revision))
            .to_owned();
        let result = self
            .database
            .execute(self.database.get_database_backend().build(&statement))
            .await?;
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(AnnouncementRepositoryError::RevisionConflict)
        }
    }

    /// Lists published announcements with read state for one user.
    ///
    /// # Errors
    ///
    /// Returns an input validation error for an invalid page, a stored-value error for malformed
    /// persisted data, or a database error when the query fails.
    pub async fn visible_page(
        &self,
        user_id: UserId,
        limit: u64,
        offset: u64,
    ) -> Result<AnnouncementList<AnnouncementView>, AnnouncementRepositoryError> {
        validate_page(offset, limit)?;
        let total = count_published(self.database).await?;
        let unread_count = count_unread(self.database, user_id).await?;
        let rows = visible_rows(self.database, user_id, limit, offset, false).await?;
        let items = rows
            .iter()
            .map(view_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AnnouncementList {
            items,
            total,
            unread_count,
        })
    }

    /// Returns the oldest unread popup announcement for one user.
    ///
    /// # Errors
    ///
    /// Returns a stored-value error for malformed persisted data or a database error when the query
    /// fails.
    pub async fn next_popup(
        &self,
        user_id: UserId,
    ) -> Result<Option<AnnouncementView>, AnnouncementRepositoryError> {
        let rows = visible_rows(self.database, user_id, 1, 0, true).await?;
        rows.first().map(view_from_row).transpose()
    }

    /// Records that a user acknowledged the current version of a published announcement.
    ///
    /// # Errors
    ///
    /// Returns not found for an unknown ID, stale version when the announcement is unpublished or
    /// its version changed, or a database error when the receipt transaction fails.
    pub async fn acknowledge(
        &self,
        user_id: UserId,
        id: Uuid,
        content_version: i64,
    ) -> Result<(), AnnouncementRepositoryError> {
        if content_version <= 0 {
            return Err(AnnouncementRepositoryError::StaleVersion);
        }
        let transaction = self.database.begin().await?;
        let result = async {
            let announcement = get_on(&transaction, id)
                .await?
                .ok_or(AnnouncementRepositoryError::NotFound)?;
            if announcement.status != AnnouncementStatus::Published
                || announcement.content_version != content_version
            {
                return Err(AnnouncementRepositoryError::StaleVersion);
            }
            let now = Utc::now();
            let statement = Query::insert()
                .into_table(Alias::new("user_announcement_receipts"))
                .columns([
                    Alias::new("id"),
                    Alias::new("announcement_id"),
                    Alias::new("user_id"),
                    Alias::new("acknowledged_version"),
                    Alias::new("acknowledged_at"),
                ])
                .values_panic([
                    Uuid::new_v4().into(),
                    id.into(),
                    user_id.as_uuid().into(),
                    content_version.into(),
                    now.into(),
                ])
                .on_conflict(
                    OnConflict::columns([Alias::new("announcement_id"), Alias::new("user_id")])
                        .update_columns([
                            Alias::new("acknowledged_version"),
                            Alias::new("acknowledged_at"),
                        ])
                        .to_owned(),
                )
                .to_owned();
            transaction
                .execute(transaction.get_database_backend().build(&statement))
                .await?;
            Ok(())
        }
        .await;
        finish(transaction, result).await
    }
}

async fn update_on(
    transaction: &DatabaseTransaction,
    id: Uuid,
    input: &AnnouncementDraftInput,
    expected_revision: i64,
) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
    if expected_revision <= 0 {
        return Err(AnnouncementRepositoryError::InvalidRevision);
    }
    let current = get_on(transaction, id)
        .await?
        .ok_or(AnnouncementRepositoryError::NotFound)?;
    if current.revision != expected_revision {
        return Err(AnnouncementRepositoryError::RevisionConflict);
    }
    let changed = current.title != input.title
        || current.body_markdown != input.body_markdown
        || current.kind != input.kind;
    let version = if current.status == AnnouncementStatus::Published && changed {
        current
            .content_version
            .checked_add(1)
            .ok_or(AnnouncementRepositoryError::InvalidRevision)?
    } else {
        current.content_version
    };
    let published_at = if current.status == AnnouncementStatus::Published && changed {
        Some(Utc::now())
    } else {
        current.published_at
    };
    let revision = expected_revision
        .checked_add(1)
        .ok_or(AnnouncementRepositoryError::InvalidRevision)?;
    let statement = Query::update()
        .table(Alias::new("announcements"))
        .values([
            (Alias::new("title"), input.title.clone().into()),
            (
                Alias::new("body_markdown"),
                input.body_markdown.clone().into(),
            ),
            (Alias::new("kind"), input.kind.as_str().into()),
            (Alias::new("content_version"), version.into()),
            (Alias::new("published_at"), published_at.into()),
            (Alias::new("revision"), revision.into()),
            (Alias::new("updated_at"), Utc::now().into()),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("revision")).eq(expected_revision))
        .to_owned();
    if transaction
        .execute(transaction.get_database_backend().build(&statement))
        .await?
        .rows_affected()
        != 1
    {
        return Err(AnnouncementRepositoryError::RevisionConflict);
    }
    get_on(transaction, id)
        .await?
        .ok_or(AnnouncementRepositoryError::NotFound)
}

async fn transition_on(
    transaction: &DatabaseTransaction,
    id: Uuid,
    expected_revision: i64,
    target: AnnouncementStatus,
) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
    if expected_revision <= 0 {
        return Err(AnnouncementRepositoryError::InvalidRevision);
    }
    let current = get_on(transaction, id)
        .await?
        .ok_or(AnnouncementRepositoryError::NotFound)?;
    if current.revision != expected_revision {
        return Err(AnnouncementRepositoryError::RevisionConflict);
    }
    if target == AnnouncementStatus::Published && current.status == AnnouncementStatus::Published {
        return Err(AnnouncementRepositoryError::InvalidTransition);
    }
    if target == AnnouncementStatus::Archived && current.status == AnnouncementStatus::Archived {
        return Err(AnnouncementRepositoryError::InvalidTransition);
    }
    let revision = expected_revision
        .checked_add(1)
        .ok_or(AnnouncementRepositoryError::InvalidRevision)?;
    let version = if target == AnnouncementStatus::Published {
        current
            .content_version
            .checked_add(1)
            .ok_or(AnnouncementRepositoryError::InvalidRevision)?
    } else {
        current.content_version
    };
    let published_at = (target == AnnouncementStatus::Published).then_some(Utc::now());
    let statement = Query::update()
        .table(Alias::new("announcements"))
        .values([
            (Alias::new("status"), target.as_str().into()),
            (Alias::new("content_version"), version.into()),
            (Alias::new("published_at"), published_at.into()),
            (Alias::new("revision"), revision.into()),
            (Alias::new("updated_at"), Utc::now().into()),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("revision")).eq(expected_revision))
        .to_owned();
    if transaction
        .execute(transaction.get_database_backend().build(&statement))
        .await?
        .rows_affected()
        != 1
    {
        return Err(AnnouncementRepositoryError::RevisionConflict);
    }
    get_on(transaction, id)
        .await?
        .ok_or(AnnouncementRepositoryError::NotFound)
}

async fn get_on<C: ConnectionTrait>(
    connection: &C,
    id: Uuid,
) -> Result<Option<AnnouncementRecord>, AnnouncementRepositoryError> {
    let statement = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("title"),
            Alias::new("body_markdown"),
            Alias::new("kind"),
            Alias::new("status"),
            Alias::new("content_version"),
            Alias::new("revision"),
            Alias::new("published_at"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .from(Alias::new("announcements"))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .to_owned();
    let row = connection
        .query_one(connection.get_database_backend().build(&statement))
        .await?;
    row.map(|value| record_from_row(&value)).transpose()
}

async fn count_admin<C: ConnectionTrait>(
    connection: &C,
    status: Option<AnnouncementStatus>,
    kind: Option<AnnouncementKind>,
) -> Result<u64, AnnouncementRepositoryError> {
    let mut query = Query::select()
        .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
        .from(Alias::new("announcements"))
        .to_owned();
    apply_admin_filters(&mut query, status, kind);
    count_query(connection, query).await
}

async fn count_published<C: ConnectionTrait>(
    connection: &C,
) -> Result<u64, AnnouncementRepositoryError> {
    let query = Query::select()
        .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
        .from(Alias::new("announcements"))
        .and_where(Expr::col(Alias::new("status")).eq(AnnouncementStatus::Published.as_str()))
        .to_owned();
    count_query(connection, query).await
}

async fn count_unread<C: ConnectionTrait>(
    connection: &C,
    user_id: UserId,
) -> Result<u64, AnnouncementRepositoryError> {
    let a = Alias::new("a");
    let r = Alias::new("r");
    let query = Query::select()
        .expr_as(
            Expr::col((a.clone(), Alias::new("id"))).count(),
            Alias::new("count"),
        )
        .from_as(Alias::new("announcements"), a.clone())
        .join_as(
            JoinType::LeftJoin,
            Alias::new("user_announcement_receipts"),
            r.clone(),
            Condition::all()
                .add(
                    Expr::col((r.clone(), Alias::new("announcement_id")))
                        .equals((a.clone(), Alias::new("id"))),
                )
                .add(Expr::col((r.clone(), Alias::new("user_id"))).eq(user_id.as_uuid())),
        )
        .and_where(
            Expr::col((a.clone(), Alias::new("status"))).eq(AnnouncementStatus::Published.as_str()),
        )
        .cond_where(
            Condition::any()
                .add(Expr::col((r.clone(), Alias::new("id"))).is_null())
                .add(
                    Expr::col((r, Alias::new("acknowledged_version")))
                        .lt(Expr::col((a, Alias::new("content_version")))),
                ),
        )
        .to_owned();
    count_query(connection, query).await
}

async fn visible_rows<C: ConnectionTrait>(
    connection: &C,
    user_id: UserId,
    limit: u64,
    offset: u64,
    popup_only: bool,
) -> Result<Vec<QueryResult>, AnnouncementRepositoryError> {
    let a = Alias::new("a");
    let r = Alias::new("r");
    let mut query = Query::select()
        .columns([
            (a.clone(), Alias::new("id")),
            (a.clone(), Alias::new("title")),
            (a.clone(), Alias::new("body_markdown")),
            (a.clone(), Alias::new("kind")),
            (a.clone(), Alias::new("status")),
            (a.clone(), Alias::new("content_version")),
            (a.clone(), Alias::new("revision")),
            (a.clone(), Alias::new("published_at")),
            (a.clone(), Alias::new("created_at")),
            (a.clone(), Alias::new("updated_at")),
            (r.clone(), Alias::new("acknowledged_version")),
        ])
        .from_as(Alias::new("announcements"), a.clone())
        .join_as(
            JoinType::LeftJoin,
            Alias::new("user_announcement_receipts"),
            r.clone(),
            Condition::all()
                .add(
                    Expr::col((r.clone(), Alias::new("announcement_id")))
                        .equals((a.clone(), Alias::new("id"))),
                )
                .add(Expr::col((r.clone(), Alias::new("user_id"))).eq(user_id.as_uuid())),
        )
        .and_where(
            Expr::col((a.clone(), Alias::new("status"))).eq(AnnouncementStatus::Published.as_str()),
        )
        .and_where(Expr::col((a.clone(), Alias::new("published_at"))).is_not_null())
        .to_owned();
    if popup_only {
        query
            .and_where(
                Expr::col((a.clone(), Alias::new("kind"))).eq(AnnouncementKind::Popup.as_str()),
            )
            .cond_where(
                Condition::any()
                    .add(Expr::col((r.clone(), Alias::new("id"))).is_null())
                    .add(
                        Expr::col((r.clone(), Alias::new("acknowledged_version")))
                            .lt(Expr::col((a.clone(), Alias::new("content_version")))),
                    ),
            );
    }
    query
        .order_by(
            (a.clone(), Alias::new("published_at")),
            if popup_only { Order::Asc } else { Order::Desc },
        )
        .order_by(
            (a, Alias::new("id")),
            if popup_only { Order::Asc } else { Order::Desc },
        )
        .offset(offset)
        .limit(limit);
    Ok(connection
        .query_all(connection.get_database_backend().build(&query))
        .await?)
}

fn apply_admin_filters(
    query: &mut SelectStatement,
    status: Option<AnnouncementStatus>,
    kind: Option<AnnouncementKind>,
) {
    if let Some(status) = status {
        query.and_where(Expr::col(Alias::new("status")).eq(status.as_str()));
    }
    if let Some(kind) = kind {
        query.and_where(Expr::col(Alias::new("kind")).eq(kind.as_str()));
    }
}

async fn count_query<C: ConnectionTrait>(
    connection: &C,
    query: SelectStatement,
) -> Result<u64, AnnouncementRepositoryError> {
    let row = connection
        .query_one(connection.get_database_backend().build(&query))
        .await?
        .ok_or(AnnouncementRepositoryError::MissingAggregate)?;
    let count: i64 = row.try_get("", "count")?;
    u64::try_from(count).map_err(|_| AnnouncementRepositoryError::InvalidStoredValue)
}

fn record_from_row(row: &QueryResult) -> Result<AnnouncementRecord, AnnouncementRepositoryError> {
    let content_version: i64 = row.try_get("", "content_version")?;
    let revision: i64 = row.try_get("", "revision")?;
    if content_version < 0 || revision <= 0 {
        return Err(AnnouncementRepositoryError::InvalidStoredValue);
    }
    Ok(AnnouncementRecord {
        id: row.try_get("", "id")?,
        title: row.try_get("", "title")?,
        body_markdown: row.try_get("", "body_markdown")?,
        kind: AnnouncementKind::parse(&row.try_get::<String>("", "kind")?)?,
        status: AnnouncementStatus::parse(&row.try_get::<String>("", "status")?)?,
        content_version,
        revision,
        published_at: row.try_get("", "published_at")?,
        created_at: row.try_get("", "created_at")?,
        updated_at: row.try_get("", "updated_at")?,
    })
}

fn view_from_row(row: &QueryResult) -> Result<AnnouncementView, AnnouncementRepositoryError> {
    let record = record_from_row(row)?;
    let acknowledged_version: Option<i64> = row.try_get("", "acknowledged_version")?;
    Ok(AnnouncementView {
        is_read: acknowledged_version.is_some_and(|version| version >= record.content_version),
        record,
    })
}

fn validate_input(input: &AnnouncementDraftInput) -> Result<(), AnnouncementRepositoryError> {
    if input.title.trim().is_empty()
        || input.title.chars().count() > MAX_TITLE_CHARS
        || input.title.chars().any(char::is_control)
    {
        return Err(AnnouncementRepositoryError::InvalidInput);
    }
    if input.body_markdown.trim().is_empty()
        || input.body_markdown.chars().count() > MAX_BODY_CHARS
        || input.body_markdown.len() > MAX_BODY_BYTES
        || input
            .body_markdown
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(AnnouncementRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_page(offset: u64, limit: u64) -> Result<(), AnnouncementRepositoryError> {
    if limit == 0 || limit > MAX_PAGE_SIZE || offset > MAX_OFFSET {
        Err(AnnouncementRepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, AnnouncementRepositoryError>,
) -> Result<T, AnnouncementRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(AnnouncementRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

#[derive(Debug, Error)]
pub enum AnnouncementRepositoryError {
    #[error("announcement input is invalid")]
    InvalidInput,
    #[error("announcement revision is invalid")]
    InvalidRevision,
    #[error("announcement revision conflict")]
    RevisionConflict,
    #[error("announcement transition is invalid")]
    InvalidTransition,
    #[error("announcement version is stale")]
    StaleVersion,
    #[error("announcement was not found")]
    NotFound,
    #[error("stored announcement value is invalid")]
    InvalidStoredValue,
    #[error("announcement aggregate row is missing")]
    MissingAggregate,
    #[error("announcement rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
    #[error("announcement database operation failed: {0}")]
    Database(#[from] DbErr),
}
