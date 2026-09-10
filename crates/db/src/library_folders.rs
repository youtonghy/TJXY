use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr,
    sea_query::{Alias, Expr, JoinType, Order, Query},
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct LibraryFolder {
    pub id: Uuid,
    pub path: Option<String>,
    pub name: String,
    pub provider: String,
    pub root_object_id: Option<Uuid>,
}

#[derive(Clone, Debug)]
pub struct LibraryFolderEntry {
    pub id: Uuid,
    pub name: String,
    pub is_directory: bool,
    pub size: Option<i64>,
    pub modified_at: Option<DateTime<Utc>>,
}

pub struct LibraryFolderRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> LibraryFolderRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Reads administrator folder labels and physical paths for a library.
    /// # Errors
    /// Returns a database error if the read model is unavailable.
    pub async fn folders(&self, library_id: Uuid) -> Result<Vec<LibraryFolder>, DbErr> {
        let mapping = Alias::new("mapping");
        let root = Alias::new("root");
        let account = Alias::new("account");
        let config = Alias::new("config");
        let relation = Alias::new("relation");
        let object = Alias::new("object");
        let query = Query::select()
            .column((root.clone(), Alias::new("id")))
            .column((account.clone(), Alias::new("provider")))
            .column((config.clone(), Alias::new("root_path")))
            .column((object.clone(), Alias::new("name")))
            .expr_as(
                Expr::col((object.clone(), Alias::new("id"))),
                Alias::new("object_id"),
            )
            .from_as(Alias::new("library_storage_roots"), mapping.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_roots"),
                root.clone(),
                Expr::col((root.clone(), Alias::new("id")))
                    .equals((mapping.clone(), Alias::new("storage_root_id"))),
            )
            .join_as(
                JoinType::LeftJoin,
                Alias::new("storage_accounts"),
                account.clone(),
                Expr::col((account.clone(), Alias::new("id")))
                    .equals((root.clone(), Alias::new("storage_account_id"))),
            )
            .join_as(
                JoinType::LeftJoin,
                Alias::new("filesystem_storage_configs"),
                config.clone(),
                Expr::col((config, Alias::new("storage_account_id")))
                    .equals((account, Alias::new("id"))),
            )
            .join_as(
                JoinType::LeftJoin,
                Alias::new("storage_root_objects"),
                relation.clone(),
                Expr::col((relation.clone(), Alias::new("storage_root_id")))
                    .equals((root.clone(), Alias::new("id")))
                    .and(
                        Expr::col((relation.clone(), Alias::new("parent_storage_object_id")))
                            .is_null(),
                    ),
            )
            .join_as(
                JoinType::LeftJoin,
                Alias::new("storage_objects"),
                object.clone(),
                Expr::col((object, Alias::new("id")))
                    .equals((relation, Alias::new("storage_object_id"))),
            )
            .and_where(Expr::col((mapping, Alias::new("library_id"))).eq(library_id))
            .order_by((root, Alias::new("id")), Order::Asc)
            .to_owned();
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .into_iter()
            .map(|row| {
                Ok(LibraryFolder {
                    id: row.try_get("", "id")?,
                    path: row.try_get("", "root_path")?,
                    name: row
                        .try_get::<Option<String>>("", "name")?
                        .unwrap_or_else(|| "Folder".to_owned()),
                    provider: row
                        .try_get::<Option<String>>("", "provider")?
                        .unwrap_or_else(|| "unknown".to_owned()),
                    root_object_id: row.try_get("", "object_id")?,
                })
            })
            .collect()
    }

    /// Reads one bounded level of the synchronized remote folder inventory.
    /// # Errors
    /// Returns a database error if the inventory cannot be read.
    pub async fn children(
        &self,
        root_id: Uuid,
        parent_id: Uuid,
    ) -> Result<Vec<LibraryFolderEntry>, DbErr> {
        let root = Alias::new("root");
        let object = Alias::new("object");
        let query = Query::select()
            .columns(
                ["id", "name", "object_type", "size", "remote_modified_at"]
                    .map(|column| (object.clone(), Alias::new(column))),
            )
            .from_as(Alias::new("storage_root_objects"), root.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_objects"),
                object.clone(),
                Expr::col((object.clone(), Alias::new("id")))
                    .equals((root.clone(), Alias::new("storage_object_id"))),
            )
            .and_where(Expr::col((root.clone(), Alias::new("storage_root_id"))).eq(root_id))
            .and_where(
                Expr::col((root.clone(), Alias::new("parent_storage_object_id"))).eq(parent_id),
            )
            .and_where(Expr::col((root, Alias::new("presence_state"))).eq("Present"))
            .and_where(Expr::col((object.clone(), Alias::new("presence_state"))).eq("Present"))
            .order_by((object.clone(), Alias::new("name")), Order::Asc)
            .order_by((object, Alias::new("id")), Order::Asc)
            .limit(10_001)
            .to_owned();
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .into_iter()
            .map(|row| {
                Ok(LibraryFolderEntry {
                    id: row.try_get("", "id")?,
                    name: row.try_get("", "name")?,
                    is_directory: row.try_get::<String>("", "object_type")? == "Directory",
                    size: row.try_get("", "size")?,
                    modified_at: row.try_get("", "remote_modified_at")?,
                })
            })
            .collect()
    }
}
