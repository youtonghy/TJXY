use sea_orm::{
    ConnectionTrait, DatabaseConnection,
    sea_query::{Alias, Query},
};
use tjxy_common::{CatalogItemId, SortKey};
use uuid::Uuid;

pub async fn seed_visible_item(database: &DatabaseConnection, name: &str) -> CatalogItemId {
    let backend = database.get_database_backend();
    let library = Uuid::new_v4();
    let item = CatalogItemId::new();
    database
        .execute(
            backend.build(
                Query::insert()
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
                        Alias::new("collection_type"),
                        Alias::new("sort_key"),
                        Alias::new("is_enabled"),
                    ])
                    .values_panic([
                        library.into(),
                        name.into(),
                        "Lazy".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "on_browse".into(),
                        "on_playback".into(),
                        1.into(),
                        "movies".into(),
                        SortKey::from_text(name).into_bytes().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("item_type"),
                        Alias::new("name"),
                        Alias::new("sort_name"),
                        Alias::new("sort_key"),
                        Alias::new("classification_state"),
                        Alias::new("metadata_state"),
                        Alias::new("structure_state"),
                        Alias::new("source_state"),
                        Alias::new("structure_expansion_revision"),
                        Alias::new("source_index_revision"),
                        Alias::new("is_present"),
                    ])
                    .values_panic([
                        item.as_uuid().into(),
                        "Movie".into(),
                        name.into(),
                        name.to_lowercase().into(),
                        SortKey::from_text(name).into_bytes().into(),
                        "Matched".into(),
                        "Ready".into(),
                        "NotApplicable".into(),
                        "Indexed".into(),
                        0_i64.into(),
                        0_i64.into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    database
        .execute(
            backend.build(
                Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([Uuid::new_v4().into(), library.into(), item.as_uuid().into()]),
            ),
        )
        .await
        .unwrap();
    item
}
