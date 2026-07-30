use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Query, SimpleExpr},
};
use serde_json::Value;
use thiserror::Error;
use tjxy_common::SortKey;
use tjxy_metadata::{MetadataItemKind, RichCatalogItem, RichSeries};
use uuid::Uuid;

use crate::{AssetPublication, AssetRepositoryError};

const DEMO_NAMESPACE: Uuid = Uuid::from_u128(0x8fd6_fbd6_d3e4_4cf5_9488_49ce_0585_97aa);
const MOVIE_LIBRARY_KEY: &str = "library:movies";
const SERIES_LIBRARY_KEY: &str = "library:series";

#[derive(Clone, Debug)]
pub struct DemoCatalogPublication {
    movies: Vec<RichCatalogItem>,
    series: Vec<RichSeries>,
    language: String,
    fetched_at: DateTime<Utc>,
    assets: Vec<AssetPublication>,
}

impl DemoCatalogPublication {
    /// Creates a bounded publication containing movie roots and complete series trees.
    ///
    /// # Errors
    ///
    /// Returns an error when the publication is empty, contains a wrong root type, or
    /// repeats the same TMDB identity.
    pub fn new(
        movies: Vec<RichCatalogItem>,
        series: Vec<RichSeries>,
        language: impl Into<String>,
        fetched_at: DateTime<Utc>,
    ) -> Result<Self, DemoCatalogPublicationError> {
        let language = language.into();
        if movies.is_empty() && series.is_empty()
            || language.is_empty()
            || language.len() > 32
            || movies.len() > 100
            || series.len() > 100
        {
            return Err(DemoCatalogPublicationError::InvalidPublication);
        }
        let mut identities = HashSet::new();
        for movie in &movies {
            validate_item(movie, MetadataItemKind::Movie)?;
            if !identities.insert((item_kind(movie.kind()), movie.provider_id())) {
                return Err(DemoCatalogPublicationError::InvalidPublication);
            }
        }
        for show in &series {
            validate_item(show.item(), MetadataItemKind::Series)?;
            if !identities.insert((item_kind(show.item().kind()), show.item().provider_id())) {
                return Err(DemoCatalogPublicationError::InvalidPublication);
            }
            for season in show.seasons() {
                validate_item(season.item(), MetadataItemKind::Season)?;
                if !identities
                    .insert((item_kind(season.item().kind()), season.item().provider_id()))
                {
                    return Err(DemoCatalogPublicationError::InvalidPublication);
                }
                for episode in season.episodes() {
                    validate_item(episode.item(), MetadataItemKind::Episode)?;
                    if !identities.insert((
                        item_kind(episode.item().kind()),
                        episode.item().provider_id(),
                    )) {
                        return Err(DemoCatalogPublicationError::InvalidPublication);
                    }
                }
            }
        }
        Ok(Self {
            movies,
            series,
            language,
            fetched_at,
            assets: Vec::new(),
        })
    }

