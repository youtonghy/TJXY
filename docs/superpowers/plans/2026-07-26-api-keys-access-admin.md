# API Keys And Access Admin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver encrypted Jellyfin-compatible API key lifecycle management and a production-tested React Admin Access page for Devices and API Keys.

**Architecture:** A dedicated `api_keys` table stores only SHA-256 token digests and versioned AES-GCM envelopes. `AuthenticatedPrincipal` distinguishes login sessions from API keys, while `AuthService` owns key lifecycle and digest authentication. Focused canonical Axum handlers and focused React Admin adapters expose the approved workflow without a private Admin API or a generic credential framework.

**Tech Stack:** Rust 1.88.0, SeaORM/SeaQuery 1.1.19, Axum 0.8.9, AES-256-GCM through `tjxy-credentials`, SQLite/PostgreSQL 17/MySQL 8.4, React 19.2.7, React Admin 5.15.1, MUI 9.2.0, TypeScript 6.0.3, Vitest 4.1.10, Playwright 1.61.1.

## Global Constraints

- Follow `docs/superpowers/specs/2026-07-26-api-keys-access-admin-design.md` exactly.
- `GET /Auth/Keys` returns recoverable complete keys; SQL never stores plaintext keys.
- Use `CredentialCipher` with versioned API-key-specific AAD; do not reuse `storage_credentials`.
- API keys are independent principals, never synthetic `auth_sessions`.
- Every auth-revision-advancing user mutation physically deletes that user's API keys in the same transaction.
- API key authentication uses only the indexed SHA-256 digest and never decrypts a token.
- Capabilities, logout, playstate, ping, and OAuth-state workflows require a real session and return 403 for API-key principals.
- Keep the maximum durable API key count at 256; do not add scope, expiry, rotation, soft deletion, or bulk key operations.
- Missing or unreadable keyring material fails closed exactly as described in the design; never downgrade to plaintext or partial results.
- Raw keys never enter logs, Redis, browser storage, error text, `Debug`, or metrics.
- Keep React Admin same-origin; use canonical `/Auth/Keys` and `/Devices`, not a private Admin API.
- Preserve all unrelated dirty-worktree changes and commit only files belonging to the current task.
- When a modified file already contains unrelated unstaged hunks, stage only the task patch and verify `git diff --cached`; never commit the whole file by convenience.
- Before each library/API-specific implementation edit, use Context7 in order: Jellyfin DTO/routes in Tasks 1 and 7, SeaORM/SeaQuery migration/query APIs in Tasks 2 and 4, Axum path/query/response APIs in Task 7, and React Admin/MUI in Tasks 9 and 10.
- Use TDD for every behavior: run the focused test red, implement the minimum behavior, then run it green.

---

## File Map

### API and database contract

- Create `crates/api/src/api_key.rs`: Jellyfin `AuthenticationInfo` DTOs without secret-leaking `Debug`.
- Create `crates/api/tests/api_key_golden.rs`: exact PascalCase and null serialization.
- Create `crates/db/src/migration/m20260726_000037_api_keys.rs`: cross-database table, indexes, and restrictive creator FK.
- Modify `crates/db/src/migration/mod.rs`: register migration 37 after migration 36.
- Modify `crates/db/tests/schema_contract.rs`: table, columns, indexes, FK, rollback, and binary digest checks.
- Create `crates/db/src/api_key.rs`: deep repository for key metadata, actor fencing, digest lookup, activity touch, and startup reads.
- Create `crates/db/tests/api_key_repository_contract.rs`: repository and user-revision lifecycle contracts.
- Modify `crates/db/src/auth.rs`: explicit authentication origin and atomic key deletion in user mutation transactions.
- Modify `crates/db/src/lib.rs`: export principal-origin and API-key repository types.
- Modify `crates/db/tests/auth_repository_contract.rs`: preserve session-origin behavior.

### Application, startup, and HTTP

- Create `crates/application/src/api_key.rs`: secret wrapper, key lifecycle, cipher use, and startup validation.
- Modify `crates/application/src/auth.rs`: optional cipher, API-key digest fallback, and session-required behavior.
- Modify `crates/application/src/lib.rs`: export key information and secret types.
- Modify `crates/application/Cargo.toml` and `Cargo.lock`: direct `tjxy-credentials` and `zeroize` dependencies.
- Modify `crates/application/tests/auth_service_contract.rs`: lifecycle, authorization, invalidation, tamper, and session-only tests.
- Modify `crates/server/src/startup.rs`: inject the keyring and validate persisted envelopes before readiness.
- Modify `crates/server/tests/startup.rs`: absent/current/historical/missing/corrupt keyring restart matrix.
- Create `crates/server/src/api_key.rs`: canonical GET/POST/DELETE handlers and safe error mapping.
- Modify `crates/server/src/lib.rs`: register the module and routes.
- Modify `crates/server/src/auth.rs`: reusable session-origin guard.
- Modify `crates/server/src/browse.rs`, `crates/server/src/session.rs`, `crates/server/src/playstate.rs`, and `crates/server/src/storage_admin.rs`: explicit 403 handling for session-only operations.
- Create `crates/server/tests/api_key_routes.rs`: focused in-process HTTP contract.
- Modify `crates/server/tests/jellyfin_tcp_smoke.rs`: durable real-TCP create/restart/use/delete lifecycle.

### React Admin and release evidence

- Create `admin/src/access/deviceApi.ts` and `deviceApi.test.ts`: canonical Devices adapter.
- Create `admin/src/access/apiKeyApi.ts` and `apiKeyApi.test.ts`: canonical API Keys adapter.
- Create `admin/src/access/AccessPage.tsx`: tab composition.
- Create `admin/src/access/DevicesPanel.tsx` and `DevicesPanel.test.tsx`: rename and revoke workflow.
- Create `admin/src/access/ApiKeysPanel.tsx` and `ApiKeysPanel.test.tsx`: create, reveal, copy, and delete workflow.
- Modify `admin/src/App.tsx`, `admin/src/layout/AdminLayout.tsx`, and `admin/vite.config.ts`: authenticated route, menu, and exact proxies.
- Create `admin/e2e/support.ts`: shared diagnostics/layout helpers with API-key path redaction.
- Create `admin/e2e/access.spec.ts`: production Access lifecycle and responsive checks.
- Modify `admin/e2e/users.spec.ts`: import shared helpers without changing existing behavior.
- Modify `admin/scripts/run-e2e-server.sh`: deterministic test keyring.
- Modify `README.md` and `docs/api-parity.md`: deployment requirements and verified status.

