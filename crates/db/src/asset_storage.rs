use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetStorageRoot {
    id: Uuid,
    canonical_path: String,
    state: String,
}

impl AssetStorageRoot {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub fn canonical_path(&self) -> &str {
        &self.canonical_path
    }
    #[must_use]
    pub fn state(&self) -> &str {
        &self.state
    }
}

pub struct AssetStorageRepository<'a> {
    database: &'a DatabaseConnection,
}

impl<'a> AssetStorageRepository<'a> {
    #[must_use]
    pub const fn new(database: &'a DatabaseConnection) -> Self {
        Self { database }
    }

    pub async fn activate(
        &self,
        canonical_path: &str,
    ) -> Result<AssetStorageRoot, AssetStorageError> {
        let transaction = self.database.begin().await?;
        let backend = transaction.get_database_backend();
        let demote = Query::update()
            .table(Alias::new("asset_storage_roots"))
            .value(Alias::new("state"), "History")
            .value(Alias::new("updated_at"), Utc::now())
            .and_where(Expr::col(Alias::new("state")).eq("Current"))
            .and_where(Expr::col(Alias::new("canonical_path")).ne(canonical_path))
            .to_owned();
        transaction.execute(backend.build(&demote)).await?;
        let id = Uuid::new_v4();
        let insert = Query::insert()
            .into_table(Alias::new("asset_storage_roots"))
            .columns([
                Alias::new("id"),
                Alias::new("canonical_path"),
                Alias::new("state"),
                Alias::new("revision"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                id.into(),
                canonical_path.into(),
                "Current".into(),
                1_i64.into(),
                Utc::now().into(),
                Utc::now().into(),
            ])
            .on_conflict(
                OnConflict::column(Alias::new("canonical_path"))
                    .update_columns([Alias::new("state"), Alias::new("updated_at")])
                    .to_owned(),
            )
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
        let select = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("canonical_path"),
                Alias::new("state"),
            ])
            .from(Alias::new("asset_storage_roots"))
            .and_where(Expr::col(Alias::new("canonical_path")).eq(canonical_path))
            .limit(1)
            .to_owned();
        let row = transaction
            .query_one(backend.build(&select))
            .await?
            .ok_or(AssetStorageError::MissingRoot)?;
        let root_id: Uuid = row.try_get("", "id")?;
        let backfill = Query::update()
            .table(Alias::new("asset_blobs"))
            .value(Alias::new("storage_root_id"), root_id)
            .and_where(Expr::col(Alias::new("storage_root_id")).is_null())
            .to_owned();
        transaction.execute(backend.build(&backfill)).await?;
        transaction.commit().await?;
        Ok(AssetStorageRoot {
            id: root_id,
            canonical_path: row.try_get("", "canonical_path")?,
            state: row.try_get("", "state")?,
        })
    }

    pub async fn register_history(
        &self,
        canonical_path: &str,
    ) -> Result<AssetStorageRoot, AssetStorageError> {
        let backend = self.database.get_database_backend();
        let insert = Query::insert()
            .into_table(Alias::new("asset_storage_roots"))
            .columns([
                Alias::new("id"),
                Alias::new("canonical_path"),
                Alias::new("state"),
                Alias::new("revision"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                canonical_path.into(),
                "History".into(),
                1_i64.into(),
                Utc::now().into(),
                Utc::now().into(),
            ])
            .on_conflict(
                OnConflict::column(Alias::new("canonical_path"))
                    .do_nothing()
                    .to_owned(),
            )
            .to_owned();
        self.database.execute(backend.build(&insert)).await?;
        let select = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("canonical_path"),
                Alias::new("state"),
            ])
            .from(Alias::new("asset_storage_roots"))
            .and_where(Expr::col(Alias::new("canonical_path")).eq(canonical_path))
            .limit(1)
            .to_owned();
        let row = self
            .database
            .query_one(backend.build(&select))
            .await?
            .ok_or(AssetStorageError::MissingRoot)?;
        Ok(AssetStorageRoot {
            id: row.try_get("", "id")?,
            canonical_path: row.try_get("", "canonical_path")?,
            state: row.try_get("", "state")?,
        })
    }

    pub async fn roots(&self) -> Result<Vec<AssetStorageRoot>, AssetStorageError> {
        let backend = self.database.get_database_backend();
        let query = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("canonical_path"),
                Alias::new("state"),
            ])
            .from(Alias::new("asset_storage_roots"))
            .order_by(Alias::new("created_at"), sea_orm::sea_query::Order::Asc)
            .to_owned();
        let rows = self.database.query_all(backend.build(&query)).await?;
        rows.into_iter()
            .map(|row| {
                Ok(AssetStorageRoot {
                    id: row.try_get("", "id")?,
                    canonical_path: row.try_get("", "canonical_path")?,
                    state: row.try_get("", "state")?,
                })
            })
            .collect()
    }

    pub async fn set_pending(&self, canonical_path: &str) -> Result<(), AssetStorageError> {
        let transaction = self.database.begin().await?;
        let backend = transaction.get_database_backend();
        let delete = Query::delete()
            .from_table(Alias::new("asset_storage_roots"))
            .and_where(Expr::col(Alias::new("state")).eq("Pending"))
            .to_owned();
        transaction.execute(backend.build(&delete)).await?;
        let current = Query::select()
            .expr(Expr::val(1))
            .from(Alias::new("asset_storage_roots"))
            .and_where(Expr::col(Alias::new("state")).eq("Current"))
            .and_where(Expr::col(Alias::new("canonical_path")).eq(canonical_path))
            .limit(1)
            .to_owned();
        if transaction
            .query_one(backend.build(&current))
            .await?
            .is_some()
        {
            transaction.commit().await?;
            return Ok(());
        }
        let insert = Query::insert()
            .into_table(Alias::new("asset_storage_roots"))
            .columns([
                Alias::new("id"),
                Alias::new("canonical_path"),
                Alias::new("state"),
                Alias::new("revision"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                canonical_path.into(),
                "Pending".into(),
                1_i64.into(),
                Utc::now().into(),
                Utc::now().into(),
            ])
            .on_conflict(
                OnConflict::column(Alias::new("canonical_path"))
                    .update_columns([Alias::new("state"), Alias::new("updated_at")])
                    .to_owned(),
            )
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
        transaction.commit().await?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum AssetStorageError {
    #[error("asset storage root is missing")]
    MissingRoot,
    #[error("asset storage database operation failed: {0}")]
    Database(#[from] DbErr),
}
