# TMDB Admin Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an administrator securely configure, enable, test, replace, and remove the TMDB API Read Access Token from the HeroUI admin application without restarting TJXY.

**Architecture:** A reloadable metadata-provider wrapper gives the long-running metadata worker an atomic snapshot of the current TMDB provider. A dedicated SQL repository stores provider state and an AES-GCM credential envelope; a server service decrypts only at the runtime boundary and hot-swaps the provider after durable writes. The HeroUI page consumes administrator-only no-store endpoints that never return credential plaintext.

**Tech Stack:** Rust 1.88, Axum, SeaORM/sea-query migrations, AES-GCM `CredentialCipher`, Tokio, React 19, TypeScript, HeroUI v3, Tailwind CSS v4, ra-core, Vitest, Playwright.

## Global Constraints

- Work only in `.worktrees/admin-heroui-rebuild` on `codex/admin-heroui-rebuild`; preserve all pre-existing dirty E2E, QA, README, and documentation changes.
- The accepted credential is the TMDB API Read Access Token used as an HTTPS Bearer token, not the legacy v3 `api_key` query parameter.
- Credential plaintext must never be stored unencrypted, serialized in GET/PUT/DELETE responses, written to logs, included in `Debug`, or persisted in browser storage.
- Persisted credentials require `TJXY_CREDENTIAL_KEYRING`; writes and configured-token tests fail closed with HTTP 503 when the cipher is unavailable.
- Database configuration overrides the environment fallback. Existing `TJXY_ENABLE_REMOTE_PROVIDERS`, `TJXY_TMDB_ACCESS_TOKEN`, and `TJXY_TMDB_LANGUAGE` remain supported when no database row exists.
- Saving, enabling, disabling, rotating, or deleting TMDB settings updates subsequent metadata jobs without restarting. An already-running request may finish with its captured provider snapshot.
- Administrator routes use strict JSON DTOs, reject unknown fields, require administrator authentication, and add `Cache-Control: no-store`.
- Use HeroUI v3 compound components and `onPress`; do not introduce MUI, Emotion, `react-admin`, or a HeroUI provider.
- Follow red-green-refactor: each production behavior starts with a focused failing test that is run and observed before implementation.

---

### Task 1: Reloadable TMDB Provider and Connection Check

**Files:**
- Modify: `crates/metadata/src/lib.rs`
- Modify: `crates/metadata/tests/tmdb_provider_contract.rs`
- Create: `crates/metadata/tests/reloadable_provider_contract.rs`

**Interfaces:**
- Produces: `ReloadableMetadataProvider::new(name)`, `replace(Option<Arc<dyn MetadataProvider>>)`, and an implementation of `MetadataProvider`.
- Produces: `TmdbProvider::validate_connection(&self) -> Result<(), MetadataProviderError>`.
- Consumes: existing `MetadataProvider`, `MetadataLookup`, `TmdbTransport`, and `TmdbProvider`.

- [ ] **Step 1: Write failing reloadable-provider tests**

  Add fake providers that return distinct literal titles. Prove that an empty wrapper returns `None`, a replacement is used by the next `resolve`, disabling returns `None`, and a resolution that already cloned the old provider can complete while the next resolution uses the replacement.

- [ ] **Step 2: Run the reloadable-provider test and verify RED**

  Run: `cargo +1.88.0 test -p tjxy-metadata --test reloadable_provider_contract`

  Expected: compilation fails because `ReloadableMetadataProvider` does not exist.

- [ ] **Step 3: Implement the minimal reloadable wrapper**

  Store `Option<Arc<dyn MetadataProvider>>` behind a short-lived lock. Clone the snapshot before awaiting `resolve`; never hold the lock across network or provider work. Preserve a stable provider name of `Tmdb`.

- [ ] **Step 4: Run the reloadable-provider test and verify GREEN**

  Run: `cargo +1.88.0 test -p tjxy-metadata --test reloadable_provider_contract`

  Expected: all reload and disable behaviors pass.

- [ ] **Step 5: Write a failing TMDB connection-validation test**

  Extend the existing fake transport with a literal success/failure validation result and assert `TmdbProvider::validate_connection` forwards the real result without performing a search.