---

### Task 1: Pin The Jellyfin API Key DTO

**Files:**
- Create: `crates/api/src/api_key.rs`
- Create: `crates/api/tests/api_key_golden.rs`
- Modify: `crates/api/src/lib.rs`

**Interfaces:**
- Produces: `AuthenticationInfoDto::new(...)` and `AuthenticationInfoQueryResult::new(Vec<AuthenticationInfoDto>)`.
- Consumes: `chrono::DateTime<Utc>`, `uuid::Uuid`, and the pinned Jellyfin OpenAPI field names.

- [ ] **Step 1: Refresh the pinned Jellyfin contract through Context7**

Resolve `Jellyfin`, select the high-reputation stable OpenAPI library, and query the full
`AuthenticationInfo` and `AuthenticationInfoQueryResult` field/null contract for
`GET /Auth/Keys`. Record the selected library ID in the task notes. If the current
contract differs from the approved design, stop for design review instead of silently
changing compatibility behavior.

- [ ] **Step 2: Write the failing golden test**

Create a fixed record and assert the complete wrapper, including explicit nulls:

```rust
let dto = AuthenticationInfoDto::new(
    7,
    "0123456789abcdef",
    "Kodi Sync",
    user_id,
    "Admin",
    created_at,
    Some(last_activity),
);
assert_eq!(serde_json::to_value(AuthenticationInfoQueryResult::new(vec![dto])).unwrap(), json!({
    "Items": [{
        "Id": 7,
        "AccessToken": "0123456789abcdef",
        "DeviceId": null,
        "AppName": "Kodi Sync",
        "AppVersion": null,
        "DeviceName": null,
        "UserId": user_id,
        "IsActive": true,
        "DateCreated": "2026-07-26T12:00:00Z",
        "DateRevoked": null,
        "DateLastActivity": "2026-07-26T12:03:00Z",
        "UserName": "Admin"
    }],
    "TotalRecordCount": 1,
    "StartIndex": 0
}));
```

Also assert that an absent last activity serializes as `null`. Do not derive `Debug` for a DTO containing `AccessToken`.

- [ ] **Step 3: Run the golden test to verify red**

Run: `cargo test -p tjxy-api --test api_key_golden --locked`

Expected: FAIL because the module and DTOs do not exist.

- [ ] **Step 4: Implement the exact DTO**

Use private fields and constructor-only creation:

```rust
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticationInfoDto {
    id: i64,
    access_token: String,
    device_id: Option<String>,
    app_name: String,
    app_version: Option<String>,
    device_name: Option<String>,
    user_id: Uuid,
    is_active: bool,
    date_created: DateTime<Utc>,
    date_revoked: Option<DateTime<Utc>>,
    date_last_activity: Option<DateTime<Utc>>,
    user_name: String,
}
```

`AuthenticationInfoQueryResult::new` sets `TotalRecordCount` before moving the vector and always sets `StartIndex` to zero.

- [ ] **Step 5: Verify DTO tests and API lint**

Run:

```bash
cargo test -p tjxy-api --test api_key_golden --locked
cargo clippy -p tjxy-api --all-targets --locked -- -D warnings
```

Expected: both commands exit 0.

- [ ] **Step 6: Commit Task 1**

```bash
git add crates/api/src/api_key.rs crates/api/src/lib.rs crates/api/tests/api_key_golden.rs
git commit -m "feat(api): pin API key compatibility DTO"
```

---

### Task 2: Add The Cross-Database API Key Schema

**Files:**
- Create: `crates/db/src/migration/m20260726_000037_api_keys.rs`
- Modify: `crates/db/src/migration/mod.rs`
- Modify: `crates/db/tests/schema_contract.rs`

**Interfaces:**
- Produces: `api_keys` with `uq_api_keys_envelope_id`, `uq_api_keys_token_digest`, and `ix_api_keys_creator`.
- Consumes: existing `users(id)`, migration helpers, and backend-specific binary-column patterns.

- [ ] **Step 1: Refresh SeaORM migration guidance through Context7**

Resolve `SeaORM` and query the installed 1.1 migration APIs for cross-database BIGINT
auto-increment keys, UUID columns, binary columns, named indexes, restrictive foreign
keys, and reversible migrations. Use the existing repository patterns when the docs offer
multiple equivalent APIs.

- [ ] **Step 2: Add failing schema assertions**

Extend the required table and index lists, then add a focused test:

```rust
#[tokio::test]
async fn api_key_schema_is_bounded_binary_and_restrictive() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let schema = SchemaManager::new(&database);
    for column in [
        "id", "envelope_id", "creator_user_id", "creator_auth_revision",
        "token_digest", "encrypted_payload", "key_version", "app_name",
        "created_at", "last_used_at",
    ] {
        assert!(schema.has_column("api_keys", column).await.unwrap());
    }
    for index in [
        "uq_api_keys_envelope_id", "uq_api_keys_token_digest", "ix_api_keys_creator",
    ] {
        assert!(schema.has_index("api_keys", index).await.unwrap());
    }
}
```

Add a test-only `column_type_name` helper because no equivalent exists in the current
suite. For SQLite read `PRAGMA table_info('api_keys')`; for PostgreSQL query
`information_schema.columns` in `current_schema()`; for MySQL query
`information_schema.columns` in `DATABASE()` and read `character_maximum_length`. Assert
`VARBINARY(32)` on MySQL, `bytea` on PostgreSQL, and a BLOB affinity on SQLite. Add
`api_keys` to rollback absence checks and assert the creator FK does not cascade.