    /// Attaches already validated, content-addressed item images to this publication.
    ///
    /// # Errors
    ///
    /// Returns an error when the image count is outside the bounded import contract.
    pub fn with_assets(
        mut self,
        assets: Vec<AssetPublication>,
    ) -> Result<Self, DemoCatalogPublicationError> {
        if assets.len() > 1_000 {
            return Err(DemoCatalogPublicationError::InvalidPublication);
        }
        self.assets = assets;
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DemoCatalogPublicationReport {
    movies: usize,
    series: usize,
    seasons: usize,
    episodes: usize,
}

impl DemoCatalogPublicationReport {
    #[must_use]
    pub const fn movies(self) -> usize {
        self.movies
    }

    #[must_use]
    pub const fn series(self) -> usize {
        self.series
    }

    #[must_use]
    pub const fn seasons(self) -> usize {
        self.seasons
    }

    #[must_use]
    pub const fn episodes(self) -> usize {
        self.episodes
    }
}

pub struct DemoCatalogRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> DemoCatalogRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Atomically replaces the selected TMDB demo projection and advances one generation.
    ///
    /// No media sources or locations are created. Stable identities make replays replace
    /// the same catalog rows and associations.
    ///
    /// # Errors
    ///
    /// Returns validation, identity-conflict, database, commit, or rollback failures.
    pub async fn publish(
        &self,
        publication: &DemoCatalogPublication,
    ) -> Result<DemoCatalogPublicationReport, DemoCatalogPublicationError> {
        let transaction = self.database.begin().await?;
        let result = publish_in_transaction(&transaction, publication).await;
        finish(transaction, result).await
    }
}

#[derive(Debug, Error)]
pub enum DemoCatalogPublicationError {
    #[error("demo catalog publication is invalid")]
    InvalidPublication,
    #[error("demo catalog asset publication failed: {0}")]
    Asset(#[from] AssetRepositoryError),
    #[error("demo catalog database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("demo catalog rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

#[must_use]
pub fn demo_catalog_item_id(kind: MetadataItemKind, provider_id: u64) -> Uuid {
    stable_id(&format!("item:{}:{provider_id}", item_kind(kind)))
}

async fn publish_in_transaction(
    transaction: &DatabaseTransaction,
    publication: &DemoCatalogPublication,
) -> Result<DemoCatalogPublicationReport, DemoCatalogPublicationError> {
    let movie_library_id = upsert_library(
        transaction,
        MOVIE_LIBRARY_KEY,
        "TMDB Demo Movies",
        "movies",
        publication.fetched_at,
    )
    .await?;
    let series_library_id = upsert_library(
        transaction,
        SERIES_LIBRARY_KEY,
        "TMDB Demo Television",
        "tvshows",
        publication.fetched_at,
    )
    .await?;

    for movie in &publication.movies {
        publish_item(
            transaction,
            movie,
            None,
            movie_library_id,
            &publication.language,
            publication.fetched_at,
        )
        .await?;
    }

    let mut seasons = 0;
    let mut episodes = 0;
    for series in &publication.series {
        let series_id = publish_item(
            transaction,
            series.item(),
            None,
            series_library_id,
            &publication.language,
            publication.fetched_at,
        )
        .await?;
        for season in series.seasons() {
            seasons += 1;
            let season_id = publish_item(
                transaction,
                season.item(),
                Some(series_id),
                series_library_id,
                &publication.language,
                publication.fetched_at,
            )
            .await?;
            for episode in season.episodes() {
                episodes += 1;
                publish_item(
                    transaction,
                    episode.item(),
                    Some(season_id),
                    series_library_id,
                    &publication.language,
                    publication.fetched_at,
                )
                .await?;
            }
        }
    }
    for asset in &publication.assets {
        crate::asset::publish_in_transaction(transaction, asset, false).await?;
    }
    crate::advance_catalog_generation(transaction).await?;
    Ok(DemoCatalogPublicationReport {
        movies: publication.movies.len(),
        series: publication.series.len(),
        seasons,
        episodes,
    })
}

async fn upsert_library(
    transaction: &DatabaseTransaction,
    key: &str,
    name: &str,
    collection_type: &str,
    now: DateTime<Utc>,
) -> Result<Uuid, DbErr> {
    let id = stable_id(key);
    let query = Query::insert()
        .into_table(Alias::new("libraries"))
        .columns([
            Alias::new("id"),
            Alias::new("name"),
            Alias::new("scan_profile"),
            Alias::new("object_selection_scope"),
            Alias::new("metadata_policy"),
            Alias::new("expansion_policy"),
            Alias::new("probe_policy"),
            Alias::new("profile_version"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
            Alias::new("collection_type"),
            Alias::new("sort_key"),
            Alias::new("is_enabled"),
        ])
        .values_panic([
            id.into(),
            name.into(),
            "Demo".into(),
            "MetadataOnly".into(),
            "Tmdb".into(),
            "Imported".into(),
            "Disabled".into(),
            1_i32.into(),
            now.into(),
            now.into(),
            collection_type.into(),
            SortKey::from_text(name).into_bytes().into(),
            true.into(),
        ])
        .on_conflict(
            OnConflict::column(Alias::new("id"))
                .update_columns([
                    Alias::new("name"),
                    Alias::new("scan_profile"),
                    Alias::new("object_selection_scope"),
                    Alias::new("metadata_policy"),
                    Alias::new("expansion_policy"),
                    Alias::new("probe_policy"),
                    Alias::new("profile_version"),
                    Alias::new("updated_at"),
                    Alias::new("collection_type"),
                    Alias::new("sort_key"),
                    Alias::new("is_enabled"),
                ])
                .to_owned(),
        )
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&query))
        .await?;
    Ok(id)
}

async fn publish_item(
    transaction: &DatabaseTransaction,
    item: &RichCatalogItem,
    parent_id: Option<Uuid>,
    library_id: Uuid,
    language: &str,
    fetched_at: DateTime<Utc>,
) -> Result<Uuid, DemoCatalogPublicationError> {
    let item_id = demo_catalog_item_id(item.kind(), item.provider_id());
    upsert_catalog_item(transaction, item_id, parent_id, item, fetched_at).await?;
    upsert_membership(transaction, library_id, item_id).await?;
    clear_item_associations(transaction, item_id).await?;
    publish_provider_ids(transaction, item_id, item).await?;
    publish_named_links(
        transaction,
        item_id,
        "genres",
        "item_genres",
        "genre_id",
        item.genres(),
    )
    .await?;
    publish_named_links(
        transaction,
        item_id,
        "studios",
        "item_studios",
        "studio_id",
        item.studios(),
    )
    .await?;
    publish_countries(transaction, item_id, item).await?;
    publish_languages(transaction, item_id, item).await?;
    publish_credits(transaction, item_id, item).await?;
    publish_snapshot(transaction, item_id, item, language, fetched_at).await?;
    publish_provenance(transaction, item_id, item).await?;
    Ok(item_id)
}

async fn upsert_catalog_item(
    transaction: &DatabaseTransaction,
    item_id: Uuid,
    parent_id: Option<Uuid>,
    item: &RichCatalogItem,
    now: DateTime<Utc>,
) -> Result<(), DemoCatalogPublicationError> {
    let query = Query::insert()
        .into_table(Alias::new("catalog_items"))
        .columns(catalog_item_columns().map(Alias::new))
        .values_panic(catalog_item_values(item_id, parent_id, item, now)?)
        .on_conflict(
            OnConflict::column(Alias::new("id"))
                .update_columns(catalog_item_update_columns().map(Alias::new))
                .to_owned(),
        )
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&query))
        .await?;
    Ok(())
}

