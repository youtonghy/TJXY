//! Axum composition root and unauthenticated system discovery routes.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use tjxy_api::PublicSystemInfo;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerIdentity {
    id: Uuid,
    local_address: Option<String>,
    server_name: String,
    operating_system: String,
    startup_wizard_completed: bool,
}

impl ServerIdentity {
    #[must_use]
    pub fn new(
        id: Uuid,
        server_name: impl Into<String>,
        operating_system: impl Into<String>,
    ) -> Self {
        Self {
            id,
            local_address: None,
            server_name: server_name.into(),
            operating_system: operating_system.into(),
            startup_wizard_completed: false,
        }
    }

    #[must_use]
    pub const fn with_startup_wizard_completed(mut self, completed: bool) -> Self {
        self.startup_wizard_completed = completed;
        self
    }

    #[must_use]
    pub fn with_local_address(mut self, local_address: impl Into<String>) -> Self {
        self.local_address = Some(local_address.into());
        self
    }

    fn public_info(&self) -> PublicSystemInfo {
        PublicSystemInfo {
            local_address: self.local_address.clone(),
            server_name: self.server_name.clone(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            product_name: "TJXY".to_owned(),
            operating_system: self.operating_system.clone(),
            id: self.id,
            startup_wizard_completed: self.startup_wizard_completed,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppState {
    identity: Arc<ServerIdentity>,
    ready: Arc<AtomicBool>,
}

impl AppState {
    #[must_use]
    pub fn new(identity: ServerIdentity) -> Self {
        Self {
            identity: Arc::new(identity),
            ready: Arc::new(AtomicBool::new(false)),
        }
    }

    #[must_use]
    pub fn with_ready(self, ready: bool) -> Self {
        self.set_ready(ready);
        self
    }

    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::Release);
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/System/Info/Public", get(public_system_info))
        .route("/System/Ping", get(system_ping))
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .with_state(state)
}

async fn public_system_info(State(state): State<AppState>) -> Json<PublicSystemInfo> {
    Json(state.identity.public_info())
}

async fn system_ping() -> &'static str {
    "TJXY Server"
}

async fn liveness() -> &'static str {
    "live"
}

async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    if state.ready.load(Ordering::Acquire) {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready")
    }
}