- [ ] **Step 3: Run the schema tests to verify red**

Run:

```bash
cargo test -p tjxy-db --test schema_contract api_key_schema_is_bounded_binary_and_restrictive --locked
cargo test -p tjxy-db --test schema_contract all_migrations_can_be_rolled_back --locked
```

Expected: FAIL because migration 37 and `api_keys` do not exist.

- [ ] **Step 4: Implement migration 37**

Create `id` with `.big_integer().auto_increment().primary_key()`. Use `VARBINARY(32)` on MySQL and `blob(...)` elsewhere for `token_digest`. Add the UUID AAD identity, restrictive creator FK, envelope blob, key version, bounded app name, and timestamps. Create the three named indexes explicitly and drop the table in `down`.

Register exactly one new migration after `m20260726_000036_device_options`:

```rust
mod m20260726_000037_api_keys;
// ...
Box::new(m20260726_000037_api_keys::Migration),
```

- [ ] **Step 5: Verify SQLite migration up/down**

Run:

```bash
cargo test -p tjxy-db --test schema_contract api_key_schema_is_bounded_binary_and_restrictive --locked
cargo test -p tjxy-db --test schema_contract all_migrations_can_be_rolled_back --locked
```

Expected: both pass.

- [ ] **Step 6: Commit Task 2**

```bash
git add crates/db/src/migration/m20260726_000037_api_keys.rs crates/db/src/migration/mod.rs crates/db/tests/schema_contract.rs
git commit -m "feat(db): add encrypted API key schema"
```

---

### Task 3: Make Authentication Origin Explicit

**Files:**
- Modify: `crates/db/src/auth.rs`
- Modify: `crates/db/src/lib.rs`
- Modify: `crates/db/tests/auth_repository_contract.rs`
- Modify: `crates/application/src/auth.rs`
- Modify: `crates/application/tests/auth_service_contract.rs`
- Modify: `crates/server/src/auth.rs`
- Modify: `crates/server/src/browse.rs`
- Modify: `crates/server/src/session.rs`
- Modify: `crates/server/src/playstate.rs`
- Modify: `crates/server/src/storage_admin.rs`

**Interfaces:**
- Produces: `AuthenticationOrigin`, `AuthenticatedPrincipal::session_id() -> Option<Uuid>`, `device_id() -> Option<&str>`, `api_key_id() -> Option<i64>`, and `AuthError::SessionRequired`.
- Consumes: existing session authentication rows and all current session-only call sites.

- [ ] **Step 1: Write failing origin assertions**

Change the repository contract to require a session origin:

```rust
let principal = repository
    .find_principal_by_token_digest(&digest, now)
    .await.unwrap().unwrap();
assert_eq!(principal.session_id(), Some(issued.id()));
assert_eq!(principal.device_id(), Some("device-1"));
assert_eq!(principal.api_key_id(), None);
```

Add application assertions that a normal session still updates capabilities, resolves its DeviceProfile, and logs out.

- [ ] **Step 2: Run focused tests to verify red**

Run:

```bash
cargo test -p tjxy-db --test auth_repository_contract --locked
cargo test -p tjxy-application --test auth_service_contract session --locked
```

Expected: FAIL at compile/assertion time because principal origin accessors do not yet have the approved signatures.

- [ ] **Step 3: Implement the origin type and session guard**