fn catalog_item_columns() -> [&'static str; 33] {
    [
        "id",
        "parent_id",
        "item_type",
        "name",
        "original_title",
        "sort_name",
        "production_year",
        "overview",
        "classification_state",
        "metadata_state",
        "structure_state",
        "source_state",
        "structure_expansion_revision",
        "source_index_revision",
        "active_structure_publication_id",
        "active_source_publication_id",
        "is_present",
        "last_error",
        "sort_key",
        "date_created",
        "metadata_revision",
        "metadata_resolved_revision",
        "metadata_resolved_requirement",
        "tagline",
        "community_rating",
        "vote_count",
        "runtime_ticks",
        "premiere_date",
        "end_date",
        "release_status",
        "official_rating",
        "original_language",
        "index_number",
    ]
}

fn catalog_item_update_columns() -> [&'static str; 32] {
    let columns = catalog_item_columns();
    std::array::from_fn(|index| columns[index + 1])
}

fn catalog_item_values(
    item_id: Uuid,
    parent_id: Option<Uuid>,
    item: &RichCatalogItem,
    now: DateTime<Utc>,
) -> Result<[SimpleExpr; 33], DemoCatalogPublicationError> {
    let vote_count = item
        .vote_count()
        .map(i64::try_from)
        .transpose()
        .map_err(|_| DemoCatalogPublicationError::InvalidPublication)?;
    let index_number = item
        .index_number()
        .map(i32::try_from)
        .transpose()
        .map_err(|_| DemoCatalogPublicationError::InvalidPublication)?;
    let structure_state = match item.kind() {
        MetadataItemKind::Series | MetadataItemKind::Season => "Expanded",
        MetadataItemKind::Movie | MetadataItemKind::Episode => "NotApplicable",
    };
    Ok([
        item_id.into(),
        parent_id.into(),
        item_kind(item.kind()).into(),
        item.title().into(),
        item.original_title().into(),
        item.title().into(),
        item.production_year().into(),
        item.overview().into(),
        "Matched".into(),
        "Ready".into(),
        structure_state.into(),
        "Missing".into(),
        1_i64.into(),
        0_i64.into(),
        Option::<Uuid>::None.into(),
        Option::<Uuid>::None.into(),
        true.into(),
        Option::<String>::None.into(),
        SortKey::from_text(item.title()).into_bytes().into(),
        now.into(),
        1_i64.into(),
        1_i64.into(),
        1_i32.into(),
        item.tagline().into(),
        item.community_rating().into(),
        vote_count.into(),
        item.runtime_ticks().into(),
        date_time(item.premiere_date()).into(),
        date_time(item.end_date()).into(),
        item.release_status().into(),
        item.official_rating().into(),
        item.original_language().into(),
        index_number.into(),
    ])
}

