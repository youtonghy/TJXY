use std::{fs, path::Path, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, get_service},
};
use bytes::Bytes;
use thiserror::Error;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Debug, Error)]
pub enum AdminAssetsError {
    #[error("admin distribution directory is missing or is not a directory")]
    MissingDistribution,
    #[error("admin distribution index.html is missing or is not a file")]
    MissingIndex,
    #[error("admin distribution index.html could not be read")]
    UnreadableIndex,
}

pub(super) fn router(dist_dir: &Path) -> Result<Router, AdminAssetsError> {
    if !dist_dir.is_dir() {
        return Err(AdminAssetsError::MissingDistribution);
    }
    let index_path = dist_dir.join("index.html");
    if !index_path.is_file() {
        return Err(AdminAssetsError::MissingIndex);
    }
    let index = Arc::new(Bytes::from(
        fs::read(&index_path).map_err(|_| AdminAssetsError::UnreadableIndex)?,
    ));
    let fallback_index = Arc::clone(&index);
    let app_fallback_index = Arc::clone(&index);

    Ok(Router::new()
        .route("/", get(|| async { Redirect::permanent("/app/") }))
        .route("/admin", get(|| async { Redirect::permanent("/admin/") }))
        .route_service("/app/", get_service(ServeFile::new(index_path.clone())))
        .route_service("/admin/", get_service(ServeFile::new(index_path)))
        .nest_service("/assets", ServeDir::new(dist_dir.join("assets")))
        .nest_service("/admin/assets", ServeDir::new(dist_dir.join("assets")))
        .route(
            "/app/{*path}",
            get(move |headers: HeaderMap| {
                let index = Arc::clone(&app_fallback_index);
                async move { spa_fallback(&headers, &index) }
            }),
        )
        .route(
            "/admin/{*path}",
            get(move |headers: HeaderMap| {
                let index = Arc::clone(&fallback_index);
                async move { spa_fallback(&headers, &index) }
            }),
        ))
}

fn spa_fallback(headers: &HeaderMap, index: &Bytes) -> Response {
    if !accepts_html(headers) {
        return StatusCode::NOT_FOUND.into_response();
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(index.clone()))
        .expect("static response headers are valid")
}

fn accepts_html(headers: &HeaderMap) -> bool {
    headers.get(header::ACCEPT).is_none_or(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .any(|item| matches!(item.trim().split(';').next(), Some("text/html" | "*/*")))
        })
    })
}