Replace the two primitive principal fields with:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthenticationOrigin {
    Session { id: Uuid, device_id: String },
    ApiKey { id: i64 },
}
```

Session row decoding constructs `Session`. Add a crate-visible constructor for the later API-key repository. In `AuthService`, centralize:

```rust
fn require_session_id(principal: &AuthenticatedPrincipal) -> Result<Uuid, AuthError> {
    principal.session_id().ok_or(AuthError::SessionRequired)
}
```

Capabilities, logout, playstate, ping, and OAuth-state handlers map missing session identity to 403. `session_device_profile` returns `Ok(None)` for an API-key origin. Add one server helper for the repeated principal-to-session-ID mapping instead of duplicating status construction across the nine OAuth call sites.

- [ ] **Step 4: Verify all principal-origin ripple tests**

Run:

```bash
cargo test -p tjxy-db --test auth_repository_contract --locked
cargo test -p tjxy-application --test auth_service_contract --locked
cargo test -p tjxy-server --test auth_routes --locked
cargo test -p tjxy-server --test storage_admin_routes --locked
cargo check -p tjxy-server --tests --locked
```

Expected: all pass with session behavior unchanged.

- [ ] **Step 5: Commit Task 3**

```bash
git add crates/db/src/auth.rs crates/db/src/lib.rs crates/db/tests/auth_repository_contract.rs crates/application/src/auth.rs crates/application/tests/auth_service_contract.rs crates/server/src/auth.rs crates/server/src/browse.rs crates/server/src/session.rs crates/server/src/playstate.rs crates/server/src/storage_admin.rs
git commit -m "refactor(auth): distinguish session principals"
```

---

### Task 4: Implement The Durable API Key Repository

**Files:**
- Create: `crates/db/src/api_key.rs`
- Create: `crates/db/tests/api_key_repository_contract.rs`
- Modify: `crates/db/src/lib.rs`
- Modify: `crates/db/src/auth.rs`

**Interfaces:**
- Produces: `ApiKeyDraft`, `StoredApiKey`, `ApiKeyRepository`, `ApiKeyRepositoryError`, and API-key principal lookup.
- Consumes: `AuthUser`, `AuthenticatedPrincipal::for_api_key`, `CredentialEnvelope`, and migration 37.

Use these repository methods consistently in later tasks:

```rust
pub async fn create(&self, actor: &AuthUser, draft: ApiKeyDraft) -> Result<(), ApiKeyRepositoryError>;
pub async fn list(&self, actor: &AuthUser) -> Result<Vec<StoredApiKey>, ApiKeyRepositoryError>;
pub async fn delete_by_digest(&self, actor: &AuthUser, digest: &[u8; 32]) -> Result<(), ApiKeyRepositoryError>;
pub async fn find_principal_by_token_digest(&self, digest: &[u8; 32], now: DateTime<Utc>) -> Result<Option<AuthenticatedPrincipal>, ApiKeyRepositoryError>;
pub async fn list_for_startup(&self) -> Result<Vec<StoredApiKey>, ApiKeyRepositoryError>;
```

- [ ] **Step 1: Write failing repository lifecycle tests**

Cover create/list/delete, deterministic newest-first ordering, duplicate app names, capacity, actor revision fencing, digest lookup, and three-minute activity throttling. The core test begins:

```rust
let draft = ApiKeyDraft {
    envelope_id,
    creator_user_id: admin.id(),
    creator_auth_revision: admin.auth_revision(),
    token_digest: [7; 32],
    envelope,
    app_name: "Kodi Sync".to_owned(),
    created_at: now,
};
repository.create(&admin, draft).await.unwrap();
let listed = repository.list(&admin).await.unwrap();
assert_eq!(listed.len(), 1);
assert_eq!(listed[0].app_name(), "Kodi Sync");
assert_eq!(listed[0].envelope_id(), envelope_id);
```

Advance the user revision through each existing name/password/policy mutation and assert the user's API-key rows are physically absent after commit. Add a rollback assertion showing a failed final-admin policy mutation does not delete keys.

- [ ] **Step 2: Run repository tests to verify red**

Run: `cargo test -p tjxy-db --test api_key_repository_contract --locked`

Expected: FAIL because the repository types do not exist.

- [ ] **Step 3: Implement the deep repository module**

Each create/list/delete transaction fences the actor with one conditional no-op user update:

```text
UPDATE users SET auth_revision = auth_revision
WHERE id = ? AND auth_revision = ? AND is_admin = true AND disabled_at IS NULL
```

Create counts rows under the same transaction and returns `CapacityReached` at 256. List joins the creator user, limits 256, and orders by `created_at DESC, id DESC`. Lookup compares only the binary digest, joins the current enabled administrator, checks the captured revision, constructs `AuthenticationOrigin::ApiKey`, and touches `last_used_at` only when older than three minutes. Delete is idempotent.

Expose one crate-private `delete_for_user_on(transaction, user_id)` helper. Call it inside every existing name/password/policy update before advancing `auth_revision`, and inside user deletion before deleting the user. Do not duplicate user mutation logic in the application layer.

- [ ] **Step 4: Verify repository and user transactions**

Run:

```bash
cargo test -p tjxy-db --test api_key_repository_contract --locked
cargo test -p tjxy-db --test auth_repository_contract --locked
cargo clippy -p tjxy-db --all-targets --locked -- -D warnings
```

Expected: all pass.

- [ ] **Step 5: Commit Task 4**

```bash
git add crates/db/src/api_key.rs crates/db/src/auth.rs crates/db/src/lib.rs crates/db/tests/api_key_repository_contract.rs crates/db/tests/auth_repository_contract.rs
git commit -m "feat(db): persist encrypted API keys"
```

---

### Task 5: Add API Key Lifecycle To AuthService

**Files:**
- Create: `crates/application/src/api_key.rs`
- Modify: `crates/application/src/auth.rs`
- Modify: `crates/application/src/lib.rs`
- Modify: `crates/application/Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/application/tests/auth_service_contract.rs`

**Interfaces:**
- Produces: `SecretApiKey`, `ApiKeyInfo`, `AuthService::with_credential_cipher`, `list_api_keys`, `create_api_key`, `delete_api_key`, and `validate_api_key_envelopes`.
- Consumes: Task 4 repository methods, `CredentialCipher`, `zeroize::Zeroizing`, and the existing token generator/digest helper.

Use these exact application signatures:

```rust
pub fn with_credential_cipher(self, cipher: Arc<CredentialCipher>) -> Self;
pub async fn list_api_keys(&self, actor: &AuthenticatedPrincipal) -> Result<Vec<ApiKeyInfo>, AuthError>;
pub async fn create_api_key(&self, actor: &AuthenticatedPrincipal, app_name: &str) -> Result<(), AuthError>;
pub async fn delete_api_key(&self, actor: &AuthenticatedPrincipal, raw_token: &str) -> Result<(), AuthError>;
pub async fn validate_api_key_envelopes(&self) -> Result<(), AuthError>;
```

`ApiKeyInfo` exposes getters for `id`, `access_token`, `app_name`,
`creator_user_id`, `creator_user_name`, `created_at`, and `last_used_at`. To keep
`application::api_key` focused while preserving field privacy, `auth.rs` supplies
crate-private `database()`, `now()`, and `credential_cipher()` accessors plus the existing
token digest/generator as crate-private helpers.

- [ ] **Step 1: Write failing application lifecycle tests**

Build the service with a fixed real cipher and test:

```rust
let cipher = Arc::new(CredentialCipher::new(
    CredentialKey::new(1, [9; 32]).unwrap(),
    Vec::new(),
).unwrap());
let service = service.with_credential_cipher(cipher);
service.create_api_key(&admin_principal, "Kodi Sync").await.unwrap();
let keys = service.list_api_keys(&admin_principal).await.unwrap();
assert_eq!(keys.len(), 1);
assert_eq!(format!("{:?}", keys[0].access_token()), "SecretApiKey([REDACTED])");
let key_principal = service
    .authenticate_token(keys[0].access_token().expose_secret())
    .await.unwrap();
