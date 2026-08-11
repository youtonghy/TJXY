use std::{
    fs,
    net::TcpListener,
    process::{Command, Stdio},
    time::Duration,
};

use serde_json::{Value, json};
use tempfile::tempdir;

#[tokio::test]
async fn unconfigured_binary_serves_only_the_setup_runtime() {
    let directory = tempdir().unwrap();
    let dist = directory.path().join("dist");
    fs::create_dir_all(dist.join("assets")).unwrap();
    fs::create_dir_all(dist.join("brand")).unwrap();
    fs::write(
        dist.join("index.html"),
        "<!doctype html><div id=\"root\"></div>",
    )
    .unwrap();
    let port = unused_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_tjxy-server"))
        .env_remove("TJXY_SERVER_ID")
        .env_remove("TJXY_DATABASE_URL")
        .env_remove("TJXY_BOOTSTRAP_ADMIN_USERNAME")
        .env_remove("TJXY_BOOTSTRAP_ADMIN_PASSWORD")
        .env("TJXY_SERVER_ID", uuid::Uuid::new_v4().to_string())
        .env(
            "TJXY_DATABASE_URL",
            "postgresql://must-not-connect.invalid/tjxy",
        )
        .env(
            "TJXY_CONFIG_FILE",
            directory.path().join("config/tjxy.toml"),
        )
        .env("TJXY_SETUP_DATA_DIR", directory.path().join("data"))
        .env("TJXY_SETUP_BIND", format!("127.0.0.1:{port}"))
        .env("TJXY_ADMIN_DIST_DIR", &dist)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let status = wait_for(&client, &format!("{base}/Setup/Status")).await;
    assert_eq!(status.status(), reqwest::StatusCode::OK);
    let redirected_application = client.get(format!("{base}/app/")).send().await.unwrap();
    assert_eq!(redirected_application.status(), reqwest::StatusCode::OK);
    assert_eq!(redirected_application.url().path(), "/setup/");
    assert_eq!(
        client
            .get(format!("{base}/setup/"))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::OK
    );

    child.kill().unwrap();
    child.wait().unwrap();
}

#[tokio::test]
async fn completed_setup_transitions_into_application_without_exiting() {
    let directory = tempdir().unwrap();
    let dist = directory.path().join("dist");
    fs::create_dir_all(dist.join("assets")).unwrap();
    fs::create_dir_all(dist.join("brand")).unwrap();
    fs::write(
        dist.join("index.html"),
        "<!doctype html><div id=\"root\"></div>",
    )
    .unwrap();
    let port = unused_port();
    let config = directory.path().join("config/tjxy.toml");
    let data = directory.path().join("data");
    let mut server = server_command(&dist, &config, &data, port).spawn().unwrap();
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let status = wait_for(&client, &format!("{base}/Setup/Status")).await;
    let cookie = status.headers()[reqwest::header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let status_body: Value = status.json().await.unwrap();
    let csrf = status_body["CsrfToken"].as_str().unwrap();
    let database = json!({ "Backend": "sqlite", "Path": data.join("tjxy.db") });
    let database_test = client
        .post(format!("{base}/Setup/Database/Test"))
        .header(reqwest::header::COOKIE, &cookie)
        .header("x-tjxy-setup-csrf", csrf)
        .json(&database)
        .send()
        .await
        .unwrap();
    assert_eq!(database_test.status(), reqwest::StatusCode::OK);
    let completion = client
        .post(format!("{base}/Setup/Complete"))
        .header(reqwest::header::COOKIE, cookie)
        .header("x-tjxy-setup-csrf", csrf)
        .json(&json!({
            "SiteTitle": "TCP Setup",
            "SiteSubtitle": "Installed through setup",
            "Locale": "en-US",
            "LogoUrl": "/brand/tjxy-mark.webp",
            "IconUrl": "/brand/favicon.svg",
            "Database": database,
            "Network": { "ListenHost": "127.0.0.1", "Port": port, "PublicUrl": null },
            "AdministratorUsername": "admin",
            "AdministratorPassword": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    let completion_status = completion.status();
    let completion_body = completion.text().await.unwrap();
    assert_eq!(
        completion_status,
        reqwest::StatusCode::OK,
        "setup completion failed: {completion_body}"
    );
    let ready = wait_for(&client, &format!("{base}/health/ready")).await;
    assert_eq!(ready.status(), reqwest::StatusCode::OK);
    assert!(server.try_wait().unwrap().is_none());
    assert_eq!(
        client
            .get(format!("{base}/Setup/Status"))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::NOT_FOUND
    );
    server.kill().unwrap();
    server.wait().unwrap();
}

fn unused_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn server_command(
    dist: &std::path::Path,
    config: &std::path::Path,
    data: &std::path::Path,
    port: u16,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tjxy-server"));
    command
        .env_remove("TJXY_SERVER_ID")
        .env_remove("TJXY_DATABASE_URL")
        .env_remove("TJXY_CREDENTIAL_KEYRING")
        .env_remove("TJXY_BOOTSTRAP_ADMIN_USERNAME")
        .env_remove("TJXY_BOOTSTRAP_ADMIN_PASSWORD")
        .env("TJXY_CONFIG_FILE", config)
        .env("TJXY_SETUP_DATA_DIR", data)
        .env("TJXY_SETUP_BIND", format!("127.0.0.1:{port}"))
        .env("TJXY_ADMIN_DIST_DIR", dist)
        .env("TJXY_CONTAINER", "true")
        .env("TJXY_REDIS_MODE", "disabled")
        .env("TJXY_ENABLE_REMOTE_PROVIDERS", "false")
        .env("TJXY_FILESYSTEM_REALTIME", "false")
        .env("TJXY_MEDIA_REFRESH_INTERVAL_SECONDS", "0")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

async fn wait_for(client: &reqwest::Client, url: &str) -> reqwest::Response {
    for _ in 0..100 {
        if let Ok(response) = client.get(url).send().await
            && response.status().is_success()
        {
            return response;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("setup server did not become ready")
}
