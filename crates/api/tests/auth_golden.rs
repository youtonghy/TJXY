use serde_json::json;
use tjxy_api::{AuthenticateUserByName, AuthenticationResult, SessionInfoDto, UserDto, UserPolicy};
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
