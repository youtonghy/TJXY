use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, JoinType, Order, Query},
};
use thiserror::Error;
use tjxy_common::{CatalogItemId, UserId};
use uuid::Uuid;

use crate::catalog_query::lock_catalog_item_visibility;

const PLAYLIST: &str = "Playlist";
const COLLECTION: &str = "Collection";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaCollectionKind {
    Playlist,
    Collection,
}

impl MediaCollectionKind {
    const fn as_database_value(self) -> &'static str {
        match self {
            Self::Playlist => PLAYLIST,
            Self::Collection => COLLECTION,
        }
    }

    fn from_database_value(value: &str) -> Result<Self, MediaCollectionRepositoryError> {
        match value {
            PLAYLIST => Ok(Self::Playlist),
            COLLECTION => Ok(Self::Collection),
            _ => Err(MediaCollectionRepositoryError::InvalidStoredKind),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaCollectionRecord {
    id: Uuid,
    kind: MediaCollectionKind,
    owner_user_id: Option<UserId>,
    name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl MediaCollectionRecord {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn kind(&self) -> MediaCollectionKind {
        self.kind
    }

    #[must_use]
    pub const fn owner_user_id(&self) -> Option<UserId> {
        self.owner_user_id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
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
pub struct MediaCollectionCatalogItem {
    id: CatalogItemId,
    name: String,
    item_type: String,
}

impl MediaCollectionCatalogItem {
    #[must_use]
    pub const fn id(&self) -> CatalogItemId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn item_type(&self) -> &str {
        &self.item_type
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaCollectionEntry {
    id: Uuid,
    position: i64,
    item: MediaCollectionCatalogItem,
}

impl MediaCollectionEntry {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn position(&self) -> i64 {
        self.position
    }

    #[must_use]
    pub const fn item(&self) -> &MediaCollectionCatalogItem {
        &self.item
    }
}

pub struct MediaCollectionRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> MediaCollectionRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Creates a user-owned Playlist.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] for an invalid name or a database failure.
    pub async fn create_playlist(
        &self,
        owner: UserId,
        name: &str,
    ) -> Result<MediaCollectionRecord, MediaCollectionRepositoryError> {
        create_collection(
            self.database,
            MediaCollectionKind::Playlist,
            Some(owner),
            name,
        )
        .await
    }

    /// Creates an administrator-managed shared Collection.
    ///
    /// Authorization for this operation belongs to the application boundary.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] for an invalid name or a database failure.
    pub async fn create_shared_collection(
        &self,
        name: &str,
    ) -> Result<MediaCollectionRecord, MediaCollectionRepositoryError> {
        create_collection(self.database, MediaCollectionKind::Collection, None, name).await
    }

    /// Lists Playlists owned by the requesting user in a stable order.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] when SQL fails or stored data is malformed.
    pub async fn playlists(
        &self,
        owner: UserId,
    ) -> Result<Vec<MediaCollectionRecord>, MediaCollectionRepositoryError> {
        read_collections(self.database, MediaCollectionKind::Playlist, Some(owner)).await
    }

    /// Lists administrator-managed shared Collections in a stable order.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] when SQL fails or stored data is malformed.
    pub async fn shared_collections(
        &self,
    ) -> Result<Vec<MediaCollectionRecord>, MediaCollectionRepositoryError> {
        read_collections(self.database, MediaCollectionKind::Collection, None).await
    }

    /// Renames one Playlist owned by the requesting user.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] for an invalid name, inaccessible Playlist,
    /// or database failure.
    pub async fn rename_playlist(
        &self,
        owner: UserId,
        playlist_id: Uuid,
        name: &str,
    ) -> Result<MediaCollectionRecord, MediaCollectionRepositoryError> {
        rename_owned_playlist(self.database, owner, playlist_id, name).await
    }

    /// Renames one administrator-managed shared Collection.
    ///
    /// Authorization for this operation belongs to the application boundary.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] for an invalid name, unknown Collection,
    /// or database failure.
    pub async fn rename_shared_collection(
        &self,
        collection_id: Uuid,
        name: &str,
    ) -> Result<MediaCollectionRecord, MediaCollectionRepositoryError> {
        rename_shared_collection(self.database, collection_id, name).await
    }

    /// Deletes one Playlist owned by the requesting user.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] for an inaccessible Playlist or database failure.
    pub async fn delete_playlist(
        &self,
        owner: UserId,
        playlist_id: Uuid,
    ) -> Result<(), MediaCollectionRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = async {
            ensure_owned_playlist(&transaction, owner, playlist_id).await?;
            delete_collection(&transaction, playlist_id).await
        }
        .await;
        finish(transaction, result).await
    }

    /// Deletes one administrator-managed shared Collection.
    ///
    /// Authorization for this operation belongs to the application boundary.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] for an unknown Collection or database failure.
    pub async fn delete_shared_collection(
        &self,
        collection_id: Uuid,
    ) -> Result<(), MediaCollectionRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = async {
            ensure_shared_collection(&transaction, collection_id).await?;
            delete_collection(&transaction, collection_id).await
        }
        .await;
        finish(transaction, result).await
    }

    /// Appends one or more currently visible `CatalogItems` to an owned Playlist.
    ///
    /// Every item is locked and validated before insertion, so a rejected batch
    /// has no partial entries.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] for an inaccessible Playlist,
    /// an invisible item, or a database failure.
    pub async fn append_items(
        &self,
        owner: UserId,
        playlist_id: Uuid,
        item_ids: &[CatalogItemId],
    ) -> Result<(), MediaCollectionRepositoryError> {
        if item_ids.is_empty() {
            return Ok(());
        }
        let transaction = self.database.begin().await?;
        let result = async {
            ensure_owned_playlist(&transaction, owner, playlist_id).await?;
            for &item_id in item_ids {
                if !lock_catalog_item_visibility(&transaction, item_id).await? {
                    return Err(MediaCollectionRepositoryError::ItemUnavailable(item_id));
                }
            }
            let position = next_position(&transaction, playlist_id).await?;
            let now = Utc::now();
            for (offset, &item_id) in item_ids.iter().enumerate() {
                let offset = i64::try_from(offset)
                    .map_err(|_| MediaCollectionRepositoryError::InvalidPosition)?;
                insert_entry(&transaction, playlist_id, item_id, position + offset, now).await?;
            }
            touch_collection(&transaction, playlist_id, now).await
        }
        .await;
        finish(transaction, result).await
    }

    /// Appends currently visible `CatalogItems` to a shared Collection.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] for an unknown Collection, an
    /// invisible item, or a database failure.
    pub async fn append_shared_items(
        &self,
        collection_id: Uuid,
        item_ids: &[CatalogItemId],
    ) -> Result<(), MediaCollectionRepositoryError> {
        if item_ids.is_empty() {
            return Ok(());
        }
        let transaction = self.database.begin().await?;
        let result = async {
            ensure_shared_collection(&transaction, collection_id).await?;
            for &item_id in item_ids {
                if !lock_catalog_item_visibility(&transaction, item_id).await? {
                    return Err(MediaCollectionRepositoryError::ItemUnavailable(item_id));
                }
            }
            let position = next_position(&transaction, collection_id).await?;
            let now = Utc::now();
            for (offset, &item_id) in item_ids.iter().enumerate() {
                let offset = i64::try_from(offset)
                    .map_err(|_| MediaCollectionRepositoryError::InvalidPosition)?;
                insert_entry(&transaction, collection_id, item_id, position + offset, now).await?;
            }
            touch_collection(&transaction, collection_id, now).await
        }
        .await;
        finish(transaction, result).await
    }

    /// Removes one Playlist entry by its stable entry ID.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] for an inaccessible Playlist,
    /// an unknown entry, or a database failure.
    pub async fn delete_item(
        &self,
        owner: UserId,
        playlist_id: Uuid,
        entry_id: Uuid,
    ) -> Result<(), MediaCollectionRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = async {
            ensure_owned_playlist(&transaction, owner, playlist_id).await?;
            let query = Query::delete()
                .from_table(Alias::new("media_collection_entries"))
                .and_where(Expr::col(Alias::new("id")).eq(entry_id))
                .and_where(Expr::col(Alias::new("media_collection_id")).eq(playlist_id))
                .to_owned();
            if transaction
                .execute(transaction.get_database_backend().build(&query))
                .await?
                .rows_affected()
                != 1
            {
                return Err(MediaCollectionRepositoryError::EntryNotFound);
            }
            compact_positions(&transaction, playlist_id).await?;
            touch_collection(&transaction, playlist_id, Utc::now()).await
        }
        .await;
        finish(transaction, result).await
    }

    /// Moves a Playlist entry to a zero-based position using its stable entry ID.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] for an inaccessible Playlist,
    /// an unknown entry, an invalid position, or a database failure.
    pub async fn move_item(
        &self,
        owner: UserId,
        playlist_id: Uuid,
        entry_id: Uuid,
        new_index: u64,
    ) -> Result<(), MediaCollectionRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = async {
            ensure_owned_playlist(&transaction, owner, playlist_id).await?;
            let mut entries = entry_ids(&transaction, playlist_id).await?;
            let old_index = entries
                .iter()
                .position(|id| *id == entry_id)
                .ok_or(MediaCollectionRepositoryError::EntryNotFound)?;
            let new_index = usize::try_from(new_index)
                .map_err(|_| MediaCollectionRepositoryError::InvalidPosition)?;
            if new_index >= entries.len() {
                return Err(MediaCollectionRepositoryError::InvalidPosition);
            }
            let entry_id = entries.remove(old_index);
            entries.insert(new_index, entry_id);
            write_positions(&transaction, playlist_id, &entries).await?;
            touch_collection(&transaction, playlist_id, Utc::now()).await
        }
        .await;
        finish(transaction, result).await
    }

    /// Lists visible Playlist entries in their persisted order.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] when the Playlist is not owned
    /// by the caller, when stored data is malformed, or when SQL fails.
    pub async fn items(
        &self,
        owner: UserId,
        playlist_id: Uuid,
    ) -> Result<Vec<MediaCollectionEntry>, MediaCollectionRepositoryError> {
        ensure_owned_playlist(self.database, owner, playlist_id).await?;
        visible_entries(self.database, playlist_id).await
    }

    /// Lists visible shared Collection entries in their persisted order.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionRepositoryError`] when the Collection does not
    /// exist, stored data is malformed, or SQL fails.
    pub async fn shared_items(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<MediaCollectionEntry>, MediaCollectionRepositoryError> {
        ensure_shared_collection(self.database, collection_id).await?;
        visible_entries(self.database, collection_id).await
    }
}

async fn create_collection(
    connection: &impl ConnectionTrait,
    kind: MediaCollectionKind,
    owner: Option<UserId>,
    name: &str,
) -> Result<MediaCollectionRecord, MediaCollectionRepositoryError> {
    let name = validate_name(name)?;
    let now = Utc::now();
    let record = MediaCollectionRecord {
        id: Uuid::new_v4(),
        kind,
        owner_user_id: owner,
        name,
        created_at: now,
        updated_at: now,
    };
    let query = Query::insert()
        .into_table(Alias::new("media_collections"))
        .columns([
            Alias::new("id"),
            Alias::new("kind"),
            Alias::new("owner_user_id"),
            Alias::new("name"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            record.id.into(),
            kind.as_database_value().into(),
            owner.map(UserId::as_uuid).into(),
            record.name.clone().into(),
            now.into(),
            now.into(),
        ])
        .to_owned();
    connection
        .execute(connection.get_database_backend().build(&query))
        .await?;
    Ok(record)
}

async fn read_collections(
    connection: &impl ConnectionTrait,
    kind: MediaCollectionKind,
    owner: Option<UserId>,
) -> Result<Vec<MediaCollectionRecord>, MediaCollectionRepositoryError> {
    let mut query = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("kind"),
            Alias::new("owner_user_id"),
            Alias::new("name"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .from(Alias::new("media_collections"))
        .and_where(Expr::col(Alias::new("kind")).eq(kind.as_database_value()))
        .to_owned();
    match owner {
        Some(owner) => {
            query.and_where(Expr::col(Alias::new("owner_user_id")).eq(owner.as_uuid()));
        }
        None => {
            query.and_where(Expr::col(Alias::new("owner_user_id")).is_null());
        }
    }
    query
        .order_by(Alias::new("name"), Order::Asc)
        .order_by(Alias::new("id"), Order::Asc);
    let backend = connection.get_database_backend();
    connection
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(collection_from_row)
        .collect()
}

async fn rename_owned_playlist(
    database: &DatabaseConnection,
    owner: UserId,
    playlist_id: Uuid,
    name: &str,
) -> Result<MediaCollectionRecord, MediaCollectionRepositoryError> {
    let name = validate_name(name)?;
    let transaction = database.begin().await?;
    let result = async {
        ensure_owned_playlist(&transaction, owner, playlist_id).await?;
        rename_collection(&transaction, playlist_id, &name).await?;
        read_collection(&transaction, playlist_id).await
    }
    .await;
    finish(transaction, result).await
}

async fn rename_shared_collection(
    database: &DatabaseConnection,
    collection_id: Uuid,
    name: &str,
) -> Result<MediaCollectionRecord, MediaCollectionRepositoryError> {
    let name = validate_name(name)?;
    let transaction = database.begin().await?;
    let result = async {
        ensure_shared_collection(&transaction, collection_id).await?;
        rename_collection(&transaction, collection_id, &name).await?;
        read_collection(&transaction, collection_id).await
    }
    .await;
    finish(transaction, result).await
}

async fn rename_collection(
    transaction: &DatabaseTransaction,
    collection_id: Uuid,
    name: &str,
) -> Result<(), MediaCollectionRepositoryError> {
    let query = Query::update()
        .table(Alias::new("media_collections"))
        .value(Alias::new("name"), name)
        .value(Alias::new("updated_at"), Utc::now())
        .and_where(Expr::col(Alias::new("id")).eq(collection_id))
        .to_owned();
    if transaction
        .execute(transaction.get_database_backend().build(&query))
        .await?
        .rows_affected()
        != 1
    {
        return Err(MediaCollectionRepositoryError::NotFound);
    }
    Ok(())
}

async fn delete_collection(
    transaction: &DatabaseTransaction,
    collection_id: Uuid,
) -> Result<(), MediaCollectionRepositoryError> {
    let query = Query::delete()
        .from_table(Alias::new("media_collections"))
        .and_where(Expr::col(Alias::new("id")).eq(collection_id))
        .to_owned();
    if transaction
        .execute(transaction.get_database_backend().build(&query))
        .await?
        .rows_affected()
        != 1
    {
        return Err(MediaCollectionRepositoryError::NotFound);
    }
    Ok(())
}

async fn read_collection(
    connection: &impl ConnectionTrait,
    collection_id: Uuid,
) -> Result<MediaCollectionRecord, MediaCollectionRepositoryError> {
    let query = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("kind"),
            Alias::new("owner_user_id"),
            Alias::new("name"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .from(Alias::new("media_collections"))
        .and_where(Expr::col(Alias::new("id")).eq(collection_id))
        .limit(1)
        .to_owned();
    let row = connection
        .query_one(connection.get_database_backend().build(&query))
        .await?
        .ok_or(MediaCollectionRepositoryError::NotFound)?;
    collection_from_row(&row)
}

fn collection_from_row(
    row: &QueryResult,
) -> Result<MediaCollectionRecord, MediaCollectionRepositoryError> {
    Ok(MediaCollectionRecord {
        id: row.try_get("", "id")?,
        kind: MediaCollectionKind::from_database_value(&row.try_get::<String>("", "kind")?)?,
        owner_user_id: row
            .try_get::<Option<Uuid>>("", "owner_user_id")?
            .map(UserId::from_uuid),
        name: row.try_get("", "name")?,
        created_at: row.try_get("", "created_at")?,
        updated_at: row.try_get("", "updated_at")?,
    })
}

async fn ensure_owned_playlist(
    connection: &impl ConnectionTrait,
    owner: UserId,
    playlist_id: Uuid,
) -> Result<(), MediaCollectionRepositoryError> {
    let query = Query::select()
        .columns([Alias::new("kind"), Alias::new("owner_user_id")])
        .from(Alias::new("media_collections"))
        .and_where(Expr::col(Alias::new("id")).eq(playlist_id))
        .limit(1)
        .to_owned();
    let row = connection
        .query_one(connection.get_database_backend().build(&query))
        .await?
        .ok_or(MediaCollectionRepositoryError::NotFound)?;
    let kind: String = row.try_get("", "kind")?;
    let record_owner: Option<Uuid> = row.try_get("", "owner_user_id")?;
    if MediaCollectionKind::from_database_value(&kind)? != MediaCollectionKind::Playlist
        || record_owner != Some(owner.as_uuid())
    {
        return Err(MediaCollectionRepositoryError::Forbidden);
    }
    Ok(())
}

async fn ensure_shared_collection(
    connection: &impl ConnectionTrait,
    collection_id: Uuid,
) -> Result<(), MediaCollectionRepositoryError> {
    let query = Query::select()
        .columns([Alias::new("kind"), Alias::new("owner_user_id")])
        .from(Alias::new("media_collections"))
        .and_where(Expr::col(Alias::new("id")).eq(collection_id))
        .limit(1)
        .to_owned();
    let row = connection
        .query_one(connection.get_database_backend().build(&query))
        .await?
        .ok_or(MediaCollectionRepositoryError::NotFound)?;
    let kind: String = row.try_get("", "kind")?;
    let owner: Option<Uuid> = row.try_get("", "owner_user_id")?;
    if MediaCollectionKind::from_database_value(&kind)? != MediaCollectionKind::Collection
        || owner.is_some()
    {
        return Err(MediaCollectionRepositoryError::NotFound);
    }
    Ok(())
}

async fn visible_entries(
    connection: &impl ConnectionTrait,
    collection_id: Uuid,
) -> Result<Vec<MediaCollectionEntry>, MediaCollectionRepositoryError> {
    let entries = read_entries(connection, collection_id).await?;
    let mut visible = Vec::with_capacity(entries.len());
    for entry in entries {
        if crate::catalog_query::catalog_item_is_visible(connection, entry.item.id()).await? {
            visible.push(entry);
        }
    }
    Ok(visible)
}

async fn next_position(
    transaction: &DatabaseTransaction,
    playlist_id: Uuid,
) -> Result<i64, MediaCollectionRepositoryError> {
    let query = Query::select()
        .expr_as(
            Expr::col(Alias::new("position")).max(),
            Alias::new("max_position"),
        )
        .from(Alias::new("media_collection_entries"))
        .and_where(Expr::col(Alias::new("media_collection_id")).eq(playlist_id))
        .to_owned();
    let row = transaction
        .query_one(transaction.get_database_backend().build(&query))
        .await?
        .ok_or(MediaCollectionRepositoryError::MissingAggregate)?;
    let previous: Option<i64> = row.try_get("", "max_position")?;
    previous.map_or(Ok(0), |position| {
        position
            .checked_add(1)
            .ok_or(MediaCollectionRepositoryError::InvalidPosition)
    })
}

async fn compact_positions(
    transaction: &DatabaseTransaction,
    playlist_id: Uuid,
) -> Result<(), MediaCollectionRepositoryError> {
    let entries = entry_ids(transaction, playlist_id).await?;
    write_positions(transaction, playlist_id, &entries).await
}

async fn entry_ids(
    connection: &impl ConnectionTrait,
    playlist_id: Uuid,
) -> Result<Vec<Uuid>, MediaCollectionRepositoryError> {
    let query = Query::select()
        .column(Alias::new("id"))
        .from(Alias::new("media_collection_entries"))
        .and_where(Expr::col(Alias::new("media_collection_id")).eq(playlist_id))
        .order_by(Alias::new("position"), Order::Asc)
        .order_by(Alias::new("id"), Order::Asc)
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| row.try_get("", "id").map_err(Into::into))
        .collect()
}

async fn write_positions(
    transaction: &DatabaseTransaction,
    playlist_id: Uuid,
    entry_ids: &[Uuid],
) -> Result<(), MediaCollectionRepositoryError> {
    let temporary_offset = i64::try_from(entry_ids.len())
        .map_err(|_| MediaCollectionRepositoryError::InvalidPosition)?;
    for (index, entry_id) in entry_ids.iter().enumerate() {
        let index =
            i64::try_from(index).map_err(|_| MediaCollectionRepositoryError::InvalidPosition)?;
        let query = Query::update()
            .table(Alias::new("media_collection_entries"))
            .value(Alias::new("position"), temporary_offset + index)
            .and_where(Expr::col(Alias::new("id")).eq(*entry_id))
            .and_where(Expr::col(Alias::new("media_collection_id")).eq(playlist_id))
            .to_owned();
        transaction
            .execute(transaction.get_database_backend().build(&query))
            .await?;
    }
    for (index, entry_id) in entry_ids.iter().enumerate() {
        let index =
            i64::try_from(index).map_err(|_| MediaCollectionRepositoryError::InvalidPosition)?;
        let query = Query::update()
            .table(Alias::new("media_collection_entries"))
            .value(Alias::new("position"), index)
            .and_where(Expr::col(Alias::new("id")).eq(*entry_id))
            .and_where(Expr::col(Alias::new("media_collection_id")).eq(playlist_id))
            .to_owned();
        transaction
            .execute(transaction.get_database_backend().build(&query))
            .await?;
    }
    Ok(())
}

async fn insert_entry(
    transaction: &DatabaseTransaction,
    playlist_id: Uuid,
    item_id: CatalogItemId,
    position: i64,
    now: DateTime<Utc>,
) -> Result<(), MediaCollectionRepositoryError> {
    let query = Query::insert()
        .into_table(Alias::new("media_collection_entries"))
        .columns([
            Alias::new("id"),
            Alias::new("media_collection_id"),
            Alias::new("catalog_item_id"),
            Alias::new("position"),
            Alias::new("created_at"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            playlist_id.into(),
            item_id.as_uuid().into(),
            position.into(),
            now.into(),
        ])
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&query))
        .await?;
    Ok(())
}

async fn touch_collection(
    transaction: &DatabaseTransaction,
    playlist_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), MediaCollectionRepositoryError> {
    let query = Query::update()
        .table(Alias::new("media_collections"))
        .value(Alias::new("updated_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(playlist_id))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&query))
        .await?;
    Ok(())
}

async fn read_entries(
    connection: &impl ConnectionTrait,
    playlist_id: Uuid,
) -> Result<Vec<MediaCollectionEntry>, MediaCollectionRepositoryError> {
    let entry = Alias::new("collection_entry");
    let item = Alias::new("collection_item");
    let query = Query::select()
        .expr_as(
            Expr::col((entry.clone(), Alias::new("id"))),
            Alias::new("entry_id"),
        )
        .expr_as(
            Expr::col((entry.clone(), Alias::new("position"))),
            Alias::new("position"),
        )
        .expr_as(
            Expr::col((item.clone(), Alias::new("id"))),
            Alias::new("item_id"),
        )
        .expr_as(
            Expr::col((item.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .expr_as(
            Expr::col((item.clone(), Alias::new("item_type"))),
            Alias::new("item_type"),
        )
        .from_as(Alias::new("media_collection_entries"), entry.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id")))
                .equals((entry.clone(), Alias::new("catalog_item_id"))),
        )
        .and_where(Expr::col((entry.clone(), Alias::new("media_collection_id"))).eq(playlist_id))
        .order_by((entry.clone(), Alias::new("position")), Order::Asc)
        .order_by((entry, Alias::new("id")), Order::Asc)
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(entry_from_row)
        .collect()
}

fn entry_from_row(
    row: &QueryResult,
) -> Result<MediaCollectionEntry, MediaCollectionRepositoryError> {
    Ok(MediaCollectionEntry {
        id: row.try_get("", "entry_id")?,
        position: row.try_get("", "position")?,
        item: MediaCollectionCatalogItem {
            id: CatalogItemId::from_uuid(row.try_get("", "item_id")?),
            name: row.try_get("", "name")?,
            item_type: row.try_get("", "item_type")?,
        },
    })
}

fn validate_name(name: &str) -> Result<String, MediaCollectionRepositoryError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 256 || name.chars().any(char::is_control) {
        return Err(MediaCollectionRepositoryError::InvalidName);
    }
    Ok(name.to_owned())
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, MediaCollectionRepositoryError>,
) -> Result<T, MediaCollectionRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(MediaCollectionRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

#[derive(Debug, Error)]
pub enum MediaCollectionRepositoryError {
    #[error("collection name is invalid")]
    InvalidName,
    #[error("collection item is unavailable: {0}")]
    ItemUnavailable(CatalogItemId),
    #[error("collection was not found")]
    NotFound,
    #[error("collection entry was not found")]
    EntryNotFound,
    #[error("collection access is not permitted")]
    Forbidden,
    #[error("collection entry position is invalid")]
    InvalidPosition,
    #[error("collection aggregate row is missing")]
    MissingAggregate,
    #[error("collection kind in storage is invalid")]
    InvalidStoredKind,
    #[error("collection database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("collection rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}