- [ ] **Step 6: Run the TMDB contract and verify RED**

  Run: `cargo +1.88.0 test -p tjxy-metadata --test tmdb_provider_contract`

  Expected: compilation fails because `validate_connection` is absent.

- [ ] **Step 7: Implement bounded production validation**

  Add a default `validate` method to `TmdbTransport` for compatibility, override it in the production transport, and request `https://api.themoviedb.org/3/configuration` with the same bounded HTTPS-only, no-redirect client and Bearer header. Map 401/403 to `Rejected`, transient statuses/network failures to `TemporarilyUnavailable`, oversized or malformed success bodies to `InvalidResponse`.

- [ ] **Step 8: Run metadata tests**

  Run: `cargo +1.88.0 test -p tjxy-metadata --tests`

  Expected: all metadata tests pass.

- [ ] **Step 9: Commit**

  Commit message: `feat(metadata): support reloadable tmdb provider`

### Task 2: Encrypted Metadata Provider Settings Repository

**Files:**
- Create: `crates/db/src/migration/m20260730_000039_metadata_provider_settings.rs`
- Modify: `crates/db/src/migration/mod.rs`
- Create: `crates/db/src/metadata_provider_settings.rs`
- Modify: `crates/db/src/lib.rs`
- Create: `crates/db/tests/metadata_provider_settings_repository_contract.rs`
- Modify: `crates/db/tests/schema_contract.rs`

**Interfaces:**
- Produces: `MetadataProviderSettingRecord` with provider, enabled, language, credential ID, envelope, revision, and update time.
- Produces: `MetadataProviderSettingsRepository::{get, put, delete}` with optimistic revision checks and atomic credential/state persistence.
- Consumes: `CredentialEnvelope`; this persistence layer never accepts plaintext.

- [ ] **Step 1: Write failing repository and schema tests**

  Prove a missing `tmdb` row returns `None`; first put creates revision `1`; update with revision `1` returns revision `2`; stale revision is rejected; encrypted payload bytes do not contain the literal token fixture; delete removes the row; rollback removes the new table.

- [ ] **Step 2: Run focused DB tests and verify RED**

  Run: `cargo +1.88.0 test -p tjxy-db --test metadata_provider_settings_repository_contract`

  Expected: compilation fails because the repository is absent.

- [ ] **Step 3: Add migration 39**

  Create `metadata_provider_settings` with a provider primary key, enabled flag, language, credential UUID, encrypted payload, key version, positive revision, and timestamps. Use portable column definitions for SQLite, PostgreSQL, and MySQL and a reversible `down`.

- [ ] **Step 4: Implement the repository**

  Validate the provider key and language bounds, create/rotate the envelope and settings in one transaction, compare expected revision before update, and delete atomically. Map stale writes to a dedicated conflict error and malformed stored envelopes to a redacted error.

- [ ] **Step 5: Run focused repository and schema tests**

  Run:
  - `cargo +1.88.0 test -p tjxy-db --test metadata_provider_settings_repository_contract`
  - `cargo +1.88.0 test -p tjxy-db --test schema_contract`

  Expected: both pass.

- [ ] **Step 6: Commit**

  Commit message: `feat(db): persist encrypted metadata settings`

### Task 3: Administrator API, Startup Loading, and Hot Apply

**Files:**
- Create: `crates/server/src/metadata_settings_admin.rs`
- Modify: `crates/server/src/lib.rs`
- Modify: `crates/server/src/main.rs`
- Modify: `crates/server/src/startup.rs`
- Create: `crates/server/tests/metadata_provider_settings_routes.rs`
- Modify: `crates/server/tests/startup.rs`
- Modify: `README.md`

**Interfaces:**
- Produces routes:
  - `GET /Admin/Metadata/Providers/Tmdb`
  - `PUT /Admin/Metadata/Providers/Tmdb`
  - `DELETE /Admin/Metadata/Providers/Tmdb`
  - `POST /Admin/Metadata/Providers/Tmdb/Test`
