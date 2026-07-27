use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::{CatalogItemId, UserId};
use tjxy_db::{
    MediaCollectionEntry, MediaCollectionRecord, MediaCollectionRepository,
    MediaCollectionRepositoryError,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct MediaCollectionService {
    database: DatabaseConnection,
}

impl MediaCollectionService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Creates a Playlist owned by the requesting user.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError`] when collection persistence fails.
    pub async fn create_playlist(
        &self,
        owner: UserId,
        name: &str,
    ) -> Result<MediaCollectionRecord, MediaCollectionServiceError> {
        MediaCollectionRepository::new(&self.database)
            .create_playlist(owner, name)
            .await
            .map_err(Into::into)
    }

    /// Lists Playlists owned by the requesting user.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError`] when collection persistence fails.
    pub async fn playlists(
        &self,
        owner: UserId,
    ) -> Result<Vec<MediaCollectionRecord>, MediaCollectionServiceError> {
        MediaCollectionRepository::new(&self.database)
            .playlists(owner)
            .await
            .map_err(Into::into)
    }

    /// Renames one Playlist owned by the requesting user.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError`] for an inaccessible Playlist or invalid name.
    pub async fn rename_playlist(
        &self,
        owner: UserId,
        playlist_id: Uuid,
        name: &str,
    ) -> Result<MediaCollectionRecord, MediaCollectionServiceError> {
        MediaCollectionRepository::new(&self.database)
            .rename_playlist(owner, playlist_id, name)
            .await
            .map_err(Into::into)
    }

    /// Deletes one Playlist owned by the requesting user.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError`] for an inaccessible Playlist or SQL failure.
    pub async fn delete_playlist(
        &self,
        owner: UserId,
        playlist_id: Uuid,
    ) -> Result<(), MediaCollectionServiceError> {
        MediaCollectionRepository::new(&self.database)
            .delete_playlist(owner, playlist_id)
            .await
            .map_err(Into::into)
    }

    /// Adds `CatalogItems` to a Playlist owned by the requesting user.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError`] for an inaccessible Playlist or
    /// unavailable `CatalogItem`.
    pub async fn append_playlist_items(
        &self,
        owner: UserId,
        playlist_id: Uuid,
        item_ids: &[CatalogItemId],
    ) -> Result<(), MediaCollectionServiceError> {
        MediaCollectionRepository::new(&self.database)
            .append_items(owner, playlist_id, item_ids)
            .await
            .map_err(Into::into)
    }

    /// Reads visible entries from an owned Playlist.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError`] for an inaccessible Playlist or SQL failure.
    pub async fn playlist_items(
        &self,
        owner: UserId,
        playlist_id: Uuid,
    ) -> Result<Vec<MediaCollectionEntry>, MediaCollectionServiceError> {
        MediaCollectionRepository::new(&self.database)
            .items(owner, playlist_id)
            .await
            .map_err(Into::into)
    }

    /// Removes one Playlist entry owned by the requesting user.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError`] for an inaccessible Playlist or unknown entry.
    pub async fn delete_playlist_item(
        &self,
        owner: UserId,
        playlist_id: Uuid,
        entry_id: Uuid,
    ) -> Result<(), MediaCollectionServiceError> {
        MediaCollectionRepository::new(&self.database)
            .delete_item(owner, playlist_id, entry_id)
            .await
            .map_err(Into::into)
    }

    /// Moves one Playlist entry owned by the requesting user.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError`] for an inaccessible Playlist,
    /// unknown entry, or invalid position.
    pub async fn move_playlist_item(
        &self,
        owner: UserId,
        playlist_id: Uuid,
        entry_id: Uuid,
        new_index: u64,
    ) -> Result<(), MediaCollectionServiceError> {
        MediaCollectionRepository::new(&self.database)
            .move_item(owner, playlist_id, entry_id, new_index)
            .await
            .map_err(Into::into)
    }

    /// Creates a shared Collection when the caller is an administrator.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError::AdministratorRequired`] for a
    /// non-administrator caller or propagates persistence failures.
    pub async fn create_shared_collection(
        &self,
        is_administrator: bool,
        name: &str,
    ) -> Result<MediaCollectionRecord, MediaCollectionServiceError> {
        if !is_administrator {
            return Err(MediaCollectionServiceError::AdministratorRequired);
        }
        MediaCollectionRepository::new(&self.database)
            .create_shared_collection(name)
            .await
            .map_err(Into::into)
    }

    /// Lists shared Collections visible to every authenticated caller.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError`] when collection persistence fails.
    pub async fn shared_collections(
        &self,
    ) -> Result<Vec<MediaCollectionRecord>, MediaCollectionServiceError> {
        MediaCollectionRepository::new(&self.database)
            .shared_collections()
            .await
            .map_err(Into::into)
    }

    /// Renames a shared Collection when the caller is an administrator.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError::AdministratorRequired`] for a
    /// non-administrator caller or propagates persistence failures.
    pub async fn rename_shared_collection(
        &self,
        is_administrator: bool,
        collection_id: Uuid,
        name: &str,
    ) -> Result<MediaCollectionRecord, MediaCollectionServiceError> {
        if !is_administrator {
            return Err(MediaCollectionServiceError::AdministratorRequired);
        }
        MediaCollectionRepository::new(&self.database)
            .rename_shared_collection(collection_id, name)
            .await
            .map_err(Into::into)
    }

    /// Deletes a shared Collection when the caller is an administrator.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError::AdministratorRequired`] for a
    /// non-administrator caller or propagates persistence failures.
    pub async fn delete_shared_collection(
        &self,
        is_administrator: bool,
        collection_id: Uuid,
    ) -> Result<(), MediaCollectionServiceError> {
        if !is_administrator {
            return Err(MediaCollectionServiceError::AdministratorRequired);
        }
        MediaCollectionRepository::new(&self.database)
            .delete_shared_collection(collection_id)
            .await
            .map_err(Into::into)
    }

    /// Adds `CatalogItems` to a shared Collection when the caller is an administrator.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError::AdministratorRequired`] for a
    /// non-administrator caller or propagates persistence failures.
    pub async fn append_shared_items(
        &self,
        is_administrator: bool,
        collection_id: Uuid,
        item_ids: &[CatalogItemId],
    ) -> Result<(), MediaCollectionServiceError> {
        if !is_administrator {
            return Err(MediaCollectionServiceError::AdministratorRequired);
        }
        MediaCollectionRepository::new(&self.database)
            .append_shared_items(collection_id, item_ids)
            .await
            .map_err(Into::into)
    }

    /// Reads visible entries from a shared Collection.
    ///
    /// # Errors
    ///
    /// Returns [`MediaCollectionServiceError`] for an unknown Collection or SQL failure.
    pub async fn shared_items(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<MediaCollectionEntry>, MediaCollectionServiceError> {
        MediaCollectionRepository::new(&self.database)
            .shared_items(collection_id)
            .await
            .map_err(Into::into)
    }
}

#[derive(Debug, Error)]
pub enum MediaCollectionServiceError {
    #[error("administrator permission is required")]
    AdministratorRequired,
    #[error("media collection persistence failed: {0}")]
    Repository(#[from] MediaCollectionRepositoryError),
}
