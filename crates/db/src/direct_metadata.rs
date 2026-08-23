use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr,
    sea_query::{Alias, Expr, JoinType, Order, Query},
};
use tjxy_common::{CatalogItemId, StorageObjectRecordId};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectMetadataObjectRecord {
    storage_object_id: StorageObjectRecordId,
    storage_account_id: Uuid,
    provider: String,
    provider_drive_id: String,
    provider_object_id: String,
    name: String,
    size: u64,
    remote_revision: Option<String>,
    resource_kind: String,
    priority: i32,
    input_revision: i64,
}

impl DirectMetadataObjectRecord {
    #[must_use]
    pub const fn storage_object_id(&self) -> StorageObjectRecordId {
        self.storage_object_id
    }
    #[must_use]
    pub const fn storage_account_id(&self) -> Uuid {
        self.storage_account_id
    }
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
    #[must_use]
    pub fn provider_drive_id(&self) -> &str {
        &self.provider_drive_id
    }
    #[must_use]
    pub fn provider_object_id(&self) -> &str {
        &self.provider_object_id
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }
    #[must_use]
    pub fn remote_revision(&self) -> Option<&str> {
        self.remote_revision.as_deref()
    }
    #[must_use]
    pub fn resource_kind(&self) -> &str {
        &self.resource_kind
    }
    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }
    #[must_use]
    pub const fn input_revision(&self) -> i64 {
        self.input_revision
    }
}

pub struct DirectMetadataRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> DirectMetadataRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Loads the authorized storage object backing one direct metadata resource.
    ///
    /// # Errors
    ///
    /// Returns a database error when the authorization snapshot cannot be read.
    #[allow(clippy::too_many_lines)] // Keeps object identity, root authorization, and current facts in one snapshot.
    pub async fn object(
        &self,
        item_id: CatalogItemId,
        resource_kind: &str,
        priority: i32,
    ) -> Result<Option<DirectMetadataObjectRecord>, DbErr> {
        let reference = Alias::new("direct_ref");
        let object = Alias::new("direct_object");
        let account = Alias::new("direct_account");
        let item = Alias::new("direct_item");
        let import_membership = Alias::new("direct_import_membership");
        let import_library = Alias::new("direct_import_library");
        let imported = Query::select()
            .expr(Expr::val(1_i32))
            .from_as(
                Alias::new("library_catalog_items"),
                import_membership.clone(),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("libraries"),
                import_library.clone(),
                Expr::col((import_library.clone(), Alias::new("id")))
                    .equals((import_membership.clone(), Alias::new("library_id"))),
            )
            .and_where(
                Expr::col((import_membership, Alias::new("catalog_item_id"))).eq(item_id.as_uuid()),
            )
            .and_where(Expr::col((import_library.clone(), Alias::new("is_enabled"))).eq(true))
            .and_where(
                sea_orm::sea_query::Condition::any()
                    .add(
                        Expr::col((import_library.clone(), Alias::new("metadata_source_mode")))
                            .eq("automatic_scrape"),
                    )
                    .add(
                        Expr::col((import_library, Alias::new("local_metadata_access_mode")))
                            .is_in(["import", "import_metadata_only"]),
                    )
                    .into(),
            )
            .limit(1)
            .to_owned();
        let query = Query::select()
            .columns([
                (object.clone(), Alias::new("id")),
                (object.clone(), Alias::new("storage_account_id")),
                (account.clone(), Alias::new("provider")),
                (object.clone(), Alias::new("provider_drive_id")),
                (object.clone(), Alias::new("provider_object_id")),
                (object.clone(), Alias::new("name")),
                (object.clone(), Alias::new("size")),
                (object.clone(), Alias::new("remote_revision")),
                (reference.clone(), Alias::new("resource_kind")),
                (reference.clone(), Alias::new("priority")),
                (reference.clone(), Alias::new("input_revision")),
            ])
            .from_as(Alias::new("direct_metadata_refs"), reference.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_objects"),
                object.clone(),
                Expr::col((object.clone(), Alias::new("id")))
                    .equals((reference.clone(), Alias::new("storage_object_id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_accounts"),
                account.clone(),
                Expr::col((account.clone(), Alias::new("id")))
                    .equals((object.clone(), Alias::new("storage_account_id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("catalog_items"),
                item.clone(),
                Expr::col((item.clone(), Alias::new("id")))
                    .equals((reference.clone(), Alias::new("catalog_item_id"))),
            )
            .and_where(
                Expr::col((reference.clone(), Alias::new("catalog_item_id"))).eq(item_id.as_uuid()),
            )
            .and_where(
                Expr::col((reference.clone(), Alias::new("resource_kind"))).eq(resource_kind),
            )
            .and_where(Expr::col((reference.clone(), Alias::new("priority"))).eq(priority))
            .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
            .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
            .and_where(Expr::exists(imported).not())
            // Direct metadata is intentionally not imported into catalog_items.  Its
            // durable readiness is represented by the reference row and its input
            // revision, so a later catalog metadata revision must not hide a still
            // readable source object.  Prefer the newest observation when multiple
            // references exist for the same library/resource.
            .order_by(
                (reference.clone(), Alias::new("input_revision")),
                Order::Desc,
            )
            .order_by((reference, Alias::new("library_id")), Order::Asc)
            .limit(1)
            .to_owned();
        self.database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
            .map(|row| {
                Ok(DirectMetadataObjectRecord {
                    storage_object_id: StorageObjectRecordId::from_uuid(row.try_get("", "id")?),
                    storage_account_id: row.try_get("", "storage_account_id")?,
                    provider: row.try_get("", "provider")?,
                    provider_drive_id: row.try_get("", "provider_drive_id")?,
                    provider_object_id: row.try_get("", "provider_object_id")?,
                    name: row.try_get("", "name")?,
                    size: row
                        .try_get::<i64>("", "size")?
                        .try_into()
                        .map_err(|_| DbErr::Custom("invalid direct metadata size".into()))?,
                    remote_revision: row.try_get("", "remote_revision")?,
                    resource_kind: row.try_get("", "resource_kind")?,
                    priority: row.try_get("", "priority")?,
                    input_revision: row.try_get("", "input_revision")?,
                })
            })
            .transpose()
    }
}
