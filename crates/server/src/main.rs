use std::{env, net::SocketAddr};

use thiserror::Error;
use tjxy_server::{AppState, ServerIdentity, build_router};
use uuid::Uuid;

#[derive(Debug, Error)]
enum StartupError {
    #[error("TJXY_SERVER_ID must contain a persistent UUID")]
    MissingServerId,
    #[error("TJXY_SERVER_ID is not a valid UUID: {0}")]
    InvalidServerId(#[source] uuid::Error),
    #[error("TJXY_BIND is not a valid socket address: {0}")]
    InvalidBindAddress(#[source] std::net::AddrParseError),
    #[error("failed to bind or serve HTTP: {0}")]
    Io(#[from] std::io::Error),
}

#[tokio::main]
async fn main() -> Result<(), StartupError> {
    let server_id = env::var("TJXY_SERVER_ID")
        .map_err(|_| StartupError::MissingServerId)
        .and_then(|value| Uuid::parse_str(&value).map_err(StartupError::InvalidServerId))?;
    let server_name = env::var("TJXY_SERVER_NAME").unwrap_or_else(|_| "TJXY".to_owned());
    let bind_address = env::var("TJXY_BIND").unwrap_or_else(|_| "127.0.0.1:8096".to_owned());
    let bind_address = bind_address
        .parse::<SocketAddr>()
        .map_err(StartupError::InvalidBindAddress)?;
    let mut identity = ServerIdentity::new(server_id, server_name, env::consts::OS);
    if let Ok(local_address) = env::var("TJXY_PUBLIC_ADDRESS") {
        identity = identity.with_local_address(local_address);
    }
    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(listener, build_router(AppState::new(identity))).await?;
    Ok(())
}
