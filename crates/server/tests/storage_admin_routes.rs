use std::{collections::HashMap, sync::Arc};

use axum::{
    Form, Json, Router,
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode, header},
    routing::{get, post as route_post},
};
use http_body_util::BodyExt;
use sea_orm::{
    ConnectionTrait,
    sea_query::{Alias, Expr, Query},
};
use serde_json::{Value, json};
use tempfile::TempDir;
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_server::{
    BootstrapAdmin, GoogleDriveOAuthConfiguration, MicrosoftOneDriveOAuthConfiguration,
    ServerIdentity, StartupOptions, build_router, initialize,
};
use tjxy_test_support::reconnectable_test_database;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn legacy_onedrive_direct_binding_is_unavailable_and_keyring_is_still_required() {
    let assets = TempDir::new().unwrap();
    let database = reconnectable_test_database().await.unwrap();
    let state = initialize(
        StartupOptions::new(
            database.database_url(),
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(assets.path())
        .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password")),
    )
    .await
    .unwrap();
    let app = build_router(state);
    let token = login(&app).await;
    let base = json!({
        "TargetLibraryId":Uuid::new_v4(),
        "DisplayName":"Personal Drive",
        "AccountIdentity":"account@example.invalid",
        "DriveId":"drive-id",
        "RootObjectId":"root-item-id",
        "ClientId":"client-id",
        "ClientSecret":null,
        "RefreshToken":"refresh-secret"
    });

    let mut legacy = base;
    legacy["AccountType"] = json!("Personal");
    let unavailable = post(&app, &token, legacy).await;
    assert_eq!(unavailable, StatusCode::NOT_FOUND);

    let invalid_google = post_to(
        &app,
        &token,
        "/Admin/Storage/Accounts/GoogleDrive",
        json!({
            "TargetLibraryId":Uuid::new_v4(),
            "Scope":"MyDrive",
            "DisplayName":"My Drive",
            "AccountIdentity":"account@example.invalid",
            "SharedDriveId":"must-not-be-present",
            "ClientId":"client-id",
            "ClientSecret":"client-secret",
            "RefreshToken":"refresh-secret"
        }),
    )
    .await;
    assert_eq!(invalid_google, StatusCode::NOT_FOUND);

    let import_without_keyring = post_to(
        &app,
        &token,
        "/Admin/Imports/Emby",
        json!({
            "BaseUrl":"http://127.0.0.1:8096",
            "EmbyUserId":"emby-user",
            "ApiKey":"api-secret",
            "SourceInstanceId":"legacy-server",
            "DryRun":true,
            "TargetLibraryId":Uuid::new_v4(),
            "TargetUserId":Uuid::new_v4()
        }),
    )
    .await;
    assert_eq!(import_without_keyring, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn relink_admin_queue_is_available_without_a_credential_keyring() {
    let assets = TempDir::new().unwrap();
    let database = reconnectable_test_database().await.unwrap();
    let state = initialize(
        StartupOptions::new(
            database.database_url(),
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(assets.path())
        .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password")),
    )
    .await
    .unwrap();
    let app = build_router(state);
    let token = login(&app).await;

    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/Admin/Storage/RelinkCandidates")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/Admin/Storage/RelinkCandidates?Limit=25")
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&list.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["Items"], json!([]));
    let missing = command(
        &app,
        &token,
        &format!("/Admin/Storage/RelinkCandidates/{}/Confirm", Uuid::new_v4()),
    )
    .await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn emby_import_admin_creates_encrypted_durable_work_and_controls_it() {
    let data = TempDir::new().unwrap();
    let database_fixture = reconnectable_test_database().await.unwrap();
    let database_url = database_fixture.database_url();
    let cipher = Arc::new(
        CredentialCipher::new(CredentialKey::new(1, [5_u8; 32]).unwrap(), Vec::new()).unwrap(),
    );
    let state = initialize(
        StartupOptions::new(
            database_url,
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(data.path().join("assets"))
        .with_credential_cipher(cipher)
        .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password")),
    )
    .await
    .unwrap();
    let app = build_router(state);
    let (token, user_id) = login_with_user(&app).await;
    let database = database_fixture.connection().clone();
    let library_id = seed_library(&database).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Admin/Imports/Emby")
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "BaseUrl":"http://127.0.0.1:1",
                        "EmbyUserId":"emby-user",
                        "ApiKey":"api-secret-never-store-plain",
                        "SourceInstanceId":"legacy-server",
                        "DryRun":true,
                        "TargetLibraryId":library_id,
                        "TargetUserId":user_id
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    let job_id = body["JobId"].as_str().unwrap();
    let backend = database.get_database_backend();
    let encrypted = database
        .query_one(
            backend.build(
                Query::select()
                    .column(Alias::new("encrypted_payload"))
                    .from(Alias::new("import_sources")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<Vec<u8>>("", "encrypted_payload")
        .unwrap();
    assert!(
        !encrypted
            .windows(b"api-secret-never-store-plain".len())
            .any(|window| window == b"api-secret-never-store-plain")
    );

    let pause_status = command(&app, &token, &format!("/Admin/Imports/{job_id}/Pause")).await;
    assert_eq!(pause_status, StatusCode::NO_CONTENT);
    let status = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/Admin/Imports/{job_id}"))
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let body = status.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["State"], "Paused");
    assert_eq!(
        command(&app, &token, &format!("/Admin/Imports/{job_id}/Resume")).await,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn google_drive_oauth_uses_server_side_pkce_and_persists_only_encrypted_credentials() {
    let fake = FakeGoogle::start().await;
    let data = TempDir::new().unwrap();
    let database_fixture = reconnectable_test_database().await.unwrap();
    let database_url = database_fixture.database_url();
    let cipher = Arc::new(
        CredentialCipher::new(CredentialKey::new(1, [9_u8; 32]).unwrap(), Vec::new()).unwrap(),
    );
    let oauth = GoogleDriveOAuthConfiguration::new(
        "server-client-id",
        "server-client-secret",
        "http://127.0.0.1:8096/Admin/Storage/OAuth/GoogleDrive/Callback",
    )
    .unwrap()
    .with_endpoints(
        format!("{}/authorize", fake.base_url),
        format!("{}/token", fake.base_url),
        format!("{}/drive/v3", fake.base_url),
    )
    .unwrap();
    let state = initialize(
        StartupOptions::new(
            database_url,
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(data.path().join("assets"))
        .with_credential_cipher(cipher)
        .with_google_oauth(oauth)
        .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password")),
    )
    .await
    .unwrap();
    let app = build_router(state);
    let (token, _) = login_with_user(&app).await;
    let library_id = seed_library(database_fixture.connection()).await;

    let start = json_request(
        &app,
        "POST",
        "/Admin/Storage/OAuth/GoogleDrive/Start",
        Some(&token),
        Some(json!({"TargetLibraryId":library_id})),
    )
    .await;
    assert_eq!(start.status(), StatusCode::OK);
    let start: Value =
        serde_json::from_slice(&start.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let oauth_state = start["State"].as_str().unwrap();
    let authorization_url = start["AuthorizationUrl"].as_str().unwrap();
    assert!(!authorization_url.contains("server-client-secret"));
    assert!(!start.to_string().contains("server-client-secret"));
    let authorization_url = reqwest::Url::parse(authorization_url).unwrap();
    let authorization: HashMap<_, _> = authorization_url.query_pairs().into_owned().collect();
    assert_eq!(authorization["state"], oauth_state);
    assert_eq!(authorization["code_challenge_method"], "S256");
    assert!(authorization["code_challenge"].len() >= 43);
    assert_eq!(authorization["access_type"], "offline");

    let callback = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/GoogleDrive/Callback?state={oauth_state}&code=authorization-code"
        ),
        None,
        None,
    )
    .await;
    assert_eq!(callback.status(), StatusCode::NO_CONTENT);
    let replay = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/GoogleDrive/Callback?state={oauth_state}&code=authorization-code"
        ),
        None,
        None,
    )
    .await;
    assert_eq!(replay.status(), StatusCode::BAD_REQUEST);

    let shared_drives = json_request(
        &app,
        "GET",
        &format!("/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/SharedDrives"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(shared_drives.status(), StatusCode::OK);
    let shared_drives: Value = serde_json::from_slice(
        &shared_drives
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(
        shared_drives,
        json!({"Items":[{"Id":"team-drive","Name":"Team Media"}],"NextPageToken":"shared-page-2"})
    );
    let shared_page_two = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/SharedDrives?PageToken=shared-page-2"
        ),
        Some(&token),
        None,
    )
    .await;
    let shared_page_two: Value = serde_json::from_slice(
        &shared_page_two
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(
        shared_page_two,
        json!({"Items":[{"Id":"archive-drive","Name":"Archive"}],"NextPageToken":null})
    );

    let directories = json_request(
        &app,
        "GET",
        &format!("/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=MyDrive"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(directories.status(), StatusCode::OK);
    let directories: Value =
        serde_json::from_slice(&directories.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert_eq!(
        directories["Items"],
        json!([{"Id":"media-folder","Name":"Media"}])
    );
    let first_cursor = Uuid::parse_str(directories["NextPageToken"].as_str().unwrap()).unwrap();
    assert!(!directories.to_string().contains("google-provider-page-2"));

    let second = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=MyDrive&PageToken={first_cursor}"
        ),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(second.status(), StatusCode::OK);
    let second: Value =
        serde_json::from_slice(&second.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        second["Items"],
        json!([
            {"Id":"media-folder","Name":"Media duplicate"},
            {"Id":"archive-folder","Name":"Archive"}
        ])
    );
    let second_cursor = Uuid::parse_str(second["NextPageToken"].as_str().unwrap()).unwrap();
    assert!(!second.to_string().contains("google-provider-page-3"));

    let replay = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=MyDrive&PageToken={first_cursor}"
        ),
        Some(&token),
        None,
    )
    .await;
    let replay: Value =
        serde_json::from_slice(&replay.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(replay["NextPageToken"], second_cursor.to_string());

    let final_page = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=MyDrive&PageToken={second_cursor}"
        ),
        Some(&token),
        None,
    )
    .await;
    let final_page: Value =
        serde_json::from_slice(&final_page.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert_eq!(final_page, json!({"Items":[],"NextPageToken":null}));

    for invalid_uri in [
        format!(
            "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=MyDrive&ParentId=other&PageToken={first_cursor}"
        ),
        format!(
            "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=SharedDrive&SharedDriveId=team-drive&PageToken={first_cursor}"
        ),
        format!(
            "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=MyDrive&PageToken=not-a-uuid"
        ),
    ] {
        let calls_before_invalid = fake.child_queries.lock().await.len();
        let invalid = json_request(&app, "GET", &invalid_uri, Some(&token), None).await;
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert_eq!(fake.child_queries.lock().await.len(), calls_before_invalid);
    }

    let (other_token, _) = login_with_user(&app).await;
    let calls_before_wrong_owner = fake.child_queries.lock().await.len();
    let wrong_owner = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=MyDrive&PageToken={first_cursor}"
        ),
        Some(&other_token),
        None,
    )
    .await;
    assert_eq!(wrong_owner.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        fake.child_queries.lock().await.len(),
        calls_before_wrong_owner
    );

    let other_start = json_request(
        &app,
        "POST",
        "/Admin/Storage/OAuth/GoogleDrive/Start",
        Some(&token),
        Some(json!({"TargetLibraryId":library_id})),
    )
    .await;
    let other_start: Value =
        serde_json::from_slice(&other_start.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    let other_oauth_state = other_start["State"].as_str().unwrap();
    let calls_before_pending = fake.child_queries.lock().await.len();
    let pending = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/GoogleDrive/{other_oauth_state}/Directories?Scope=MyDrive&PageToken={first_cursor}"
        ),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(pending.status(), StatusCode::CONFLICT);
    assert_eq!(fake.child_queries.lock().await.len(), calls_before_pending);
    let other_callback = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/GoogleDrive/Callback?state={other_oauth_state}&code=other-authorization-code"
        ),
        None,
        None,
    )
    .await;
    assert_eq!(other_callback.status(), StatusCode::NO_CONTENT);
    let calls_before_other_state = fake.child_queries.lock().await.len();
    let wrong_state = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/GoogleDrive/{other_oauth_state}/Directories?Scope=MyDrive&PageToken={first_cursor}"
        ),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(wrong_state.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        fake.child_queries.lock().await.len(),
        calls_before_other_state
    );

    let browser_supplied_identity = json_request(
        &app,
        "POST",
        &format!("/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Bind"),
        Some(&token),
        Some(json!({
            "Scope":"MyDrive",
            "DisplayName":"Google Media",
            "AccountIdentity":"forged@example.invalid",
            "RootObjectId":"media-folder"
        })),
    )
    .await;
    assert_eq!(browser_supplied_identity.status(), StatusCode::BAD_REQUEST);

    let binding = json_request(
        &app,
        "POST",
        &format!("/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Bind"),
        Some(&token),
        Some(json!({
            "Scope":"MyDrive",
            "DisplayName":"Google Media",
            "RootObjectId":"media-folder"
        })),
    )
    .await;
    assert_eq!(binding.status(), StatusCode::CREATED);
    let binding: Value =
        serde_json::from_slice(&binding.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(binding["AccountId"].is_string());
    assert_eq!(binding["RestartRequired"], false);
    assert!(!binding.to_string().contains("refresh-from-oauth"));
    wait_for_completed_work_job(
        database_fixture.connection(),
        binding["InitialSyncJobId"].as_str().unwrap(),
    )
    .await;

    let forms = fake.forms.lock().await;
    let exchange = forms
        .iter()
        .find(|form| {
            form.get("grant_type")
                .is_some_and(|value| value == "authorization_code")
        })
        .unwrap();
    assert_eq!(exchange["code"], "authorization-code");
    assert_eq!(exchange["client_id"], "server-client-id");
    assert_eq!(exchange["client_secret"], "server-client-secret");
    assert_eq!(
        exchange["redirect_uri"],
        "http://127.0.0.1:8096/Admin/Storage/OAuth/GoogleDrive/Callback"
    );
    assert!(exchange["code_verifier"].len() >= 43);
    drop(forms);

    let database = database_fixture.connection();
    let encrypted = database
        .query_one(
            database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("encrypted_payload"))
                    .from(Alias::new("storage_credentials")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<Vec<u8>>("", "encrypted_payload")
        .unwrap();
    assert!(
        !encrypted
            .windows(b"refresh-from-oauth".len())
            .any(|value| value == b"refresh-from-oauth")
    );
    assert_eq!(
        post_to(
            &app,
            &token,
            "/Admin/Storage/Accounts/GoogleDrive",
            json!({"RefreshToken":"must-not-be-accepted"}),
        )
        .await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn onedrive_oauth_derives_personal_identity_and_never_accepts_browser_credentials() {
    let fake = FakeMicrosoft::start().await;
    let data = TempDir::new().unwrap();
    let database_fixture = reconnectable_test_database().await.unwrap();
    let cipher = Arc::new(
        CredentialCipher::new(CredentialKey::new(1, [7_u8; 32]).unwrap(), Vec::new()).unwrap(),
    );
    let oauth = MicrosoftOneDriveOAuthConfiguration::new(
        "microsoft-client-id",
        Some("microsoft-client-secret".to_owned()),
        "http://127.0.0.1:8096/Admin/Storage/OAuth/OneDrive/Callback",
    )
    .unwrap()
    .with_endpoints(
        format!("{}/authorize", fake.base_url),
        format!("{}/token", fake.base_url),
        format!("{}/graph/v1.0/", fake.base_url),
    )
    .unwrap();
    let state = initialize(
        StartupOptions::new(
            database_fixture.database_url(),
            ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
        )
        .with_assets_dir(data.path())
        .with_bootstrap_admin(BootstrapAdmin::new("Admin", "first password"))
        .with_credential_cipher(cipher)
        .with_onedrive_oauth(oauth),
    )
    .await
    .unwrap();
    let app = build_router(state);
    let token = login(&app).await;
    let library_id = seed_library(database_fixture.connection()).await;

    let start = json_request(
        &app,
        "POST",
        "/Admin/Storage/OAuth/OneDrive/Start",
        Some(&token),
        Some(json!({"TargetLibraryId":library_id})),
    )
    .await;
    assert_eq!(start.status(), StatusCode::OK);
    let start: Value =
        serde_json::from_slice(&start.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let oauth_state = start["State"].as_str().unwrap();
    let authorization_url =
        reqwest::Url::parse(start["AuthorizationUrl"].as_str().unwrap()).unwrap();
    let authorization: HashMap<_, _> = authorization_url.query_pairs().into_owned().collect();
    assert_eq!(authorization["state"], oauth_state);
    assert_eq!(authorization["code_challenge_method"], "S256");
    assert_eq!(
        authorization["scope"],
        "offline_access User.Read Files.Read"
    );
    assert!(!authorization.contains_key("client_secret"));

    let callback = json_request(
        &app,
        "GET",
        &format!("/Admin/Storage/OAuth/OneDrive/Callback?state={oauth_state}&code=microsoft-code"),
        None,
        None,
    )
    .await;
    assert_eq!(callback.status(), StatusCode::NO_CONTENT);

    let directories = json_request(
        &app,
        "GET",
        &format!("/Admin/Storage/OAuth/OneDrive/{oauth_state}/Directories"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(directories.status(), StatusCode::OK);
    let directories: Value =
        serde_json::from_slice(&directories.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert_eq!(
        directories["Items"],
        json!([{"Id":"media-folder","Name":"Media"}])
    );
    let first_cursor = Uuid::parse_str(directories["NextPageToken"].as_str().unwrap()).unwrap();
    assert!(!directories.to_string().contains("@odata.nextLink"));
    assert!(!directories.to_string().contains("$skiptoken"));
    assert!(!directories.to_string().contains(&fake.base_url));

    let second = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/OneDrive/{oauth_state}/Directories?PageToken={first_cursor}"
        ),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(second.status(), StatusCode::OK);
    let second: Value =
        serde_json::from_slice(&second.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        second["Items"],
        json!([
            {"Id":"media-folder","Name":"Media duplicate"},
            {"Id":"archive-folder","Name":"Archive"}
        ])
    );
    let second_cursor = Uuid::parse_str(second["NextPageToken"].as_str().unwrap()).unwrap();
    assert!(!second.to_string().contains("$skiptoken"));
    assert!(!second.to_string().contains(&fake.base_url));

    let replay = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/OneDrive/{oauth_state}/Directories?PageToken={first_cursor}"
        ),
        Some(&token),
        None,
    )
    .await;
    let replay: Value =
        serde_json::from_slice(&replay.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(replay["NextPageToken"], second_cursor.to_string());

    let final_page = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/OneDrive/{oauth_state}/Directories?PageToken={second_cursor}"
        ),
        Some(&token),
        None,
    )
    .await;
    let final_page: Value =
        serde_json::from_slice(&final_page.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert_eq!(final_page, json!({"Items":[],"NextPageToken":null}));

    for invalid_uri in [
        format!(
            "/Admin/Storage/OAuth/OneDrive/{oauth_state}/Directories?ParentId=other&PageToken={first_cursor}"
        ),
        format!("/Admin/Storage/OAuth/OneDrive/{oauth_state}/Directories?PageToken=not-a-uuid"),
    ] {
        let calls_before_invalid = fake.child_queries.lock().await.len();
        let invalid = json_request(&app, "GET", &invalid_uri, Some(&token), None).await;
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert_eq!(fake.child_queries.lock().await.len(), calls_before_invalid);
    }

    let other_token = login(&app).await;
    let calls_before_wrong_owner = fake.child_queries.lock().await.len();
    let wrong_owner = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/OneDrive/{oauth_state}/Directories?PageToken={first_cursor}"
        ),
        Some(&other_token),
        None,
    )
    .await;
    assert_eq!(wrong_owner.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        fake.child_queries.lock().await.len(),
        calls_before_wrong_owner
    );

    let other_start = json_request(
        &app,
        "POST",
        "/Admin/Storage/OAuth/OneDrive/Start",
        Some(&token),
        Some(json!({"TargetLibraryId":library_id})),
    )
    .await;
    let other_start: Value =
        serde_json::from_slice(&other_start.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    let other_oauth_state = other_start["State"].as_str().unwrap();
    let calls_before_pending = fake.child_queries.lock().await.len();
    let pending = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/OneDrive/{other_oauth_state}/Directories?PageToken={first_cursor}"
        ),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(pending.status(), StatusCode::CONFLICT);
    assert_eq!(fake.child_queries.lock().await.len(), calls_before_pending);
    let other_callback = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/OneDrive/Callback?state={other_oauth_state}&code=other-microsoft-code"
        ),
        None,
        None,
    )
    .await;
    assert_eq!(other_callback.status(), StatusCode::NO_CONTENT);
    let calls_before_other_state = fake.child_queries.lock().await.len();
    let wrong_state = json_request(
        &app,
        "GET",
        &format!(
            "/Admin/Storage/OAuth/OneDrive/{other_oauth_state}/Directories?PageToken={first_cursor}"
        ),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(wrong_state.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        fake.child_queries.lock().await.len(),
        calls_before_other_state
    );

    let forged = json_request(
        &app,
        "POST",
        &format!("/Admin/Storage/OAuth/OneDrive/{oauth_state}/Bind"),
        Some(&token),
        Some(json!({
            "DisplayName":"Microsoft Media",
            "RootObjectId":"media-folder",
            "AccountIdentity":"forged@example.invalid"
        })),
    )
    .await;
    assert_eq!(forged.status(), StatusCode::BAD_REQUEST);

    let binding = json_request(
        &app,
        "POST",
        &format!("/Admin/Storage/OAuth/OneDrive/{oauth_state}/Bind"),
        Some(&token),
        Some(json!({"DisplayName":"Microsoft Media","RootObjectId":"media-folder"})),
    )
    .await;
    assert_eq!(binding.status(), StatusCode::CREATED);
    let binding: Value =
        serde_json::from_slice(&binding.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(binding["RestartRequired"], false);
    assert!(!binding.to_string().contains("microsoft-refresh-token"));
    wait_for_completed_work_job(
        database_fixture.connection(),
        binding["InitialSyncJobId"].as_str().unwrap(),
    )
    .await;

    let forms = fake.forms.lock().await;
    let exchange = forms
        .iter()
        .find(|form| {
            form.get("grant_type")
                .is_some_and(|value| value == "authorization_code")
        })
        .unwrap();
    assert_eq!(exchange["client_secret"], "microsoft-client-secret");
    assert_eq!(exchange["code_verifier"].len(), 64);
    drop(forms);

    let database = database_fixture.connection();
    let encrypted = database
        .query_one(
            database.get_database_backend().build(
                Query::select()
                    .column(Alias::new("encrypted_payload"))
                    .from(Alias::new("storage_credentials")),
            ),
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<Vec<u8>>("", "encrypted_payload")
        .unwrap();
    assert!(
        !encrypted
            .windows(b"microsoft-refresh-token".len())
            .any(|value| value == b"microsoft-refresh-token")
    );
}

struct FakeGoogle {
    base_url: String,
    forms: FakeGoogleForms,
    child_queries: FakeChildQueries,
    server: tokio::task::JoinHandle<()>,
}

type FakeGoogleForms = Arc<tokio::sync::Mutex<Vec<HashMap<String, String>>>>;
type FakeChildQueries = Arc<tokio::sync::Mutex<Vec<HashMap<String, String>>>>;

#[derive(Clone)]
struct FakeGoogleState {
    forms: FakeGoogleForms,
    child_queries: FakeChildQueries,
}

impl FakeGoogle {
    async fn start() -> Self {
        let forms = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let child_queries = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let state = FakeGoogleState {
            forms: Arc::clone(&forms),
            child_queries: Arc::clone(&child_queries),
        };
        let router = Router::new()
            .route("/token", route_post(fake_google_token))
            .route("/drive/v3/about", get(fake_google_about))
            .route("/drive/v3/drives", get(fake_google_shared_drives))
            .route("/drive/v3/files/{id}", get(fake_google_file))
            .route("/drive/v3/files", get(fake_google_children))
            .route(
                "/drive/v3/changes/startPageToken",
                get(fake_google_start_page_token),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            base_url: format!("http://{address}"),
            forms,
            child_queries,
            server,
        }
    }
}

impl Drop for FakeGoogle {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn fake_google_token(
    State(state): State<FakeGoogleState>,
    Form(form): Form<HashMap<String, String>>,
) -> Json<Value> {
    state.forms.lock().await.push(form.clone());
    if form
        .get("grant_type")
        .is_some_and(|value| value == "authorization_code")
    {
        Json(json!({"access_token":"initial-access","refresh_token":"refresh-from-oauth"}))
    } else {
        Json(json!({"access_token":"refresh-access","expires_in":3600}))
    }
}

async fn fake_google_file(Path(id): Path<String>) -> Json<Value> {
    Json(json!({
        "id":id,
        "name":"Media",
        "mimeType":"application/vnd.google-apps.folder",
        "trashed":false
    }))
}

async fn fake_google_about() -> Json<Value> {
    Json(json!({"user":{"emailAddress":"admin@example.invalid","displayName":"Admin"}}))
}

async fn fake_google_shared_drives(
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
) -> Json<Value> {
    if query
        .get("pageToken")
        .is_some_and(|value| value == "shared-page-2")
    {
        Json(json!({"drives":[{"id":"archive-drive","name":"Archive"}]}))
    } else {
        Json(json!({
            "drives":[{"id":"team-drive","name":"Team Media"}],
            "nextPageToken":"shared-page-2"
        }))
    }
}

async fn fake_google_children(
    State(state): State<FakeGoogleState>,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
) -> Json<Value> {
    state.child_queries.lock().await.push(query.clone());
    match query.get("pageToken").map(String::as_str) {
        Some("google-provider-page-2") => Json(json!({
            "files":[
                {"id":"media-folder","name":"Media duplicate","mimeType":"application/vnd.google-apps.folder","trashed":false},
                {"id":"archive-folder","name":"Archive","mimeType":"application/vnd.google-apps.folder","trashed":false}
            ],
            "nextPageToken":"google-provider-page-3"
        })),
        Some("google-provider-page-3") => Json(json!({"files":[]})),
        _ => Json(json!({
            "files":[
                {"id":"media-folder","name":"Media","mimeType":"application/vnd.google-apps.folder","trashed":false},
                {"id":"movie-file","name":"Movie.mkv","mimeType":"video/x-matroska","size":"1","trashed":false}
            ],
            "nextPageToken":"google-provider-page-2"
        })),
    }
}

async fn fake_google_start_page_token() -> Json<Value> {
    Json(json!({"startPageToken":"cursor-1"}))
}

struct FakeMicrosoft {
    base_url: String,
    forms: FakeGoogleForms,
    child_queries: FakeChildQueries,
    server: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
struct FakeMicrosoftState {
    forms: FakeGoogleForms,
    child_queries: FakeChildQueries,
    graph_api_base: String,
}

impl FakeMicrosoft {
    async fn start() -> Self {
        let forms = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let child_queries = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let base_url = format!("http://{address}");
        let state = FakeMicrosoftState {
            forms: Arc::clone(&forms),
            child_queries: Arc::clone(&child_queries),
            graph_api_base: format!("{base_url}/graph/v1.0/"),
        };
        let router = Router::new()
            .route("/token", route_post(fake_microsoft_token))
            .route("/graph/v1.0/me", get(fake_microsoft_me))
            .route("/graph/v1.0/me/drive", get(fake_microsoft_drive))
            .route("/graph/v1.0/me/drive/root", get(fake_microsoft_root))
            .route(
                "/graph/v1.0/drives/{drive_id}/items/{item_id}",
                get(fake_microsoft_item),
            )
            .route(
                "/graph/v1.0/drives/{drive_id}/items/{item_id}/children",
                get(fake_microsoft_children),
            )
            .route(
                "/graph/v1.0/drives/{drive_id}/root/delta",
                get(fake_microsoft_delta),
            )
            .with_state(state);
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            base_url,
            forms,
            child_queries,
            server,
        }
    }
}

impl Drop for FakeMicrosoft {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn fake_microsoft_token(
    State(state): State<FakeMicrosoftState>,
    Form(form): Form<HashMap<String, String>>,
) -> Json<Value> {
    state.forms.lock().await.push(form.clone());
    if form
        .get("grant_type")
        .is_some_and(|value| value == "authorization_code")
    {
        Json(json!({"access_token":"initial-access","refresh_token":"microsoft-refresh-token"}))
    } else {
        Json(json!({"access_token":"refresh-access","expires_in":3600}))
    }
}

async fn fake_microsoft_me() -> Json<Value> {
    Json(json!({"id":"microsoft-user","displayName":"Microsoft Admin"}))
}

async fn fake_microsoft_drive() -> Json<Value> {
    Json(json!({"id":"personal-drive","driveType":"personal"}))
}

async fn fake_microsoft_root() -> Json<Value> {
    Json(json!({"id":"root-item"}))
}

async fn fake_microsoft_item(Path((_drive_id, item_id)): Path<(String, String)>) -> Json<Value> {
    Json(json!({
        "id":item_id,
        "name":"Media",
        "folder":{},
        "parentReference":{"driveId":"personal-drive"}
    }))
}

async fn fake_microsoft_children(
    State(state): State<FakeMicrosoftState>,
    Path((_drive_id, _item_id)): Path<(String, String)>,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
) -> Json<Value> {
    state.child_queries.lock().await.push(query.clone());
    match query.get("$skiptoken").map(String::as_str) {
        Some("page-2") => Json(json!({
            "value":[
                {"id":"media-folder","name":"Media duplicate","folder":{},"parentReference":{"driveId":"personal-drive"}},
                {"id":"archive-folder","name":"Archive","folder":{},"parentReference":{"driveId":"personal-drive"}}
            ],
            "@odata.nextLink":format!("{}drives/personal-drive/items/root-item/children?$skiptoken=page-3", state.graph_api_base)
        })),
        Some("page-3") => Json(json!({"value":[]})),
        _ => Json(json!({
            "value":[
                {"id":"media-folder","name":"Media","folder":{},"parentReference":{"driveId":"personal-drive"}},
                {"id":"movie-file","name":"Movie.mkv","size":1,"file":{},"parentReference":{"driveId":"personal-drive"}}
            ],
            "@odata.nextLink":format!("{}drives/personal-drive/items/root-item/children?$skiptoken=page-2", state.graph_api_base)
        })),
    }
}

async fn fake_microsoft_delta(
    State(state): State<FakeMicrosoftState>,
    Path(_drive_id): Path<String>,
) -> Json<Value> {
    Json(json!({
        "value":[],
        "@odata.deltaLink":format!("{}drives/personal-drive/root/delta?token=cursor-1", state.graph_api_base)
    }))
}

async fn json_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> axum::response::Response {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        request = request.header(
            header::AUTHORIZATION,
            format!(r#"MediaBrowser Token="{token}""#),
        );
    }
    if body.is_some() {
        request = request.header(header::CONTENT_TYPE, "application/json");
    }
    app.clone()
        .oneshot(
            request
                .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn login(app: &axum::Router) -> String {
    login_with_user(app).await.0
}

async fn login_with_user(app: &axum::Router) -> (String, Uuid) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/Users/AuthenticateByName")
                .header(
                    header::AUTHORIZATION,
                    r#"MediaBrowser Client="AdminTest", Device="Test", DeviceId="1", Version="1""#,
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"Username":"admin","Pw":"first password"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    (
        body["AccessToken"].as_str().unwrap().to_owned(),
        Uuid::parse_str(body["User"]["Id"].as_str().unwrap()).unwrap(),
    )
}

async fn post(app: &axum::Router, token: &str, body: Value) -> StatusCode {
    post_to(app, token, "/Admin/Storage/Accounts/OneDrive", body).await
}

async fn post_to(app: &axum::Router, token: &str, uri: &str, body: Value) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

async fn command(app: &axum::Router, token: &str, uri: &str) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(
                    header::AUTHORIZATION,
                    format!(r#"MediaBrowser Token="{token}""#),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

async fn wait_for_completed_work_job(database: &sea_orm::DatabaseConnection, job_id: &str) {
    let job_id = Uuid::parse_str(job_id).unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let row = database
            .query_one(
                database.get_database_backend().build(
                    Query::select()
                        .column(Alias::new("state"))
                        .from(Alias::new("work_jobs"))
                        .and_where(Expr::col(Alias::new("id")).eq(job_id)),
                ),
            )
            .await
            .unwrap()
            .unwrap();
        let state = row.try_get::<String>("", "state").unwrap();
        match state.as_str() {
            "Completed" => return,
            "Failed" | "Cancelled" => panic!("initial sync job terminated as {state}"),
            _ if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            _ => panic!("initial sync job did not complete in the active server process"),
        }
    }
}

async fn seed_library(database: &sea_orm::DatabaseConnection) -> Uuid {
    let library = Uuid::new_v4();
    let backend = database.get_database_backend();
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
                        "Imported".into(),
                        "Manual".into(),
                        "title_layer".into(),
                        "basic".into(),
                        "manual".into(),
                        "on_playback".into(),
                        1.into(),
                        "mixed".into(),
                        b"imported".to_vec().into(),
                        true.into(),
                    ]),
            ),
        )
        .await
        .unwrap();
    library
}