assert_eq!(key_principal.api_key_id(), Some(keys[0].id()));
```

Query every `api_keys` column and assert no raw-token byte window occurs. Cover non-admin rejection, missing cipher, invalid app names, capacity mapping, creator name/password/policy invalidation, ciphertext/AAD swapping, whole-list failure, idempotent delete, and session-required capability/logout behavior.

- [ ] **Step 2: Run application tests to verify red**

Run: `cargo test -p tjxy-application --test auth_service_contract api_key --locked`

Expected: FAIL because lifecycle methods and types are missing.

- [ ] **Step 3: Implement secret and cipher boundaries**

Store API key strings in `Zeroizing<String>`. Implement only a deliberately named `expose_secret` accessor and a redacted `Debug`. Add `tjxy-credentials` and `zeroize` as direct application dependencies.

Creation validates the app, requires an enabled administrator, generates 256 random bits with the existing token generator, computes SHA-256, seals with a fresh `envelope_id` and the constant provider `tjxy-api-key/access-token/v1`, then passes only digest and envelope to the repository. Listing decrypts every bounded row and fails the whole operation on any cipher error. Delete validates the raw token bound and needs no decryption.

Extend `authenticate_token` only after the existing session lookup returns none. Retain
the existing disabled-user check after either lookup:

```rust
if let Some(principal) = AuthRepository::new(&self.database)
    .find_principal_by_token_digest(&digest, now).await?
{
    if principal.user().is_disabled() { return Err(AuthError::Forbidden); }
    return Ok(principal);
}
let principal = ApiKeyRepository::new(&self.database)
    .find_principal_by_token_digest(&digest, now).await?
    .ok_or(AuthError::InvalidToken)?;
if principal.user().is_disabled() { return Err(AuthError::Forbidden); }
Ok(principal)
```

Add explicit `InvalidApiKeyRequest`, `ApiKeyCapacity`, `CredentialCipherUnavailable`, cipher/repository, and `SessionRequired` errors without including secret values.

- [ ] **Step 4: Verify application lifecycle and regressions**

Run:

```bash
cargo test -p tjxy-application --test auth_service_contract api_key --locked
cargo test -p tjxy-application --test auth_service_contract --locked
cargo clippy -p tjxy-application --all-targets --locked -- -D warnings
```

Expected: all pass.

- [ ] **Step 5: Commit Task 5**

```bash
git add Cargo.lock crates/application/Cargo.toml crates/application/src/api_key.rs crates/application/src/auth.rs crates/application/src/lib.rs crates/application/tests/auth_service_contract.rs
git commit -m "feat(auth): manage encrypted API keys"
```

---

### Task 6: Fail Closed During Startup

**Files:**
- Modify: `crates/server/src/startup.rs`
- Modify: `crates/server/tests/startup.rs`

**Interfaces:**
- Produces: startup injection of the existing `StartupOptions.credential_cipher` and `InitializationError::ApiKeyValidation`.
- Consumes: `AuthService::with_credential_cipher` and `validate_api_key_envelopes` from Task 5.

- [ ] **Step 1: Write failing restart tests**

Use one `reconnectable_test_database` fixture across the direct AuthService seed and the
subsequent `initialize` call:

```rust
let fixture = reconnectable_test_database().await.unwrap();
tjxy_db::Migrator::up(fixture.connection(), None).await.unwrap();
let service = AuthService::new(
    fixture.connection().clone(), SystemClock, Some(Duration::days(30)), 2,
).await.unwrap().with_credential_cipher(cipher_v1.clone());
service.create_user("Admin", "password", true).await.unwrap();
let issued = service.authenticate(
    "Admin",
    "password",
    ClientIdentity::new("Test", "Browser", "startup-key-test", "1.0").unwrap(),
).await.unwrap();
let admin = service.authenticate_token(issued.access_token().expose_secret()).await.unwrap();
service.create_api_key(&admin, "Automation").await.unwrap();
drop(service);

let Err(error) = initialize(StartupOptions::new(
    fixture.database_url(), ServerIdentity::new(Uuid::new_v4(), "TJXY", "Linux"),
)).await else {
    panic!("persisted API keys unexpectedly started without a keyring");
};
assert!(matches!(error, InitializationError::ApiKeyValidation(_)));
```

Add cases for: no keys/no keyring succeeds; current key succeeds; active v2 plus historical v1 succeeds; v2 without historical v1 fails; corrupted payload fails; swapped envelope IDs fail.

- [ ] **Step 2: Run startup tests to verify red**

Run: `cargo test -p tjxy-server --test startup api_key --locked`

Expected: FAIL because AuthService is not initialized with or validating the keyring.

- [ ] **Step 3: Inject and validate before readiness**

Construct the mutable service, attach the optional cloned cipher, perform bootstrap-admin handling, then call `validate_api_key_envelopes` before wrapping the service in `Arc` and before `AppState.with_ready(true)`. Preserve the existing storage-backend keyring validation order and use an error variant that never includes cipher details or payload bytes.

- [ ] **Step 4: Verify startup and storage credential regressions**

Run:

```bash
cargo test -p tjxy-server --test startup --locked
cargo test -p tjxy-server --test storage_admin_routes --locked
```

Expected: all pass.

- [ ] **Step 5: Commit Task 6**

```bash
git add crates/server/src/startup.rs crates/server/tests/startup.rs
git commit -m "feat(server): validate API keys at startup"
```

---

### Task 7: Expose The Canonical `/Auth/Keys` Contract

**Files:**
- Create: `crates/server/src/api_key.rs`
- Create: `crates/server/tests/api_key_routes.rs`
- Modify: `crates/server/src/lib.rs`
- Modify: `crates/server/src/auth.rs`

**Interfaces:**
- Produces: `GET|POST /Auth/Keys`, `DELETE /Auth/Keys/{key}`, exact status mapping, and `Cache-Control: no-store`.
- Consumes: Task 1 DTOs and Task 5 AuthService lifecycle methods.

- [ ] **Step 1: Refresh Jellyfin and Axum route guidance through Context7**

Query the selected Jellyfin stable OpenAPI library for the complete GET/POST/DELETE
contract, then resolve `Axum` and query 0.8 behavior for `RawQuery`, percent-decoded
`Path<String>`, method routing, response headers, and rejection handling. Preserve the
project's stricter duplicate-query validation and secret-redaction invariants.

- [ ] **Step 2: Write failing route contracts**

Create a focused fixture with a fixed cipher, admin, normal user, and login helpers. Cover:

```rust
let created = token_request(app.clone(), Method::POST, "/Auth/Keys?app=Kodi%20Sync", &admin_token, None).await;
assert_eq!(created.status(), StatusCode::NO_CONTENT);

