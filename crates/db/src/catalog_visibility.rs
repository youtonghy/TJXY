use sea_orm::sea_query::{Alias, Condition, Expr, JoinType, Query, SelectStatement};
use uuid::Uuid;

#[must_use]
pub fn catalog_item_visibility_condition(item: &Alias) -> Condition {
    Condition::all()
        .add(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .add(Expr::col((item.clone(), Alias::new("classification_state"))).eq("Matched"))
        .add(
            Condition::any()
                .add(Expr::exists(enabled_membership_for_item(item)))
                .add(Expr::exists(projected_enabled_membership(item))),
        )
}

pub(crate) fn projected_enabled_membership(item: &Alias) -> SelectStatement {
    projected_enabled_membership_in_library(item, None)
}

pub(crate) fn projected_enabled_membership_in_library(
    item: &Alias,
    library_id: Option<Uuid>,
) -> SelectStatement {
    let projection = Alias::new("image_projection");
    let publication = Alias::new("image_publication");
    let owner = Alias::new("image_publication_owner");
    let membership = Alias::new("image_owner_membership");
    let library = Alias::new("image_owner_library");
    let mut query = Query::select();
    query
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("publication_catalog_items"), projection.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id")))
                .equals((projection.clone(), Alias::new("publication_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            owner.clone(),
            Expr::col((owner.clone(), Alias::new("active_structure_publication_id")))
                .equals((publication.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                .equals((owner.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .and_where(
            Expr::col((projection, Alias::new("catalog_item_id")))
                .equals((item.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(Expr::col((publication, Alias::new("state"))).eq("Active"))
        .and_where(Expr::col((owner.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((owner, Alias::new("classification_state"))).eq("Matched"))
        .and_where(Expr::col((library.clone(), Alias::new("is_enabled"))).eq(true));
    if let Some(library_id) = library_id {
        query.and_where(Expr::col((library, Alias::new("id"))).eq(library_id));
    }
    query.clone()
}

pub(crate) fn enabled_membership_for_item(item: &Alias) -> SelectStatement {
    enabled_membership_for_item_in_library(item, None)
}

pub(crate) fn enabled_membership_for_item_in_library(
    item: &Alias,
    library_id: Option<Uuid>,
) -> SelectStatement {
    let membership = Alias::new("enabled_item_membership");
    let library = Alias::new("enabled_item_library");
    let mut query = Query::select();
    query
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("library_catalog_items"), membership.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership.clone(), Alias::new("library_id"))),
        )
        .and_where(
            Expr::col((membership, Alias::new("catalog_item_id")))
                .equals((item.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((library.clone(), Alias::new("is_enabled"))).eq(true));
    if let Some(library_id) = library_id {
        query.and_where(Expr::col((library, Alias::new("id"))).eq(library_id));
    }
    query.clone()
}