async fn upsert_membership(
    transaction: &DatabaseTransaction,
    library_id: Uuid,
    item_id: Uuid,
) -> Result<(), DbErr> {
    let id = stable_id(&format!("membership:{library_id}:{item_id}"));
    let query = Query::insert()
        .into_table(Alias::new("library_catalog_items"))
        .columns([
            Alias::new("id"),
            Alias::new("library_id"),
            Alias::new("catalog_item_id"),
        ])
        .values_panic([id.into(), library_id.into(), item_id.into()])
        .on_conflict(
            OnConflict::columns([Alias::new("library_id"), Alias::new("catalog_item_id")])
                .do_nothing()
                .to_owned(),
        )
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&query))
        .await?;
    Ok(())
}

async fn clear_item_associations(
    transaction: &DatabaseTransaction,
    item_id: Uuid,
) -> Result<(), DbErr> {
    for table in [
        "provider_ids",
        "metadata_provenance",
        "item_people",
        "item_genres",
        "item_studios",
        "item_countries",
        "item_languages",
        "metadata_snapshots",
    ] {
        let delete = Query::delete()
            .from_table(Alias::new(table))
            .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id))
            .to_owned();
        transaction
            .execute(transaction.get_database_backend().build(&delete))
            .await?;
    }
    Ok(())
}

async fn publish_provider_ids(
    transaction: &DatabaseTransaction,
    item_id: Uuid,
    item: &RichCatalogItem,
) -> Result<(), DbErr> {
    for (provider, provider_item_id) in item.provider_ids() {
        let id = stable_id(&format!(
            "provider-id:{item_id}:{provider}:{provider_item_id}"
        ));
        insert_values(
            transaction,
            "provider_ids",
            ["id", "catalog_item_id", "provider", "provider_item_id"],
            [
                id.into(),
                item_id.into(),
                provider.as_str().into(),
                provider_item_id.as_str().into(),
            ],
        )
        .await?;
    }
    Ok(())
}

async fn publish_named_links(
    transaction: &DatabaseTransaction,
    item_id: Uuid,
    entity_table: &str,
    link_table: &str,
    foreign_key: &str,
    names: &[String],
) -> Result<(), DbErr> {
    for name in names {
        let entity_id = stable_id(&format!("{entity_table}:{name}"));
        let entity = Query::insert()
            .into_table(Alias::new(entity_table))
            .columns([Alias::new("id"), Alias::new("name")])
            .values_panic([entity_id.into(), name.as_str().into()])
            .on_conflict(
                OnConflict::column(Alias::new("name"))
                    .update_column(Alias::new("name"))
                    .to_owned(),
            )
            .to_owned();
        transaction
            .execute(transaction.get_database_backend().build(&entity))
            .await?;
        insert_values(
            transaction,
            link_table,
            ["id", "catalog_item_id", foreign_key],
            [
                stable_id(&format!("{link_table}:{item_id}:{entity_id}")).into(),
                item_id.into(),
                entity_id.into(),
            ],
        )
        .await?;
    }
    Ok(())
}

async fn publish_countries(
    transaction: &DatabaseTransaction,
    item_id: Uuid,
    item: &RichCatalogItem,
) -> Result<(), DemoCatalogPublicationError> {
    for (order, country) in item.countries().iter().enumerate() {
        let country_id = stable_id(&format!("country:{}", country.code()));
        upsert_code_name(
            transaction,
            "countries",
            country_id,
            country.code(),
            country.name(),
        )
        .await?;
        publish_ordered_link(
            transaction,
            "item_countries",
            "country_id",
            item_id,
            country_id,
            order,
        )
        .await?;
    }
    Ok(())
}