let listed = token_request(app.clone(), Method::GET, "/Auth/Keys", &admin_token, None).await;
assert_eq!(listed.status(), StatusCode::OK);
assert_eq!(listed.headers()[header::CACHE_CONTROL], "no-store");
let body = json_response(listed).await;
let raw_key = body["Items"][0]["AccessToken"].as_str().unwrap();

let me = token_request(app.clone(), Method::GET, "/Users/Me", raw_key, None).await;
assert_eq!(me.status(), StatusCode::OK);
```

Also cover 401, 403, empty/duplicate/overlong app, unexpected query fields, 409
capacity, 503 missing cipher, full DTO shape, canonical `ApiKey` query authentication,
legacy `api_key` behavior, idempotent unknown delete, self-delete, and error bodies that
do not contain the path key. Use the created key to assert `GET /Sessions` and
`GET /Devices` remain available to its administrator principal, while capabilities,
logout, playstate/ping, and both Google/OneDrive OAuth Start routes return 403 before
consulting their optional service dependencies.

- [ ] **Step 3: Run route tests to verify red**

Run: `cargo test -p tjxy-server --test api_key_routes --locked`

Expected: FAIL with route 404/module missing.

- [ ] **Step 4: Implement handlers and safe query parsing**

Use `RawQuery` plus the existing decoded query-pair helper so duplicate `app` values are rejected rather than silently overwritten. Remove only allowed auth query aliases. The DELETE handler receives Axum's decoded `Path<String>`, validates through AuthService, and never formats the token in an error.

Map errors exactly:

```text
InvalidApiKeyRequest -> 400
InvalidToken -> 401
Forbidden | SessionRequired -> 403
ApiKeyCapacity -> 409
CredentialCipherUnavailable | cipher/repository failure -> 503
```

Attach `Cache-Control: no-store` to list, create/delete, and API-key error responses. Register routes near Users/Sessions in `build_router`.

- [ ] **Step 5: Verify routes and existing authentication**

Run:

```bash
cargo test -p tjxy-server --test api_key_routes --locked
cargo test -p tjxy-server --test auth_routes --locked
cargo test -p tjxy-server --test browse_routes playback --locked
```

Expected: all pass.

- [ ] **Step 6: Commit Task 7**

```bash
git add crates/server/src/api_key.rs crates/server/src/auth.rs crates/server/src/lib.rs crates/server/tests/api_key_routes.rs
git commit -m "feat(server): expose API key lifecycle"
```

---

### Task 8: Prove The Durable Real-TCP Lifecycle

**Files:**
- Modify: `crates/server/tests/jellyfin_tcp_smoke.rs`

**Interfaces:**
- Produces: `tcp_api_key_lifecycle_is_durable`.
- Consumes: production router/startup, one persistent temporary SQLite database, and a fixed active/historical test keyring.

- [ ] **Step 1: Write the failing TCP/restart test**

The test must perform real HTTP over a bound loopback socket:

```text
login admin session
POST /Auth/Keys?app=Smoke -> 204
GET /Auth/Keys -> recover raw key
GET /Users/Me with raw key -> 200
stop server without deleting database
restart with active v2 + historical v1
GET /Users/Me with raw key -> 200
DELETE /Auth/Keys/{encoded raw key} -> 204
GET /Users/Me with deleted key -> 401
```

Assert the list response is `no-store`, the raw key is unchanged across restart, and no returned error contains the key.

- [ ] **Step 2: Run the TCP test to verify red**

Run: `cargo test -p tjxy-server --test jellyfin_tcp_smoke tcp_api_key_lifecycle_is_durable --locked -- --nocapture`

Expected: FAIL until the test server can restart with a persistent database and keyring.

- [ ] **Step 3: Extend only the test harness needed for restart**

Preserve the existing random loopback port and shutdown behavior. Add a constructor that accepts an existing database URL and optional cipher so the second server instance reuses state. Do not add sleeps; await readiness and graceful shutdown through existing synchronization.

- [ ] **Step 4: Verify TCP and all server integration tests**

Run:

```bash
cargo test -p tjxy-server --test jellyfin_tcp_smoke tcp_api_key_lifecycle_is_durable --locked -- --nocapture
cargo test -p tjxy-server --tests --locked
```

Expected: all pass. If loopback binding is sandbox-blocked, rerun the same commands with the required permission rather than weakening the test.

- [ ] **Step 5: Commit Task 8**

```bash
git add crates/server/tests/jellyfin_tcp_smoke.rs
git commit -m "test(server): prove durable API key lifecycle"
```

---

### Task 9: Add Strict Access API Adapters

**Files:**
- Create: `admin/src/access/deviceApi.ts`
- Create: `admin/src/access/deviceApi.test.ts`
- Create: `admin/src/access/apiKeyApi.ts`
- Create: `admin/src/access/apiKeyApi.test.ts`
- Modify: `admin/vite.config.ts`

**Interfaces:**
- Produces: `listDevices`, `updateDeviceName`, `deleteDevice`, `listApiKeys`, `createApiKey`, and `deleteApiKey`.
- Consumes: shared `apiRequest`, canonical PascalCase response envelopes, and empty 204 responses.

Use these signatures:

```ts
export async function listDevices(signal?: AbortSignal): Promise<DeviceInfo[]>;
export async function updateDeviceName(deviceId: string, customName: string | null): Promise<void>;
export async function deleteDevice(deviceId: string): Promise<void>;
export async function listApiKeys(signal?: AbortSignal): Promise<ApiKeyInfo[]>;
export async function createApiKey(appName: string): Promise<void>;
export async function deleteApiKey(rawToken: string): Promise<void>;
```

- [ ] **Step 1: Fetch current React Admin/MUI guidance through Context7**

Resolve `React Admin` first, then query the approved task: custom authenticated routes/menu items and MUI tabs/dialog/icon-button accessibility in React Admin 5.15.1. Resolve/query MUI separately if the React Admin documentation does not cover controlled Tabs and Dialog focus. Record only the relevant API decisions in implementation notes; do not add dependencies.

- [ ] **Step 2: Write failing adapter tests**

Test exact request shapes and strict guards:

```ts
await createApiKey('Kodi / Sync');
expect(requestMock).toHaveBeenCalledWith('/Auth/Keys?app=Kodi+%2F+Sync', { method: 'POST' });

