//! Isolated local capacity fixture server. Never connects to an existing installation.
//! Usage: `cargo run -p tjxy-server --example capacity_server -- /tmp/new-run 274 600 [restart]`
mod capacity_support;

use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_server::{BootstrapAdmin, ServerIdentity, StartupOptions};
use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;
const MEDIA: &[u8] =
    include_bytes!("../tests/fixtures/jellyfin-smoke/Smoke Show/Season 01/Smoke Show S01E01.mp4");
// One deterministic source image. It is never copied into the application asset directory.
const POSTER: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 248, 15, 4, 0, 9, 251, 3,
    253, 251, 94, 107, 43, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[tokio::main]
async fn main() -> Result<(), Error> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&arguments.len()) {
        return Err("usage: capacity_server NEW_DIRECTORY ITEMS LIFETIME_SECONDS [restart]".into());
    }
    let root = PathBuf::from(&arguments[0]);
    let count = arguments[1].parse::<usize>()?;
    let lifetime = arguments[2].parse::<u64>()?;
    if !root.is_absolute() || !(1..=82_200).contains(&count) || !(30..=3600).contains(&lifetime) {
        return Err(
            "fixture bounds: absolute new directory, 1..82200 items, 30..3600 seconds".into(),
        );
    }
    let restart = arguments.get(3).is_some_and(|arg| arg == "restart");
    if restart {
        if fs::read_to_string(root.join(".tjxy-capacity"))? != "isolated-capacity-v1" {
            return Err("not a capacity fixture".into());
        }
        let _ = fs::remove_file(root.join("connection.json"));
    } else {
        create_fixture(&root, count)?;
    }
    let database_url = format!("sqlite://{}?mode=rwc", root.join("catalog.db").display());
    let database = sea_orm::Database::connect(&database_url).await?;
    tjxy_db::migrate_database(&database).await?;
    if !restart {
        let settings = tjxy_db::SystemSettingsInput {
            media_browser_roots: vec![root.join("media").display().to_string()],
            ..Default::default()
        };
        tjxy_db::SystemSettingsRepository::new(&database)
            .put(&settings, None)
            .await?;
    }
    let password = fs::read_to_string(root.join("admin-password"))?;
    let key: [u8; 32] = fs::read(root.join("credential-key"))?
        .try_into()
        .map_err(|_| "invalid fixture key")?;
    let cipher = Arc::new(CredentialCipher::new(
        CredentialKey::new(1, key)?,
        Vec::new(),
    )?);
    let (logging, _guard) = tjxy_server::LoggingRuntime::initialize(root.join("logs"))?;
    let measurements = Arc::new(capacity_support::SqlMeasurements::default());
    let observer = Arc::clone(&measurements);
    let state = tjxy_server::initialize(
        StartupOptions::new(
            database_url,
            ServerIdentity::new(Uuid::new_v4(), "TJXY isolated capacity", "local"),
        )
        .with_database_metric_callback(move |info| observer.observe(info))
        .with_bootstrap_admin(BootstrapAdmin::new("Capacity", &password))
        .with_assets_dir(root.join("assets"))
        .with_credential_cipher(cipher)
        .with_logging_runtime(Arc::new(logging))
        .with_lazy_wait_timeout(Duration::from_secs(5))
        .with_filesystem_realtime_enabled(false),
    )
    .await?;
    measurements.spawn(root.clone(), database);
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../admin/dist");
    let router = if dist.join("index.html").exists() {
        tjxy_server::build_router_with_admin_dist(state, &dist)?
    } else {
        tjxy_server::build_router(state)
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    private_file(&root.join("connection.json"),json!({"base_url":base,"username":"Capacity","password":password,"items":count,"pid":std::process::id()}).to_string().as_bytes())?;
    println!(
        "Isolated capacity server ready; connection file: {}",
        root.join("connection.json").display()
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::time::sleep(Duration::from_secs(lifetime)).await;
        })
        .await?;
    Ok(())
}

fn create_fixture(root: &Path, count: usize) -> Result<(), Error> {
    fs::create_dir(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    }
    fs::write(root.join(".tjxy-capacity"), "isolated-capacity-v1")?;
    private_file(
        &root.join("admin-password"),
        Uuid::new_v4().to_string().as_bytes(),
    )?;
    let mut key = [0_u8; 32];
    getrandom::fill(&mut key).map_err(|_| "OS randomness unavailable")?;
    private_file(&root.join("credential-key"), &key)?;
    fs::create_dir(root.join("media"))?;
    for index in 0..count {
        let title = format!("Capacity Movie {index:05}");
        let directory = root.join("media").join(format!("{title} (2001)"));
        fs::create_dir(&directory)?;
        fs::write(directory.join(format!("{title}.mp4")), MEDIA)?;
        fs::write(
            directory.join("movie.nfo"),
            format!(
                "<movie><title>{title}</title><year>2001</year><plot>Deterministic capacity fixture {index}.</plot><genre>Test</genre></movie>"
            ),
        )?;
        fs::write(directory.join("poster.png"), POSTER)?;
    }
    Ok(())
}
fn private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
}