async fn publish_languages(
    transaction: &DatabaseTransaction,
    item_id: Uuid,
    item: &RichCatalogItem,
) -> Result<(), DemoCatalogPublicationError> {
    for (order, language) in item.languages().iter().enumerate() {
        let language_id = stable_id(&format!("language:{}", language.code()));
        upsert_code_name(
            transaction,
            "languages",
            language_id,
            language.code(),
            language.name(),
        )
        .await?;
        publish_ordered_link(
            transaction,
            "item_languages",
            "language_id",
            item_id,
            language_id,
            order,
        )
        .await?;
    }
    Ok(())
}

async fn publish_ordered_link(
    transaction: &DatabaseTransaction,
    table: &str,
    foreign_key: &str,
    item_id: Uuid,
    foreign_id: Uuid,
    order: usize,
) -> Result<(), DemoCatalogPublicationError> {
    let sort_order =
        i32::try_from(order).map_err(|_| DemoCatalogPublicationError::InvalidPublication)?;
    insert_values(
        transaction,
        table,
        ["id", "catalog_item_id", foreign_key, "sort_order"],
        [
            stable_id(&format!("{table}:{item_id}:{foreign_id}")).into(),
            item_id.into(),
            foreign_id.into(),
            sort_order.into(),
        ],
    )
    .await?;
    Ok(())
}

async fn upsert_code_name(
    transaction: &DatabaseTransaction,
    table: &str,
    id: Uuid,
    code: &str,
    name: &str,
) -> Result<(), DbErr> {
    let query = Query::insert()
        .into_table(Alias::new(table))
        .columns([Alias::new("id"), Alias::new("code"), Alias::new("name")])
        .values_panic([id.into(), code.into(), name.into()])
        .on_conflict(
            OnConflict::column(Alias::new("code"))
                .update_column(Alias::new("name"))
                .to_owned(),
        )
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&query))
        .await?;
    Ok(())
}

async fn publish_credits(
    transaction: &DatabaseTransaction,
    item_id: Uuid,
    item: &RichCatalogItem,
) -> Result<(), DemoCatalogPublicationError> {
    for credit in item.credits() {
        let person_id = stable_id(&format!("person:tmdb:{}", credit.person_provider_id()));
        let person = Query::insert()
            .into_table(Alias::new("people"))
            .columns([
                Alias::new("id"),
                Alias::new("name"),
                Alias::new("sort_name"),
            ])
            .values_panic([
                person_id.into(),
                credit.person_name().into(),
                credit.person_name().into(),
            ])
            .on_conflict(
                OnConflict::column(Alias::new("id"))
                    .update_columns([Alias::new("name"), Alias::new("sort_name")])
                    .to_owned(),
            )
            .to_owned();
        transaction
            .execute(transaction.get_database_backend().build(&person))
            .await?;
        let provider_person_id = credit.person_provider_id().to_string();
        let person_provider = Query::insert()
            .into_table(Alias::new("person_provider_ids"))
            .columns([
                Alias::new("id"),
                Alias::new("person_id"),
                Alias::new("provider"),
                Alias::new("provider_person_id"),
            ])
            .values_panic([
                stable_id(&format!("person-provider:tmdb:{provider_person_id}")).into(),
                person_id.into(),
                "tmdb".into(),
                provider_person_id.into(),
            ])
            .on_conflict(
                OnConflict::columns([Alias::new("provider"), Alias::new("provider_person_id")])
                    .update_column(Alias::new("person_id"))
                    .to_owned(),
            )
            .to_owned();
        transaction
            .execute(transaction.get_database_backend().build(&person_provider))
            .await?;

        let role = credit.role().unwrap_or(credit.credit_type());
        let sort_order = i32::try_from(credit.order())
            .map_err(|_| DemoCatalogPublicationError::InvalidPublication)?;
        insert_values(
            transaction,
            "item_people",
            [
                "id",
                "catalog_item_id",
                "person_id",
                "role",
                "sort_order",
                "credit_type",
            ],
            [
                stable_id(&format!("credit:{item_id}:{person_id}:{role}")).into(),
                item_id.into(),
                person_id.into(),
                role.into(),
                sort_order.into(),
                credit.credit_type().into(),
            ],
        )
        .await?;
    }
    Ok(())
}