await deleteApiKey('raw/key');
expect(requestMock).toHaveBeenCalledWith('/Auth/Keys/raw%2Fkey', { method: 'DELETE' });

await updateDeviceName('Phone', 'Living room');
expect(requestMock).toHaveBeenCalledWith('/Devices/Options?id=Phone', {
  method: 'POST',
  body: JSON.stringify({ DeviceId: 'Phone', CustomName: 'Living room' }),
});
```

Cover complete/nullable key fields, malformed wrappers/records, case-sensitive DeviceIds, abort signals, encoded delete IDs, and no inclusion of a raw token in an `ApiError` message.

- [ ] **Step 3: Run adapter tests to verify red**

Run: `npm --prefix admin test -- --run src/access/deviceApi.test.ts src/access/apiKeyApi.test.ts`

Expected: FAIL because modules are missing.

- [ ] **Step 4: Implement adapters and exact dev proxies**

Follow the existing `libraryApi.ts` pattern: call `apiRequest<unknown>`, validate every field before returning a typed record, use `URLSearchParams` for query values, and `encodeURIComponent` for path segments. Do not store, log, or globally cache returned keys. Add exact `/Devices` and `/Auth` Vite proxy prefixes without rewrites.

- [ ] **Step 5: Verify adapters, typecheck, and lint**

Run:

```bash
npm --prefix admin test -- --run src/access/deviceApi.test.ts src/access/apiKeyApi.test.ts
npm --prefix admin run typecheck
npm --prefix admin run lint
```

Expected: all pass.

- [ ] **Step 6: Commit Task 9**

```bash
git add admin/src/access/deviceApi.ts admin/src/access/deviceApi.test.ts admin/src/access/apiKeyApi.ts admin/src/access/apiKeyApi.test.ts admin/vite.config.ts
git commit -m "feat(admin): add access management adapters"
```

---

### Task 10: Build The Responsive Access Workflow

**Files:**
- Create: `admin/src/access/AccessPage.tsx`
- Create: `admin/src/access/DevicesPanel.tsx`
- Create: `admin/src/access/DevicesPanel.test.tsx`
- Create: `admin/src/access/ApiKeysPanel.tsx`
- Create: `admin/src/access/ApiKeysPanel.test.tsx`
- Create: `admin/src/access/AccessPage.test.tsx`
- Modify: `admin/src/App.tsx`
- Modify: `admin/src/layout/AdminLayout.tsx`

**Interfaces:**
- Produces: authenticated `/admin/access`, one Access menu item, and accessible Devices/API Keys tabs.
- Consumes: Task 9 adapters, existing theme, `useNotify`, MUI Tabs/Dialog/Table/IconButton, and existing responsive list patterns.

- [ ] **Step 1: Write failing page-shell and Devices tests**

Assert tab semantics, abortable loading, empty states, rename/refetch, and revoke confirmation:

```tsx
await screen.findByRole('tab', { name: 'Devices' });
expect(screen.getByRole('tab', { name: 'Devices' })).toHaveAttribute('aria-selected', 'true');
await user.click(screen.getByRole('button', { name: 'Edit Living room' }));
await user.clear(screen.getByRole('textbox', { name: 'Custom device name' }));
await user.type(screen.getByRole('textbox', { name: 'Custom device name' }), 'Bedroom');
await user.click(screen.getByRole('button', { name: 'Save device name' }));
expect(updateDeviceName).toHaveBeenCalledWith('Phone', 'Bedroom');
await waitFor(() => expect(listDevices).toHaveBeenCalledTimes(2));
```

Confirm revocation by effective device name, never by an opaque hash. Assert mutation buttons are disabled while pending and failures preserve editable state.

- [ ] **Step 2: Write failing API Keys interaction tests**

Cover create/refetch, mask/reveal/hide/copy, delete/refetch, and secret lifecycle:

```tsx
expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
await user.click(screen.getByRole('button', { name: 'Show key for Kodi Sync' }));
expect(screen.getByText(rawToken)).toBeVisible();
await user.click(screen.getByRole('button', { name: 'Copy key for Kodi Sync' }));
expect(navigator.clipboard.writeText).toHaveBeenCalledWith(rawToken);
for (const storage of [sessionStorage, localStorage]) {
  for (let index = 0; index < storage.length; index += 1) {
    expect(storage.getItem(storage.key(index) ?? '')).not.toContain(rawToken);
  }
}
```

Refetch and unmount must reset reveal state. Delete confirmation and all notifications identify only the app name. Error assertions include a deliberately distinctive token and prove it never appears in visible text.

- [ ] **Step 3: Run component tests to verify red**

Run:

```bash
npm --prefix admin test -- --run src/access/AccessPage.test.tsx src/access/DevicesPanel.test.tsx src/access/ApiKeysPanel.test.tsx
```

Expected: FAIL because components/routes are missing.

- [ ] **Step 4: Implement the page and focused panels**

`AccessPage` owns only the title, controlled tab value, and panel composition. Each panel owns its abortable list/refetch and focused dialogs. Use a compact desktop table and an `sm`-down stacked layout; do not make mobile users horizontally scroll a wide table. Use MUI icons with tooltips and stable 40px action targets.

API keys render a fixed mask until revealed. Reveal state is a `Set<number>` in component memory and is cleared before every authoritative refetch and during unmount. Clipboard failure produces a nonsecret error notification. No secret is used as a React key, URL, dialog title, or analytics label.

Register:

```tsx
<Route path="/access" element={<Authenticated><AccessPage /></Authenticated>} />
```

Add one `SecurityOutlined` Access menu item. Do not change the Users `dataProvider` or add placeholder pages.

- [ ] **Step 5: Verify UI, route, type, lint, and build**

Run:

```bash
npm --prefix admin test -- --run src/access/AccessPage.test.tsx src/access/DevicesPanel.test.tsx src/access/ApiKeysPanel.test.tsx
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build
```

Expected: all pass.

- [ ] **Step 6: Commit Task 10**

```bash
git add admin/src/access/AccessPage.tsx admin/src/access/AccessPage.test.tsx admin/src/access/DevicesPanel.tsx admin/src/access/DevicesPanel.test.tsx admin/src/access/ApiKeysPanel.tsx admin/src/access/ApiKeysPanel.test.tsx admin/src/App.tsx admin/src/layout/AdminLayout.tsx
git commit -m "feat(admin): add devices and API keys page"
```

---

### Task 11: Production Browser Evidence, Documentation, And Full Gate

**Files:**
- Create: `admin/e2e/support.ts`
- Create: `admin/e2e/access.spec.ts`
- Modify: `admin/e2e/users.spec.ts`
- Modify: `admin/scripts/run-e2e-server.sh`
- Modify: `README.md`
- Modify: `docs/api-parity.md`

**Interfaces:**
- Produces: a production Access lifecycle, secret-safe diagnostics, deployment guidance, and updated parity evidence.
- Consumes: the complete backend/frontend implementation and existing Playwright production server.

- [ ] **Step 1: Write the failing production browser lifecycle**

Add one serial test that:

```text
logs in through the production Admin build
creates an API key and reads it only after clicking reveal
authenticates GET /Users/Me with that key
reloads and proves the same key is recoverable but masked
creates a secondary-device login through Playwright request context
renames and revokes only the secondary device
deletes the API key by app-name confirmation
proves the deleted key receives 401
checks desktop and 390x844 layouts for overflow and action overlap
checks page errors, console errors, and same-origin request failures
```

Run: `npm --prefix admin run e2e -- access.spec.ts`

Expected: FAIL because the E2E keyring, support module, and test do not exist.

- [ ] **Step 2: Extract and harden shared Playwright support**

Move `login`, `monitorPage`, screenshot, horizontal-overflow, and action-intersection helpers from `users.spec.ts` into `support.ts`. Keep existing Users behavior unchanged. Sanitize diagnostics before formatting:

```ts
export function safeRequestPath(url: string): string {
  const pathname = new URL(url).pathname;
  return /^\/Auth\/Keys\/[^/]+$/u.test(pathname)
    ? '/Auth/Keys/[REDACTED]'
    : pathname;
}
```

Never append `request.url()` for an API key DELETE failure. Configure the temporary E2E server with a deterministic 32-byte test keyring:

```sh
export TJXY_CREDENTIAL_KEYRING='{"active_version":1,"keys":{"1":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}}'
```

- [ ] **Step 3: Implement and verify the production E2E workflow**

Use a secondary MediaBrowser `DeviceId` for device revocation so the UI's own session remains valid. Read the raw key from the revealed cell into a local test variable only; do not attach it to screenshots, traces, test titles, or error messages.

Run:

```bash
npm --prefix admin run build
npm --prefix admin run e2e -- access.spec.ts
npm --prefix admin run e2e
```

Expected: both the focused and full production Playwright suites pass and the server process exits.

- [ ] **Step 4: Update operator and parity documentation**

README must state that API key creation/listing needs `TJXY_CREDENTIAL_KEYRING`, existing keys make the keyring a startup requirement, and historical versions must remain configured while referenced. `docs/api-parity.md` must mark the API Keys backend and Access page complete only after every command in this task is green; retain unrelated incomplete PLAN items.

- [ ] **Step 5: Run the complete local gate**

Run fresh:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build
npm --prefix admin run e2e
git diff --check
```

