use tjxy_credentials::{CredentialCipher, CredentialEnvelope, CredentialKey};
use uuid::Uuid;

#[test]
fn envelope_round_trips_and_is_bound_to_credential_and_provider() {
    let cipher = CredentialCipher::new(
        CredentialKey::new(2, [7_u8; 32]).unwrap(),
        vec![CredentialKey::new(1, [3_u8; 32]).unwrap()],
    )
    .unwrap();
    let credential_id = Uuid::new_v4();
    let sealed = cipher
        .seal(credential_id, "google-drive", b"refresh-token")
        .unwrap();

    assert_eq!(sealed.key_version(), 2);
    assert_eq!(
        cipher
            .open(credential_id, "google-drive", &sealed)
            .unwrap()
            .as_slice(),
        b"refresh-token"
    );
    assert!(
        cipher
            .open(Uuid::new_v4(), "google-drive", &sealed)
            .is_err()
    );
    assert!(cipher.open(credential_id, "onedrive", &sealed).is_err());
}

#[test]
fn tampering_is_rejected_and_each_envelope_uses_a_fresh_nonce() {
    let cipher =
        CredentialCipher::new(CredentialKey::new(1, [9_u8; 32]).unwrap(), Vec::new()).unwrap();
    let credential_id = Uuid::new_v4();
    let first = cipher
        .seal(credential_id, "google-drive", b"same-secret")
        .unwrap();
    let second = cipher
        .seal(credential_id, "google-drive", b"same-secret")
        .unwrap();

    assert_ne!(first.payload(), second.payload());
    let mut tampered = first.payload().to_vec();
    *tampered.last_mut().unwrap() ^= 1;
    let tampered = CredentialEnvelope::from_parts(first.key_version(), tampered).unwrap();
    assert!(
        cipher
            .open(credential_id, "google-drive", &tampered)
            .is_err()
    );
}

#[test]
fn historical_key_versions_decrypt_but_new_envelopes_use_the_active_key() {
    let old =
        CredentialCipher::new(CredentialKey::new(1, [1_u8; 32]).unwrap(), Vec::new()).unwrap();
    let credential_id = Uuid::new_v4();
    let old_envelope = old
        .seal(credential_id, "google-drive", b"old-secret")
        .unwrap();
    let rotated = CredentialCipher::new(
        CredentialKey::new(2, [2_u8; 32]).unwrap(),
        vec![CredentialKey::new(1, [1_u8; 32]).unwrap()],
    )
    .unwrap();

    assert_eq!(
        rotated
            .open(credential_id, "google-drive", &old_envelope)
            .unwrap()
            .as_slice(),
        b"old-secret"
    );
    assert_eq!(
        rotated
            .seal(credential_id, "google-drive", b"new-secret")
            .unwrap()
            .key_version(),
        2
    );
    assert!(!format!("{:?}", CredentialKey::new(9, [42_u8; 32]).unwrap()).contains("42"));
}
