use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::UserId;
use tjxy_db::{
    BrowseParent, CatalogPage, CatalogPageRequest, CatalogQueryError, CatalogQueryRepository,
    LibraryViewRecord,
};
use uuid::Uuid;

/// Authenticated read boundary for the published catalog.
#[derive(Clone)]
pub struct CatalogQueryService {
    database: DatabaseConnection,
}

impl CatalogQueryService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Returns enabled library views visible under the current v1 policy.
    ///
    /// The v1 schema has no per-user library grants, so every authenticated,
    /// enabled user sees every enabled library. A supplied Jellyfin `UserId`
    /// remains an assertion about the principal, never an authority source.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for a mismatched user or
    /// propagates a catalog query failure.
    pub async fn user_views(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
    ) -> Result<Vec<LibraryViewRecord>, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        CatalogQueryRepository::new(&self.database)
            .user_views()
            .await
            .map_err(Into::into)
    }

    /// Returns a bounded, membership-filtered catalog page for the principal.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for a mismatched user or
    /// propagates a catalog query failure.
    pub async fn items(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        parent: BrowseParent,
        page: CatalogPageRequest,
    ) -> Result<CatalogPage, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        CatalogQueryRepository::new(&self.database)
            .items(principal, parent, page)
            .await
            .map_err(Into::into)
    }

    /// Resolves a wire-level parent UUID and returns its catalog page.
    ///
    /// `None` deliberately combines unknown and inaccessible parents so callers
    /// cannot use this boundary to enumerate disabled catalog data.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogServiceError::ForbiddenUser`] for a mismatched user or
    /// propagates a catalog query failure.
    pub async fn items_by_parent_id(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        parent_id: Uuid,
        page: CatalogPageRequest,
    ) -> Result<Option<CatalogPage>, CatalogServiceError> {
        authorize_user(principal, requested_user)?;
        let repository = CatalogQueryRepository::new(&self.database);
        let Some(parent) = repository.resolve_parent(parent_id).await? else {
            return Ok(None);
        };
        repository
            .items(principal, parent, page)
            .await
            .map(Some)
            .map_err(Into::into)
    }
}

#[derive(Debug, Error)]
pub enum CatalogServiceError {
    #[error("requested user does not match the authenticated principal")]
    ForbiddenUser,
    #[error("catalog query failed: {0}")]
    Query(#[from] CatalogQueryError),
}

fn authorize_user(
    principal: UserId,
    requested_user: Option<UserId>,
) -> Result<(), CatalogServiceError> {
    if requested_user.is_some_and(|requested| requested != principal) {
        return Err(CatalogServiceError::ForbiddenUser);
    }
    Ok(())
}