async fn publish_snapshot(
    transaction: &DatabaseTransaction,
    item_id: Uuid,
    item: &RichCatalogItem,
    language: &str,
    fetched_at: DateTime<Utc>,
) -> Result<(), DbErr> {
    insert_values(
        transaction,
        "metadata_snapshots",
        [
            "id",
            "catalog_item_id",
            "person_id",
            "provider",
            "entity_kind",
            "provider_entity_id",
            "language",
            "fetched_at",
            "payload",
        ],
        [
            stable_id(&format!(
                "snapshot:{}:{}:{language}",
                item_kind(item.kind()),
                item.provider_id()
            ))
            .into(),
            item_id.into(),
            Option::<Uuid>::None.into(),
            "tmdb".into(),
            item_kind(item.kind()).into(),
            item.provider_id().to_string().into(),
            language.into(),
            fetched_at.into(),
            item.snapshot().clone().into(),
        ],
    )
    .await
}

async fn publish_provenance(
    transaction: &DatabaseTransaction,
    item_id: Uuid,
    item: &RichCatalogItem,
) -> Result<(), DbErr> {
    for (field, value) in [
        ("name", Value::String(item.title().to_owned())),
        (
            "overview",
            item.overview()
                .map_or(Value::Null, |value| Value::String(value.to_owned())),
        ),
        (
            "community_rating",
            item.community_rating()
                .and_then(serde_json::Number::from_f64)
                .map_or(Value::Null, Value::Number),
        ),
    ] {
        let value_hash = value_hash(&value);
        insert_values(
            transaction,
            "metadata_provenance",
            [
                "id",
                "catalog_item_id",
                "field_name",
                "source_provider",
                "source_reference",
                "value_hash",
            ],
            [
                stable_id(&format!("provenance:{item_id}:{field}:tmdb")).into(),
                item_id.into(),
                field.into(),
                "tmdb".into(),
                item.provider_id().to_string().into(),
                value_hash.into(),
            ],
        )
        .await?;
    }
    Ok(())
}

async fn insert_values<const COLUMNS: usize, const VALUES: usize>(
    transaction: &DatabaseTransaction,
    table: &str,
    columns: [&str; COLUMNS],
    values: [SimpleExpr; VALUES],
) -> Result<(), DbErr> {
    debug_assert_eq!(COLUMNS, VALUES);
    let query = Query::insert()
        .into_table(Alias::new(table))
        .columns(columns.map(Alias::new))
        .values_panic(values)
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&query))
        .await?;
    Ok(())
}

fn validate_item(
    item: &RichCatalogItem,
    expected: MetadataItemKind,
) -> Result<(), DemoCatalogPublicationError> {
    if item.kind() != expected
        || item.provider_id() == 0
        || item.title().is_empty()
        || item.title().chars().count() > 512
    {
        return Err(DemoCatalogPublicationError::InvalidPublication);
    }
    Ok(())
}

fn item_kind(kind: MetadataItemKind) -> &'static str {
    match kind {
        MetadataItemKind::Movie => "Movie",
        MetadataItemKind::Series => "Series",
        MetadataItemKind::Season => "Season",
        MetadataItemKind::Episode => "Episode",
    }
}

fn stable_id(value: &str) -> Uuid {
    Uuid::new_v5(&DEMO_NAMESPACE, value.as_bytes())
}

fn date_time(value: Option<NaiveDate>) -> Option<DateTime<Utc>> {
    value.and_then(|date| {
        date.and_hms_opt(0, 0, 0)
            .map(|date_time| date_time.and_utc())
    })
}

fn value_hash(value: &Value) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, DemoCatalogPublicationError>,
) -> Result<T, DemoCatalogPublicationError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(rollback) => Err(DemoCatalogPublicationError::RollbackFailed {
                original: error.to_string(),
                rollback,
            }),
        },
    }
}