Expected: every command exits 0; the workspace test output has zero failures other than already-declared ignored environment contracts.

- [ ] **Step 6: Run PostgreSQL, MySQL, and Redis release contracts**

Start disposable PostgreSQL 17, MySQL 8.4, and Redis 7.4 instances on unused local ports, then run the same backend-aware suites used by CI:

```bash
TJXY_TEST_DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:15432/tjxy_test cargo test -p tjxy-test-support -p tjxy-db -p tjxy-application -p tjxy-server --tests --locked
TJXY_TEST_DATABASE_URL=mysql://root:root@127.0.0.1:13306/tjxy_test cargo test -p tjxy-test-support -p tjxy-db --tests --locked
TJXY_TEST_REDIS_URL=redis://127.0.0.1:16379/ cargo test -p tjxy-cache --test redis_invalidation_contract --locked -- --ignored
```

Expected: all suites pass. MySQL results are mandatory evidence even if CI still marks that job `continue-on-error`.

- [ ] **Step 7: Perform the required two-axis code review**

Use the code-review skill against fixed point `9ac51c4`, with the approved design spec as the Spec source and repository instructions plus the smell baseline as Standards. Resolve all high/medium correctness, security, and spec findings; rerun the affected focused tests after every fix.

- [ ] **Step 8: Re-audit `PLAN.md` and commit Task 11**

Re-read the release gates and `docs/api-parity.md`. Confirm only the API Keys/Devices Admin gap has closed; do not call the overall plan complete while storage reauthorization, metadata, migration/conflict pages, identity/import, disconnect, or other verified gaps remain.

```bash
git add admin/e2e/support.ts admin/e2e/access.spec.ts admin/e2e/users.spec.ts admin/scripts/run-e2e-server.sh README.md docs/api-parity.md
git commit -m "test: verify access management lifecycle"
```

After this commit, capture `git status --short`, the task commit list, every verification command and result, remaining PLAN gaps, and whether the environment is ready for preliminary rather than final real smoke testing.
