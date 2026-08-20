use chrono::{TimeZone, Utc};
use serde_json::json;
use tjxy_api::{
    AuthenticateUserByName, AuthenticationResult, CreateUserByName, SessionInfoDto, UpdateUserName,
    UpdateUserPassword, UpdateUserPolicy, UserDto, UserPolicy,
};
use uuid::Uuid;

#[test]
fn login_request_uses_the_pinned_pascal_case_names() {
    let request: AuthenticateUserByName =
        serde_json::from_value(json!({"Username": "Alice", "Pw": ""})).unwrap();

    assert_eq!(request.username, "Alice");
    assert_eq!(request.password, "");
    for value in [
        json!({"Username": "Alice"}),
        json!({"Username": "Alice", "Pw": null}),
    ] {
        let request: AuthenticateUserByName = serde_json::from_value(value).unwrap();
        assert_eq!(request.password, "");
    }
    assert!(
        serde_json::from_value::<AuthenticateUserByName>(json!({"username": "Alice"})).is_err()
    );
}

#[test]
fn login_request_ignores_vidhub_password_compatibility_field() {
    let request: AuthenticateUserByName = serde_json::from_value(json!({
        "Username": "Alice",
        "Pw": "canonical password",
        "Password": "legacy compatibility field"
    }))
    .unwrap();

    assert_eq!(request.username, "Alice");
    assert_eq!(request.password, "canonical password");
}

#[test]
fn authentication_result_matches_the_l1_contract() {
    let server_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let user_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
    let session_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
    let user = UserDto::new(
        user_id,
        "Alice",
        server_id,
        true,
        UserPolicy::direct_play_only(true),
    );
    let session = SessionInfoDto::active(
        session_id, user_id, "Alice", "Findroid", "phone-1", "Pixel", "0.16.0", server_id,
    );
    let response = AuthenticationResult::new(user, session, "secret", server_id);

    assert_eq!(
        serde_json::to_value(response).unwrap(),
        json!({
            "User": {
                "Name": "Alice",
                "ServerId": server_id,
                "Id": user_id,
                "HasPassword": true,
                "HasConfiguredPassword": true,
                "Configuration": {},
                "Policy": {
                    "IsAdministrator": true,
                    "IsDisabled": false,
                    "EnableMediaPlayback": true,
                    "EnableAudioPlaybackTranscoding": false,
                    "EnableVideoPlaybackTranscoding": false,
                    "EnablePlaybackRemuxing": false,
                    "AuthenticationProviderId": "TJXY.LocalAuthentication",
                    "PasswordResetProviderId": "TJXY.LocalPasswordReset"
                }
            },
            "SessionInfo": {
                "Id": session_id,
                "UserId": user_id,
                "UserName": "Alice",
                "Client": "Findroid",
                "DeviceId": "phone-1",
                "DeviceName": "Pixel",
                "ApplicationVersion": "0.16.0",
                "ServerId": server_id,
                "IsActive": true,
                "PlayableMediaTypes": [],
                "SupportedCommands": []
            },
            "AccessToken": "secret",
            "ServerId": server_id
        })
    );
}

#[test]
fn administrator_user_requests_use_the_pinned_pascal_case_contract() {
    let create: CreateUserByName =
        serde_json::from_value(json!({"Name": "Bob", "Password": null})).unwrap();
    assert_eq!(create.name, "Bob");
    assert_eq!(create.password, "");

    let rename: UpdateUserName = serde_json::from_value(json!({
        "Name": "Robert",
        "ServerId": "ignored-client-field"
    }))
    .unwrap();
    assert_eq!(rename.name, "Robert");

    let password: UpdateUserPassword = serde_json::from_value(json!({
        "CurrentPw": null,
        "NewPw": "new password",
        "ResetPassword": false
    }))
    .unwrap();
    assert_eq!(password.new_password, "new password");
    assert!(!password.reset_password);

    let policy: UpdateUserPolicy = serde_json::from_value(json!({
        "IsAdministrator": true,
        "IsDisabled": false,
        "EnableVideoPlaybackTranscoding": true,
        "AuthenticationProviderId": "TJXY.LocalAuthentication",
        "PasswordResetProviderId": "TJXY.LocalPasswordReset"
    }))
    .unwrap();
    assert!(policy.is_administrator);
    assert!(!policy.is_disabled);
}

#[test]
fn listed_session_exposes_activity_and_persisted_capabilities() {
    let session_id = Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f21").unwrap();
    let user_id = Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f22").unwrap();
    let server_id = Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f23").unwrap();
    let activity = Utc.with_ymd_and_hms(2026, 7, 26, 11, 0, 0).unwrap();

    let session = SessionInfoDto::listed(
        session_id,
        user_id,
        "Alice",
        "Findroid",
        "phone-1",
        "Pixel",
        "0.16.0",
        server_id,
        activity,
        vec!["Video".to_owned(), "Audio".to_owned()],
        vec!["Play".to_owned()],
        true,
        true,
    );

    assert_eq!(
        serde_json::to_value(session).unwrap(),
        json!({
            "Id": session_id,
            "UserId": user_id,
            "UserName": "Alice",
            "Client": "Findroid",
            "DeviceId": "phone-1",
            "DeviceName": "Pixel",
            "ApplicationVersion": "0.16.0",
            "ServerId": server_id,
            "IsActive": true,
            "PlayableMediaTypes": ["Video", "Audio"],
            "SupportedCommands": ["Play"],
            "LastActivityDate": "2026-07-26T11:00:00Z",
            "SupportsMediaControl": true,
            "SupportsRemoteControl": true,
            "Capabilities": {
                "PlayableMediaTypes": ["Video", "Audio"],
                "SupportedCommands": ["Play"],
                "SupportsMediaControl": true,
                "SupportsPersistentIdentifier": true
            }
        })
    );
}