- GET response: `{Provider, Configured, Enabled, Language, Revision, Source, EncryptionAvailable}`.
- PUT request: `{Enabled, Language, AccessToken?, Revision?}`; an omitted token retains the stored credential.
- Test request: `{AccessToken?, Language?}`; a provided draft is tested without persistence, omission tests the configured token.
- Consumes: `MetadataProviderSettingsRepository`, `CredentialCipher`, `ReloadableMetadataProvider`, and `TmdbProvider`.

- [ ] **Step 1: Write failing administrator route tests**

  Cover unauthenticated and non-admin denial, strict content type and unknown fields, unconfigured GET, PUT without cipher returning 503, encrypted save/rotation, GET never containing the token, revision conflict returning 409, enable/disable hot apply, draft test without persistence, configured-token test, DELETE idempotence, and `Cache-Control: no-store`.

- [ ] **Step 2: Run route tests and verify RED**

  Run: `cargo +1.88.0 test -p tjxy-server --test metadata_provider_settings_routes`

  Expected: route requests return 404 or fail to compile because the service is absent.

- [ ] **Step 3: Implement the admin service and routes**

  Authenticate with the existing administrator guard. Decrypt only inside the service, use `Zeroizing` for plaintext buffers, construct a new `TmdbProvider`, persist before swapping, and swap only after a successful durable mutation. Return redacted error variants and no-store responses.

- [ ] **Step 4: Run route tests and verify GREEN**

  Run: `cargo +1.88.0 test -p tjxy-server --test metadata_provider_settings_routes`

  Expected: all route contracts pass.

- [ ] **Step 5: Write failing startup precedence tests**

  Prove persisted enabled settings replace the environment fallback, persisted disabled settings suppress it, missing database settings preserve the environment fallback, and an unreadable persisted envelope prevents readiness without exposing plaintext.

- [ ] **Step 6: Run startup tests and verify RED**

  Run: `cargo +1.88.0 test -p tjxy-server --test startup`

  Expected: the new persisted-TMDB cases fail.

- [ ] **Step 7: Wire startup and shared runtime**

  Create one reloadable TMDB handle, initialize it from environment fallback in `main`, then after migrations load and decrypt a database override in `startup`. Pass the same handle to the metadata worker and metadata-settings admin service. Keep other injected metadata providers unchanged.

- [ ] **Step 8: Run server tests**

  Run:
  - `cargo +1.88.0 test -p tjxy-server --test startup`
  - `cargo +1.88.0 test -p tjxy-server --test metadata_provider_settings_routes`
  - `cargo +1.88.0 test -p tjxy-server --test metadata_admin_routes`

  Expected: all pass.

- [ ] **Step 9: Document deployment behavior**

  Update the TMDB README section to describe the admin setting, encryption-keyring requirement, hot application, database-over-environment precedence, and the API Read Access Token credential type.

- [ ] **Step 10: Commit**

  Commit message: `feat(server): manage tmdb settings at runtime`

### Task 4: HeroUI Metadata Settings Page

**Files:**
- Create: `admin/src/settings/metadataSettingsApi.ts`
- Create: `admin/src/settings/metadataSettingsApi.test.ts`
- Create: `admin/src/settings/MetadataSettingsPage.tsx`
- Create: `admin/src/settings/MetadataSettingsPage.test.tsx`
- Modify: `admin/src/App.tsx`
- Modify: `admin/src/App.test.tsx`
- Modify: `admin/src/layout/adminNavigation.ts`
- Modify: `admin/src/layout/AdminShell.test.tsx`

**Interfaces:**
- Consumes the four administrator endpoints from Task 3 through the existing `httpClient`.
- Produces route `/admin/settings/metadata` and a `System` navigation group containing `Metadata`.
- Uses write-only local draft state for `AccessToken`; successful saves and unmounts clear it.

- [ ] **Step 1: Write failing API-client tests**

  Assert exact endpoint/method/body contracts, abort-signal forwarding, strict response validation, and that malformed-response errors never include a submitted token.

- [ ] **Step 2: Run the API-client test and verify RED**

  Run: `npm --prefix admin test -- --run src/settings/metadataSettingsApi.test.ts`

  Expected: compilation fails because the client module is absent.

- [ ] **Step 3: Implement the strict API client**

  Add explicit response validators for provider/source/status fields. Use the shared `httpClient`; do not cache or persist the token.

