use std::{fs, path::Path};

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, get_service},
};
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
    #[error("Jellyfin Web distribution directory is missing or is not a directory")]
    MissingJellyfinWebDistribution,
    #[error("Jellyfin Web distribution index.html is missing or is not a file")]
    MissingJellyfinWebIndex,
    #[error("Jellyfin Web distribution index.html could not be read")]
    UnreadableJellyfinWebIndex,
}

pub(super) fn router(dist_dir: &Path) -> Result<Router, AdminAssetsError> {
    router_with_jellyfin_web(dist_dir, None)
}

pub(super) fn router_with_jellyfin_web(
    dist_dir: &Path,
    jellyfin_web_dist_dir: Option<&Path>,
) -> Result<Router, AdminAssetsError> {
    let index_path = distribution(dist_dir)?;
    let fallback_index_path = index_path.clone();
    let app_fallback_index_path = index_path.clone();
    let jellyfin_web_dist_dir = jellyfin_web_dist_dir
        .map(jellyfin_web_distribution)
        .transpose()?;

    let router = Router::new()
        .route("/admin", get(|| async { Redirect::permanent("/admin/") }))
        .route("/setup", get(|| async { Redirect::temporary("/app/") }))
        .route("/setup/", get(|| async { Redirect::temporary("/app/") }))
        .route(
            "/setup/{*path}",
            get(|| async { Redirect::temporary("/app/") }),
        )
        .route_service("/app/", get_service(ServeFile::new(index_path.clone())))
        .route_service("/admin/", get_service(ServeFile::new(index_path)))
        .nest_service("/assets", ServeDir::new(dist_dir.join("assets")))
        .nest_service("/brand", ServeDir::new(dist_dir.join("brand")))
        .nest_service("/admin/assets", ServeDir::new(dist_dir.join("assets")))
        .route(
            "/app/{*path}",
            get(move |headers: HeaderMap| {
                let index_path = app_fallback_index_path.clone();
                async move { spa_fallback(&headers, &index_path).await }
            }),
        )
        .route(
            "/admin/{*path}",
            get(move |headers: HeaderMap| {
                let index_path = fallback_index_path.clone();
                async move { spa_fallback(&headers, &index_path).await }
            }),
        );
    let router = if let Some(web_dist_dir) = jellyfin_web_dist_dir {
        router
            .route("/", get(|| async { Redirect::permanent("/web/") }))
            .nest_service(
                "/web",
                ServeDir::new(web_dist_dir).append_index_html_on_directories(true),
            )
    } else {
        router.route("/", get(|| async { Redirect::permanent("/app/") }))
    };
    Ok(router)
}

pub(super) fn setup_router(dist_dir: &Path) -> Result<Router, AdminAssetsError> {
    let index_path = distribution(dist_dir)?;
    let fallback_index_path = index_path.clone();
    Ok(Router::new()
        .route("/", get(|| async { Redirect::temporary("/setup/") }))
        .route("/setup", get(|| async { Redirect::temporary("/setup/") }))
        .route("/app", get(|| async { Redirect::temporary("/setup/") }))
        .route("/app/", get(|| async { Redirect::temporary("/setup/") }))
        .route(
            "/app/{*path}",
            get(|| async { Redirect::temporary("/setup/") }),
        )
        .route("/admin", get(|| async { Redirect::temporary("/setup/") }))
        .route("/admin/", get(|| async { Redirect::temporary("/setup/") }))
        .route(
            "/admin/{*path}",
            get(|| async { Redirect::temporary("/setup/") }),
        )
        .route_service("/setup/", get_service(ServeFile::new(index_path)))
        .nest_service("/assets", ServeDir::new(dist_dir.join("assets")))
        .nest_service("/brand", ServeDir::new(dist_dir.join("brand")))
        .route(
            "/setup/{*path}",
            get(move |headers: HeaderMap| {
                let index_path = fallback_index_path.clone();
                async move { spa_fallback(&headers, &index_path).await }
            }),
        ))
}

fn distribution(dist_dir: &Path) -> Result<std::path::PathBuf, AdminAssetsError> {
    if !dist_dir.is_dir() {
        return Err(AdminAssetsError::MissingDistribution);
    }
    let index_path = dist_dir.join("index.html");
    if !index_path.is_file() {
        return Err(AdminAssetsError::MissingIndex);
    }
    fs::read(&index_path).map_err(|_| AdminAssetsError::UnreadableIndex)?;
    Ok(index_path)
}

fn jellyfin_web_distribution(dist_dir: &Path) -> Result<std::path::PathBuf, AdminAssetsError> {
    if !dist_dir.is_dir() {
        return Err(AdminAssetsError::MissingJellyfinWebDistribution);
    }
    let index_path = dist_dir.join("index.html");
    if !index_path.is_file() {
        return Err(AdminAssetsError::MissingJellyfinWebIndex);
    }
    fs::read(&index_path).map_err(|_| AdminAssetsError::UnreadableJellyfinWebIndex)?;
    Ok(dist_dir.to_owned())
}

async fn spa_fallback(headers: &HeaderMap, index_path: &Path) -> Response {
    if !accepts_html(headers) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(index) = tokio::fs::read(index_path).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(index))
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