- [ ] **Step 4: Run API-client tests and verify GREEN**

  Run: `npm --prefix admin test -- --run src/settings/metadataSettingsApi.test.ts`

  Expected: pass.

- [ ] **Step 5: Write failing page, route, and navigation tests**

  Cover loading/error/retry, unconfigured/configured status, empty secret field after load, show/hide, language and enabled editing, save pending/success/conflict/failure, draft connection test, configured connection test, remove confirmation, secret clearing after success, guarded route rendering, and the new navigation order.

- [ ] **Step 6: Run focused UI tests and verify RED**

  Run:
  - `npm --prefix admin test -- --run src/settings/MetadataSettingsPage.test.tsx`
  - `npm --prefix admin test -- --run src/App.test.tsx src/layout/AdminShell.test.tsx`

  Expected: failures because the page, route, and navigation item are absent.

- [ ] **Step 7: Implement the HeroUI page**

  Compose `PageHeader`, `Alert`, `Card`, `TextField`, `Input`, `Switch`, `Button`, `Tooltip`, and status `Chip`. Label the secret `TMDB API Read Access Token`, explain that replacement is write-only, and show environment/database source without showing credential material. Use `onPress`, keyboard-accessible confirmation, and `useNotify`.

- [ ] **Step 8: Wire route and navigation**

  Add `/settings/metadata` to `App.tsx`; extend `AdminNavigationGroup.label` with `System`; add `Metadata` using a Lucide settings/database icon.

- [ ] **Step 9: Run frontend gates**

  Run:
  - `npm --prefix admin test -- --run src/settings/metadataSettingsApi.test.ts src/settings/MetadataSettingsPage.test.tsx src/App.test.tsx src/layout/AdminShell.test.tsx src/test/dependencyBoundary.test.ts`
  - `npm --prefix admin run typecheck`
  - `npm --prefix admin run lint`
  - `npm --prefix admin run build`

  Expected: all pass with no MUI/Emotion/react-admin boundary violation.

- [ ] **Step 10: Commit**

  Commit message: `feat(admin): add tmdb metadata settings`

### Task 5: Integrated Verification and Browser QA

**Files:**
- Modify only files required to fix verified integration defects.
- Add an E2E test only if the existing fixture can exercise the settings flow without contacting TMDB.

**Interfaces:**
- Consumes all previous task outputs.
- Produces a tested running admin settings page and documented verification evidence.

- [ ] **Step 1: Run Rust formatting and focused workspace gates**

  Run:
  - `cargo +1.88.0 fmt --all -- --check`
  - `cargo +1.88.0 test -p tjxy-metadata --tests`
  - `cargo +1.88.0 test -p tjxy-db --tests`
  - `cargo +1.88.0 test -p tjxy-server --tests`
  - `cargo +1.88.0 clippy -p tjxy-metadata -p tjxy-db -p tjxy-server --all-targets -- -D warnings`

- [ ] **Step 2: Run the complete frontend test suite**

  Run:
  - `npm --prefix admin test -- --run`
  - `npm --prefix admin run typecheck`
  - `npm --prefix admin run lint`
  - `npm --prefix admin run build`

- [ ] **Step 3: Start the updated service**

  Stop only the known development process from this worktree, restart it with the existing local development configuration and credential keyring, and preserve any unrelated processes.

- [ ] **Step 4: Browser QA every new state**

  Open `/admin/settings/metadata`; verify desktop and narrow viewport layout, keyboard focus, loading, configured/unconfigured presentation, secret masking, show/hide, validation, save/replace, connection-test feedback, disable, and remove. Confirm browser network responses never contain the token.

- [ ] **Step 5: Run the existing Playwright smoke set**

  Run the smallest existing authenticated smoke set that covers login, shell navigation, and secret safety. Fix only regressions caused by this feature.

- [ ] **Step 6: Final code review**

  Review the complete branch diff for security, concurrency, API consistency, HeroUI v3 correctness, and accidental overlap with the pre-existing dirty files. Resolve all load-bearing findings.

- [ ] **Step 7: Commit verification fixes**

  If verification required code changes, commit them as `fix: harden tmdb settings flow`. Otherwise do not create an empty commit.
