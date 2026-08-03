# AI P0 Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the four confirmed High-severity P0 findings: AI chat admission exhaustion, provider DNS-rebinding SSRF, media-library visibility bypasses, and non-atomic system-settings revisions.

**Architecture:** Keep admission state in the server process for fast per-user and concurrent-SSE rejection, and keep the daily quota in a durable SeaORM table so restarts cannot reset it. Route all AI provider HTTP calls through one transport that resolves and validates the complete DNS answer set immediately before connecting, pins the approved addresses with Reqwest, and is injectable in tests. Centralize the catalog visibility predicate and apply it to every AI, Discover, and dashboard aggregate query. Replace system-settings read/compare/upsert with a transactional insert-or-compare-and-swap update that treats `rows_affected() != 1` as a conflict.

**Tech Stack:** Rust 1.88, Axum, Tokio 1.45, Reqwest 0.12.28, SeaORM 1.1.14, SQLite/PostgreSQL/MySQL contract tests, `async-stream`, `chrono`, and existing repository test fixtures.

## Global Constraints

- Preserve all existing uncommitted work in the dirty worktree; do not reset, checkout, or overwrite unrelated files.
- Use the repository's existing SeaQuery/SeaORM query builders and error enums; do not introduce a second persistence or HTTP abstraction for an existing responsibility.
- Follow TDD for every behavior change: add a focused failing test, run it to record the failure, implement the smallest complete change, then rerun the focused test before moving on.
- Reject provider DNS answers if any address is loopback, private, link-local, unspecified, multicast, documentation, benchmark, or otherwise reserved; IPv4-mapped IPv6 addresses must be classified by their mapped IPv4 address.
- Resolve provider hostnames for each logical provider operation, pin the complete validated answer set with Reqwest `resolve_to_addrs`, preserve the hostname for HTTP Host/TLS SNI, and call `.no_proxy()` so proxy environment variables cannot bypass the policy.
- Do not add a production loopback allow-list, environment escape hatch, or test-only branch selected by an environment variable.
- AI admission defaults are 10 accepted chats/minute/user, 2 concurrent SSE streams/user, 8 concurrent SSE streams globally, and 100 accepted chats/UTC day/user.
- Admission rejections return HTTP 429 with `Retry-After` (at least one second) and `Cache-Control: no-store`, before an SSE body or provider request is opened.
- The daily-quota migration number is `m20260803_000051_ai_daily_quota.rs`; `000049` and `000050` are already present in `crates/db/src/migration/mod.rs`.
- Do not use SQL `RETURNING` or backend-specific date/time functions; the application passes `Utc::now().date_naive()` to the repository.
- Extend the existing untracked/WIP `crates/db/src/ai_usage.rs`, `AiUsageRepository`, and `m20260803_000050_ai_usage_analytics.rs`; do not replace or duplicate the user's analytics work.
- Update nearby documentation when public behavior changes, especially the README's provider URL/loopback wording.

---

## File Map

The implementation is intentionally split by responsibility so each task has one reviewable boundary:

- Create `crates/db/src/migration/m20260803_000051_ai_daily_quota.rs`: durable per-user UTC-day quota table and indexes.
- Modify the existing `crates/db/src/ai_usage.rs`: add atomic daily quota methods to the existing analytics repository.
- Modify `crates/db/src/migration/mod.rs`: register the quota migration without changing the existing `000050` analytics registration.
- Create `crates/server/src/ai_admission.rs`: validated configuration, sliding-minute counters, and RAII semaphore leases.
- Modify `crates/server/src/lib.rs`, `crates/server/src/startup.rs`, `crates/server/src/main.rs`: wire admission configuration through startup and AppState.
- Modify `crates/server/src/ai.rs`: reserve admission, consume durable quota, retain the SSE permit through stream cancellation, and map admission errors to 429.
- Create `crates/server/src/ai_provider.rs`: provider transport trait, DNS/address policy, Reqwest address pinning, response-size limit, and transport errors.
- Modify `crates/server/src/ai.rs`, `crates/server/src/lib.rs`, `crates/server/tests/ai_routes.rs`, `crates/server/tests/ai_settings_routes.rs`, `README.md`: use the transport, inject fakes in tests, remove production loopback assumptions, and document the new policy.
- Create `crates/db/src/catalog_visibility.rs`: reusable correlated `EXISTS` condition for canonical catalog visibility.
- Modify `crates/db/src/catalog_query.rs`, `crates/db/src/lib.rs`: make `catalog_item_is_visible` reuse the condition and expose it to query repositories.
- Modify `crates/server/src/ai.rs`, `crates/server/src/client_portal.rs`, `crates/db/src/dashboard.rs`: apply the condition to favorites, history/insights, Popular/Server Top fallbacks, and dashboard top items.
- Modify `crates/server/tests/ai_routes.rs`, `crates/server/tests/browse_routes.rs`, `crates/db/tests/catalog_query_repository_contract.rs`: disabled-library and removed-membership regression coverage.
- Modify `crates/db/src/system_settings.rs`: transactional insert/update CAS, atomic locale updates, and rollback/error mapping.
- Modify `crates/server/src/system_settings.rs`: map setup conflicts to HTTP 409.
- Create `crates/db/tests/system_settings_repository_contract.rs`, modify `crates/server/tests/browse_routes.rs`: stale revision and concurrent-save coverage.
- Modify `README.md`: document admission defaults, environment overrides, 429 behavior, and the provider network policy.

Each implementation task below ends with a focused test command and a commit boundary. Keep commits ordered as written because later tasks consume the interfaces introduced earlier.

## Task 1: Add the durable daily AI quota

**Files:**
- Create: `crates/db/src/migration/m20260803_000051_ai_daily_quota.rs`
- Modify: `crates/db/src/ai_usage.rs`
- Modify: `crates/db/src/migration/mod.rs`
- Test: `crates/db/tests/ai_repository_contract.rs`
- Test: `crates/db/tests/schema_contract.rs`

**Interfaces:**
- Extend the existing `AiUsageRepository<'a>`; preserve its `record` and `analytics` methods and existing exported analytics types.
- Produce `async fn try_consume_daily_quota(&self, user_id: UserId, usage_day: NaiveDate, limit: u32) -> Result<bool, AiUsageRepositoryError>`.
- Produce `async fn daily_quota_count(&self, user_id: UserId, usage_day: NaiveDate) -> Result<u64, AiUsageRepositoryError>` for contract assertions and future quota-status responses.

```rust
impl<'a> AiUsageRepository<'a> {
    pub async fn try_consume_daily_quota(
        &self,
        user_id: UserId,
        usage_day: NaiveDate,
        limit: u32,
    ) -> Result<bool, AiUsageRepositoryError>;

    pub async fn daily_quota_count(
        &self,
        user_id: UserId,
        usage_day: NaiveDate,
    ) -> Result<u64, AiUsageRepositoryError>;
}
```

- [ ] **Step 1: Write the failing repository contract.** Extend `ai_repository_contract.rs` and assert the following exact sequence: first `try_consume_daily_quota(user, 2026-08-03, 2)` is `true` with count 1; second is `true` with count 2; third is `false` with count 2; the next UTC day is `true`; a different user is `true`; `limit == 0` returns `AiUsageRepositoryError::InvalidInput`. Add a `tokio::spawn` test using five database connections, limit 5, and ten concurrent calls; assert exactly five `true` results and final count 5.

- [ ] **Step 2: Run the contract to capture the failure.**

  Run: `cargo test -p tjxy-db --test ai_repository_contract daily_quota --locked`

  Expected: FAIL because `AiUsageRepository` does not yet expose the daily-quota methods and the quota table is absent.

- [ ] **Step 3: Add migration `m20260803_000051_ai_daily_quota.rs`.** Create `ai_daily_usage` with `id` UUID primary key, `user_id` UUID foreign key to `users` with cascade delete, `day_key` VARCHAR(10) not null, `request_count` BIGINT not null default 0, and `created_at`/`updated_at` timestamps using the project's migration conventions. Add a unique index on `(user_id, day_key)` and a user foreign-key index. Register it immediately after `m20260803_000050_ai_usage_analytics`; do not edit that migration. Using `day_key` matches the existing analytics representation while the public API still receives `NaiveDate` computed in UTC.

- [ ] **Step 4: Implement the atomic repository operation.** Format `usage_day` as `%Y-%m-%d`. In one transaction, insert `(user_id, day_key, request_count=0)` idempotently using the repository's backend-aware conflict helper (MySQL uses `update_column(id)`; SQLite/Postgres use `do_nothing`). Then issue an `UPDATE` with the exact predicate `user_id = ? AND day_key = ? AND request_count < limit`, setting `request_count = request_count + 1` and `updated_at` to `Utc::now()`. Return `Ok(result.rows_affected() == 1)`. Never perform a SELECT-then-update decision. Roll back and preserve the original error if commit cannot complete, using `RollbackFailed { original, rollback }` added to the existing error enum.

- [ ] **Step 5: Verify the schema without disturbing analytics exports.** Keep the current `tjxy_db` re-exports unchanged because `AiUsageRepository` and its error are already exported. Add schema assertions for `ai_daily_usage`, its columns, unique index, and foreign key to the existing schema contract.

- [ ] **Step 6: Run focused tests and commit.**

  Run: `cargo test -p tjxy-db --test ai_repository_contract daily_quota --locked` and `cargo test -p tjxy-db --test schema_contract --locked`

  Expected: PASS on the default database. Run the same contract with `TJXY_TEST_DATABASE_URL` for PostgreSQL and MySQL in CI before merge; the concurrent success count must remain exactly the configured limit.

  Commit: `git add crates/db/src/migration/m20260803_000051_ai_daily_quota.rs crates/db/src/migration/mod.rs crates/db/src/ai_usage.rs crates/db/tests/ai_repository_contract.rs crates/db/tests/schema_contract.rs && git commit -m "feat: add atomic daily AI usage quota"`

## Task 2: Build in-process AI admission control and startup configuration

**Files:**
- Create: `crates/server/src/ai_admission.rs`
- Modify: `crates/server/src/lib.rs`
- Modify: `crates/server/src/startup.rs`
- Modify: `crates/server/src/main.rs`
- Test: `crates/server/src/ai_admission.rs` (unit tests)
- Test: existing startup/main configuration tests in `crates/server/src/main.rs`

**Interfaces:**
- Produce `pub struct AiAdmissionConfig` with `pub fn new(requests_per_minute: u32, max_user_concurrent_sse: usize, max_global_concurrent_sse: usize, daily_quota: u32) -> Result<Self, AiAdmissionConfigError>` and getters for all four values.
- Produce `pub(crate) struct AiAdmissionController` with `pub(crate) fn new(config: AiAdmissionConfig) -> Self` and `pub(crate) fn try_acquire(&self, user_id: UserId) -> Result<AiAdmissionLease, AiAdmissionError>`.
- Produce `AiAdmissionLease::commit(self) -> AiStreamPermit`; dropping an uncommitted lease removes its minute reservation, while dropping `AiStreamPermit` releases both semaphore permits.
- Produce `AiAdmissionRejection::{MinuteRate { retry_after_seconds }, UserConcurrency, GlobalConcurrency, DailyQuota { retry_after_seconds }}` with `retry_after_seconds() -> u64`; the controller emits the first three and the chat boundary constructs `DailyQuota`.
- Produce `AiAdmissionError::{Rejected(AiAdmissionRejection), Unavailable}`; `Rejected` maps to 429 and poisoned-state `Unavailable` fails closed with 503.
- Keep only `AiAdmissionConfig` public because `StartupOptions` is a public builder API; controller, lease, permit, and rejection types remain `pub(crate)`.

```rust
pub struct AiAdmissionConfig { /* private validated fields */ }

impl AiAdmissionConfig {
    pub fn new(
        requests_per_minute: u32,
        max_user_concurrent_sse: usize,
        max_global_concurrent_sse: usize,
        daily_quota: u32,
    ) -> Result<Self, AiAdmissionConfigError>;
}

impl AiAdmissionController {
    pub(crate) fn try_acquire(
        &self,
        user_id: UserId,
    ) -> Result<AiAdmissionLease, AiAdmissionError>;
}

impl AiAdmissionLease {
    pub(crate) fn commit(self) -> AiStreamPermit;
}
```

- [ ] **Step 1: Write deterministic unit tests.** Add tests for invalid zero values, 10th accepted request and 11th minute rejection, minute-window expiry using an injected `Instant` helper, two streams per user with the third rejected, global cap across users, and release-on-drop for both user and global permits. Assert the rate rejection computes a positive `Retry-After`.

- [ ] **Step 2: Run the unit tests to capture the failure.**

  Run: `cargo test -p tjxy-server --lib ai_admission`

  Expected: FAIL because the module and types are absent.

- [ ] **Step 3: Implement the controller.** Store `Arc<Semaphore>` for global streams and `std::sync::Mutex<HashMap<UserId, Arc<UserAdmissionState>>>` for per-user state; each user state has a short-held `std::sync::Mutex` around `VecDeque<(u64, Instant)>` so `Drop` can synchronously cancel an uncommitted reservation. Prune entries at `now - 60s`, reject without waiting, and calculate the oldest-entry retry delay. Acquire user then global `try_acquire_owned`; if the second acquisition fails, dropping the first releases it. A committed `AiStreamPermit` owns the two `OwnedSemaphorePermit`s and is moved into the SSE stream. Store permit fields and the rate ticket in `Option`s so `commit(mut self)` can `take()` them and mark the lease committed before its `Drop` runs. Use a per-user wrapping ticket ID to remove exactly one uncommitted reservation. Treat a poisoned mutex as an explicit internal admission error rather than silently bypassing limits.

- [ ] **Step 4: Wire startup configuration.** Add `ai_admission: AiAdmissionConfig` to `StartupOptions` with the four defaults. Add `with_ai_admission_config`, include non-sensitive numeric values in `Debug`, and change startup initialization to call `AppState::with_ai_config(database, cipher, options.ai_admission)`. Keep `with_ai(database, cipher)` as a compatibility helper that uses defaults.

- [ ] **Step 5: Parse environment overrides with bounded validation.** Add `TJXY_AI_REQUESTS_PER_MINUTE`, `TJXY_AI_MAX_CONCURRENT_STREAMS_PER_USER`, `TJXY_AI_MAX_CONCURRENT_STREAMS`, and `TJXY_AI_DAILY_QUOTA`. Parse positive integers, reject zero/non-numeric values and values above 1,000 requests/minute, 100 user streams, 1,000 global streams, or 100,000 daily requests with a dedicated `StartupError`, and feed the result through `AiAdmissionConfig::new`. Follow the existing testable environment-lookup helper pattern and never log secret values.

- [ ] **Step 6: Run tests and commit.**

  Run: `cargo test -p tjxy-server --lib ai_admission --locked` and `cargo test -p tjxy-server --bin tjxy-server --locked`

  Expected: PASS, with defaults preserved when variables are absent and explicit startup errors for invalid values.

  Commit: `git add crates/server/src/ai_admission.rs crates/server/src/lib.rs crates/server/src/startup.rs crates/server/src/main.rs && git commit -m "feat: add configurable AI admission controller"`

## Task 3: Enforce admission and daily quota at the chat boundary

**Files:**
- Modify: `crates/server/src/ai.rs:367-470,1514-1527`
- Modify: `crates/server/src/lib.rs` (add the configured AppState builder used by startup and integration tests)
- Test: `crates/server/tests/ai_routes.rs`

**Interfaces:**
- `AiService` owns `Arc<AiAdmissionController>` and receives it through `AiService::new_with_config` (the existing `new` delegates to defaults).
- `chat` obtains `AiAdmissionLease` after authentication, payload validation, provider preparation, and conversation authorization, but before constructing `ChatStreamRequest` or an SSE response.
- `ChatStreamRequest` carries the committed `AiStreamPermit`; `agent_stream_response` moves it into the `async_stream!` body so normal completion, provider errors, and body cancellation all drop it.

- [ ] **Step 1: Add failing route tests.** Extend `ConfiguredApp` to pass a small `AiAdmissionConfig`. Block scripted completions with `Notify` and track current/max upstream activity. For one user, launch four body-consuming chats with a per-user cap of 2 and assert exactly two streams enter the provider and two responses are 429. For distinct users, launch 16 chats with a global cap of 8 and assert exactly eight enter the provider, eight are 429, and observed provider concurrency never exceeds 8. Assert rejected requests do not increase the provider hit counter. Drop one held response body, then make the provider return an error for another accepted stream; after each case assert a replacement request succeeds, covering disconnect and error permit release. Add a minute-rate test that asserts `Retry-After` and `Cache-Control: no-store`. Add a daily-quota test with quota 1 that asserts the second request is 429, no provider call is made, and a fixed-time `daily_retry_after_seconds` unit test reaches the next UTC midnight exactly.

- [ ] **Step 2: Run focused tests to capture the failure.**

  Run: `cargo test -p tjxy-server --test ai_routes admission --locked`

  Expected: FAIL because chat currently opens a stream without admission or quota checks.

- [ ] **Step 3: Add admission and quota checks.** After the existing `prepare_chat` and conversation checks, call `service.admission.try_acquire(user_id)`. On success call `AiUsageRepository::new(&service.database).try_consume_daily_quota(user_id, Utc::now().date_naive(), service.admission.config().daily_quota())`. If the durable quota returns `false`, construct `AiAdmissionRejection::DailyQuota` with seconds until the next UTC midnight and drop the uncommitted lease. If the database returns an error, return `503` with `Cache-Control: no-store`. Commit the lease only after quota success. A request rejected by either local admission or daily quota must not create an `ai_execution_records` analytics row; analytics begins only after the quota is consumed and the request becomes an accepted chat attempt.

- [ ] **Step 4: Map admission errors without weakening fail-closed behavior.** Add `admission_error_response(error: AiAdmissionError) -> Response`. For `Rejected(rejection)`, construct `HeaderValue::from_str(&rejection.retry_after_seconds().max(1).to_string())`, set `Retry-After` and `Cache-Control: no-store`, and return status 429. Use 1 second for concurrency, the sliding-window delay for minute rate, and seconds until the next UTC midnight for daily quota. Map `Unavailable` to 503 with `Cache-Control: no-store`; keep existing 400/502/503 mappings unchanged.

- [ ] **Step 5: Ensure RAII release covers cancellation.** Destructure the permit in `agent_stream_response`, bind it inside the `async_stream!` generator before the first await, and do not store it in a temporary that is dropped before the body is returned. Add a code comment explaining that the permit's lifetime intentionally follows the SSE body.

- [ ] **Step 6: Run route tests and commit.**

  Run: `cargo test -p tjxy-server --test ai_routes admission --locked` and `cargo test -p tjxy-server --test ai_routes --locked`

  Expected: PASS; rejected requests must not reach the upstream and dropping a body must release the concurrency permits.

  Commit: `git add crates/server/src/ai.rs crates/server/tests/ai_routes.rs && git commit -m "fix: enforce AI chat rate limits and daily quota"`

## Task 4: Add DNS-rebinding-safe provider transport

**Files:**
- Create: `crates/server/src/ai_provider.rs`
- Modify: `crates/server/src/ai.rs`
- Modify: `crates/server/src/lib.rs`
- Test: `crates/server/src/ai_provider.rs` (address-policy unit tests)
- Test: `crates/server/tests/ai_routes.rs`, `crates/server/tests/ai_settings_routes.rs`

**Interfaces:**
- Produce `AiProviderTransport::open(&Url) -> Result<Arc<dyn AiProviderSession>, AiProviderTransportError>` and `AiProviderSession::request(...) -> Result<ProviderResponse, AiProviderTransportError>`.
- Produce `pub enum ProviderMethod { Get, Post }`, `pub struct ProviderResponse { pub status: StatusCode, pub body: Value }`, and a transport error that distinguishes invalid URL, DNS resolution rejection, connection failure, response-too-large, and invalid JSON.
- Produce `SafeReqwestTransport::new()` and a resolver/policy seam that tests can inject to supply deterministic DNS answer sets.
- `AiService` stores `Arc<dyn AiProviderTransport>`; `AppState::with_ai_transport(...)` is an explicit test/integration constructor while normal startup uses `SafeReqwestTransport`.

```rust
#[async_trait]
pub trait AiProviderTransport: Send + Sync {
    async fn open(
        &self,
        base_url: &Url,
    ) -> Result<Arc<dyn AiProviderSession>, AiProviderTransportError>;
}

#[async_trait]
pub trait AiProviderSession: Send + Sync {
    async fn request(
        &self,
        method: ProviderMethod,
        endpoint: Url,
        api_key: &str,
        body: Option<Value>,
    ) -> Result<ProviderResponse, AiProviderTransportError>;
}

#[async_trait]
pub trait ProviderDnsResolver: Send + Sync {
    async fn resolve(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, AiProviderTransportError>;
}
```

- [ ] **Step 1: Write address-policy tests.** Test rejection of `127.0.0.1`, `::1`, `169.254.169.254`, `10.0.0.1`, `172.16.0.1`, `192.168.1.1`, `fc00::1`, `fe80::1`, `0.0.0.0`, multicast, documentation, benchmark, and reserved ranges. Test IPv4-mapped IPv6 forms such as `::ffff:10.0.0.1`. Test that a DNS answer set containing one public and one private address is rejected as a whole, an empty answer set is rejected, and an all-public set is accepted.

- [ ] **Step 2: Run the policy tests to capture the failure.**

  Run: `cargo test -p tjxy-server --lib ai_provider`

  Expected: FAIL because the transport module and policy are absent.

- [ ] **Step 3: Implement resolution and validation.** `AiProviderTransport::open` parses the URL and resolves `(host, effective_port)` with `tokio::net::lookup_host` through `ProviderDnsResolver`. Reject an empty set or any unsafe address. Implement prefix checks without adding a dependency: reject IPv4 `0/8`, `10/8`, `100.64/10`, `127/8`, `169.254/16`, `172.16/12`, `192.0.0/24`, `192.0.2/24`, `192.168/16`, `198.18/15`, `198.51.100/24`, `203.0.113/24`, `224/4`, and `240/4`. For IPv6, accept only global-unicast `2000::/3`, then reject special/documentation transition ranges `2001::/23`, `2001:db8::/32`, `2002::/16`, and `3fff::/20`; this inherently rejects unspecified, loopback, NAT64/local translation, unique-local, link-local, and multicast space. Classify every IPv4-mapped IPv6 address by its mapped IPv4 value before the IPv6 rule. Validate literal IPs with the same policy and do not make a loopback exception. Return explicit `DnsResolutionRejected` errors rather than silently falling back to the system client.

- [ ] **Step 4: Pin the validated addresses without changing Host/SNI.** `open` builds a fresh Reqwest client with the existing 5-second connect timeout, 30-second request timeout, redirect policy `none`, `.no_proxy()`, and `.resolve_to_addrs(host, &validated_addrs)`, then returns an `AiProviderSession` that owns that client and the approved origin. `request` rejects an endpoint whose scheme/host/effective port differs from the approved origin. Pass the original hostname in every URL so Reqwest retains HTTP Host and TLS SNI/certificate verification; only the TCP destination is replaced. Open one session at the start of `run_agent` and reuse it for all tool/completion rounds; open a new session for configuration test, model discovery, and every new chat/admin operation.

- [ ] **Step 5: Preserve response limits and transport behavior.** Move the current bounded response reader into the transport implementation, reject bodies over `MAX_PROVIDER_RESPONSE_BYTES`, decode JSON once, and return status/body to `AiService`. Keep redirects disabled and do not honor `HTTP_PROXY`/`HTTPS_PROXY` because `.no_proxy()` is mandatory.

- [ ] **Step 6: Inject fakes and remove loopback fixtures.** Replace loopback upstream servers in `ai_routes.rs` and `ai_settings_routes.rs` with a fake `AiProviderTransport` that returns scripted model/completion JSON. Add a resolver fake that returns a public address on the first lookup and a private address on the second; call `open` twice and assert the second operation is rejected before any connection. Verify `.no_proxy()` in an isolated child-process integration test: the parent starts a trap proxy, launches the focused child test with `HTTP_PROXY`/`HTTPS_PROXY` pointing to the trap, and the child opens a session using a resolver-pinned public test endpoint; assert the trap receives zero requests.

- [ ] **Step 7: Integrate all provider calls and update documentation.** Route `test_configuration`, `discover_models`, and completion requests through an opened session. Remove the `normalize_base_url("http://127.0.0.1:...")` success expectation. Update `README.md` around the provider URL section to state that every hostname is resolved immediately before connection, all answers are validated and pinned, system proxies are ignored, and loopback/private provider addresses are unsupported. In the same README section, document the admission defaults, the four environment variables, and the 429 headers.

- [ ] **Step 8: Run focused tests and commit.**

  Run: `cargo test -p tjxy-server --lib ai_provider --locked`, `cargo test -p tjxy-server --test ai_routes --locked`, `cargo test -p tjxy-server --test ai_settings_routes --locked`, and `cargo clippy -p tjxy-server --all-targets --locked -- -D warnings`

  Expected: PASS; tests for `169.254.169.254`, private ranges, malicious hostnames, mixed DNS answers, rebinding, and proxy bypass all reject before provider I/O.

  Commit: `git add crates/server/src/ai_provider.rs crates/server/src/ai.rs crates/server/src/lib.rs crates/server/tests/ai_routes.rs crates/server/tests/ai_settings_routes.rs README.md && git commit -m "fix: pin and validate AI provider DNS addresses"`

## Task 5: Centralize and enforce catalog visibility

**Files:**
- Create: `crates/db/src/catalog_visibility.rs`
- Modify: `crates/db/src/catalog_query.rs`
- Modify: `crates/db/src/lib.rs`
- Modify: `crates/server/src/ai.rs`
- Modify: `crates/server/src/client_portal.rs`
- Modify: `crates/db/src/dashboard.rs`
- Test: `crates/db/tests/catalog_query_repository_contract.rs`
- Test: `crates/server/tests/ai_routes.rs`
- Test: `crates/server/tests/browse_routes.rs`

**Interfaces:**
- Produce `pub fn catalog_item_visibility_condition(item: &Alias) -> Condition` in `catalog_visibility.rs`.
- The condition must be exactly `is_present = true AND classification_state = 'Matched' AND (enabled direct library membership OR enabled active projected membership)`, with the existing correlated `EXISTS` semantics for direct membership and Structure/Active projection ownership.
- Make `catalog_item_is_visible` reuse the helper; do not replace `visible_item(item_id)`, whose narrower present/matched semantics are intentionally used by browse/lock code.

```rust
pub fn catalog_item_visibility_condition(item: &Alias) -> Condition {
    Condition::all()
        .add(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .add(Expr::col((item.clone(), Alias::new("classification_state"))).eq("Matched"))
        .add(
            Condition::any()
                .add(Expr::exists(enabled_membership_for_item(item)))
                .add(Expr::exists(projected_enabled_membership(item))),
        )
}
```

- [ ] **Step 1: Add DB helper contract tests.** Cover a directly associated item in an enabled library, the same item after disabling the library, the same item after deleting `library_catalog_items`, and a projected Structure/Active item whose owner library is disabled or owner is not present/matched. Assert only the canonical helper result changes and unrelated present/matched checks remain intact.

- [ ] **Step 2: Run the DB contract to capture the failure.**

  Run: `cargo test -p tjxy-db --test catalog_query_repository_contract --locked`

  Expected: FAIL once the test calls the not-yet-exported helper or reveals the current query bypass.

- [ ] **Step 3: Extract the condition without changing semantics.** Move the direct and projected correlated `EXISTS` builders into `catalog_visibility.rs`; expose the condition and have `catalog_item_is_visible` compose it with the item alias. Re-export the helper from `tjxy_db` for dashboard and server query builders.

- [ ] **Step 4: Apply the helper to every required query.** Add the condition with the correct `ci`/item alias to:
  - `crates/server/src/ai.rs` favorites (`favorite_items`), preserving user ownership and ordering.
  - `crates/server/src/client_portal.rs` `user_sessions`, `series_started_before`, `completed_series` (episode and returned series), and `user_genres` (add an inner join to `catalog_items` before applying the condition).
  - `crates/server/src/client_portal.rs` `latest_visible_items`, which feeds local Popular fallback.
  - `crates/db/src/dashboard.rs` `DashboardRepository::top_items`.
  - `/Discover/Popular` and `/Discover/Server/Top` through their `top_items`/latest fallback paths.
  - AI recent/history/insights through the session queries above and dashboard snapshots through `top_items`.
  - `local_tmdb_item_ids`, which maps local IDs into TMDB Popular, so `/Discover/Tmdb/Popular` cannot expose an invisible local item.

- [ ] **Step 5: Add visibility regression fixtures.** In `browse_routes.rs`, seed one visible library with movie/series/episode, favorite data, playback sessions, genres, and a fully played episode. Assert Insights, Popular, Server Top, and dashboard top items contain the item. Disable the library and assert all lists/counts/timelines are empty; re-enable it, delete the library membership, and repeat the assertions. In `ai_routes.rs`, script a `get_favorites` tool call and assert hidden titles/UUIDs are absent from the tool result; exercise recent/insights tool paths with the same hidden item.

- [ ] **Step 6: Run focused tests and commit.**

  Run: `cargo test -p tjxy-db --test catalog_query_repository_contract --locked`; `cargo test -p tjxy-server --test browse_routes insights --locked`; `cargo test -p tjxy-server --test browse_routes discover --locked`; `cargo test -p tjxy-server --test ai_routes --locked`

  Expected: PASS; disabling a library or removing its association removes the item from every listed endpoint, while direct browse behavior covered by existing tests remains unchanged.

  Commit: `git add crates/db/src/catalog_visibility.rs crates/db/src/catalog_query.rs crates/db/src/lib.rs crates/db/src/dashboard.rs crates/server/src/ai.rs crates/server/src/client_portal.rs crates/db/tests/catalog_query_repository_contract.rs crates/server/tests/ai_routes.rs crates/server/tests/browse_routes.rs && git commit -m "fix: enforce catalog visibility across AI and discovery"`

## Task 6: Make system-settings writes atomic CAS operations

**Files:**
- Modify: `crates/db/src/system_settings.rs:196-295`
- Create: `crates/db/tests/system_settings_repository_contract.rs`

**Interfaces:**
- Preserve `SystemSettingsRepository::put(&SystemSettingsInput, Option<i64>) -> Result<SystemSettingsRecord, SystemSettingsRepositoryError>` and `put_locale(&str, Option<i64>) -> Result<SystemSettingsRecord, SystemSettingsRepositoryError>`.
- `None` means create-only when no singleton exists; `Some(expected)` means update-only and must conflict when the row is missing. A successful update increments the revision exactly once.

```rust
async fn put_on(
    transaction: &DatabaseTransaction,
    input: &SystemSettingsInput,
    expected_revision: Option<i64>,
) -> Result<SystemSettingsRecord, SystemSettingsRepositoryError>;

async fn get_on(
    connection: &impl ConnectionTrait,
) -> Result<Option<SystemSettingsRecord>, SystemSettingsRepositoryError>;
```

- [ ] **Step 1: Write repository CAS tests.** Assert create with `None` returns revision 1; a second create with `None` returns `Conflict`; update with `Some(1)` returns revision 2; stale `Some(1)` returns `Conflict`; missing row with `Some(1)` returns `Conflict`; `put_locale` preserves all non-locale fields and increments revision. Add `tokio::join!` with two connections saving different titles at revision 1 and assert exactly one `Ok(revision 2)` and one `Conflict`; the final row must equal the successful writer.

- [ ] **Step 2: Run the contract to capture the failure.**

  Run: `cargo test -p tjxy-db --test system_settings_repository_contract --locked`

  Expected: FAIL on the current read/compare/unconditional-upsert implementation, especially under concurrent writers.

- [ ] **Step 3: Implement transactional create/update.** Import `DatabaseTransaction`, `TransactionTrait`, and `SqlErr`. Begin a transaction. For `expected_revision == None`, insert `id=1` with revision 1 and map a unique-constraint error to `Conflict`; do not upsert. For `Some(expected)`, validate it is positive, compute `next = expected.checked_add(1)` (return `InvalidRevision` on overflow), run `UPDATE system_settings SET ..., revision=next, updated_at=now WHERE id=1 AND revision=expected`, require `rows_affected() == 1`, and return `Conflict` otherwise. Read the updated row through a transaction-aware `get_on(&DatabaseTransaction)` before commit. Commit only after the read succeeds; preserve rollback failures using `RollbackFailed { original, rollback }`.

- [ ] **Step 4: Make locale updates use the same CAS transaction.** Begin one transaction in `put_locale`, load the current row with `get_on`, construct a complete `SystemSettingsInput` (defaults only when creating with `None`), replace locale, and invoke the same transaction-local CAS path. Do not perform a separate preflight `get()` outside the transaction.

- [ ] **Step 5: Run cross-backend focused tests and commit.**

  Run: `cargo test -p tjxy-db --test system_settings_repository_contract --locked`

  Expected: PASS on SQLite. Run the same contract with `TJXY_TEST_DATABASE_URL` for PostgreSQL and MySQL in CI; no `UPDATE ... RETURNING` may be introduced.

  Commit: `git add crates/db/src/system_settings.rs crates/db/tests/system_settings_repository_contract.rs && git commit -m "fix: make system settings revision updates atomic"`

## Task 7: Map setup conflicts to HTTP 409

**Files:**
- Modify: `crates/server/src/system_settings.rs:137-170`
- Test: `crates/server/tests/browse_routes.rs` (system settings routes)

**Interfaces:**
- `put_setup` must map `SystemSettingsRepositoryError::Conflict` to `StatusCode::CONFLICT`, matching the existing `repository_error` mapping used by admin settings routes.
- Validation errors remain 400 and unexpected database/rollback errors remain 500; no wildcard conversion may turn conflicts into 500.

- [ ] **Step 1: Add route tests.** Seed revision 1, send two concurrent `PUT /System/Settings` requests with revision 1 and different titles, and assert the response status multiset is exactly `{200, 409}`. Add the setup language conflict case on `/System/Language` and assert 409 rather than 500.

- [ ] **Step 2: Run the tests to capture the failure.**

  Run: `cargo test -p tjxy-server --test browse_routes system_settings --locked`

  Expected: FAIL because `put_setup` currently maps repository conflicts through its wildcard 500 branch.

- [ ] **Step 3: Use the shared repository error mapping.** Pass every `put_setup` repository error to the existing `repository_error` function, which preserves `InvalidLocale -> 400`, `Conflict -> 409`, and all other errors -> 500. Retain the existing media-root/restart side effects only after a successful CAS write.

- [ ] **Step 4: Run focused tests and commit.**

  Run: `cargo test -p tjxy-server --test browse_routes system_settings --locked`

  Expected: PASS with exactly one successful concurrent writer and one 409 response.

  Commit: `git add crates/server/src/system_settings.rs crates/server/tests/browse_routes.rs && git commit -m "fix: return conflict for setup settings CAS failures"`

## Task 8: Full verification and security review

**Files:**
- Review all files changed by Tasks 1-7.
- Modify nearby comments/docs only when the review finds stale behavior descriptions.

- [ ] **Step 1: Run formatting and static checks.**

  Run: `cargo fmt --all -- --check`

  Expected: PASS with no formatting diff.

  Run: `cargo clippy --workspace --all-targets --locked -- -D warnings`

  Expected: PASS with no warnings promoted to errors.

- [ ] **Step 2: Run the complete test matrix.**

  Run: `cargo test --workspace --locked`

  Expected: PASS, including the new DB contracts, AI route tests, browse/discovery tests, and existing migration/schema tests.

  Run in the PostgreSQL and MySQL CI jobs, where `TJXY_TEST_DATABASE_URL` is injected by the job: `cargo test -p tjxy-db --test ai_repository_contract daily_quota --locked` and `cargo test -p tjxy-db --test system_settings_repository_contract --locked`.

  Expected: both backends preserve the exact quota limit and one-winner CAS behavior.

- [ ] **Step 3: Perform a targeted manual review.** Confirm no provider request path still uses the old shared unpinned `reqwest::Client`; no production code accepts loopback/private provider addresses; no proxy environment variable can redirect a request; all four listed endpoint families apply the canonical visibility predicate; every SSE permit is moved into the body generator; daily quota increments only after local admission succeeds; every 429 has both required headers; system-settings writes contain no unconditional upsert.

- [ ] **Step 4: Record residual risks.** Document that in-process minute/concurrency limits are per process and require a distributed limiter if the deployment becomes multi-instance; the durable daily quota remains globally authoritative. Document that DNS changes are re-evaluated per logical provider operation, while all rounds within one agent request intentionally reuse one pinned address set. Record any backend CI not available in the execution environment instead of weakening the tests.

## Self-review checklist

- [ ] Every P0 requirement has a task, implementation interface, failing test, passing test command, and commit boundary.
- [ ] Placeholder scan is clean: every implementation, test, error mapping, and verification step is concrete.
- [ ] Type names and signatures are consistent across tasks: `AiUsageRepository::try_consume_daily_quota`, `AiAdmissionController::try_acquire`, `AiAdmissionLease::commit`, `AiProviderTransport::open`, `AiProviderSession::request`, `catalog_item_visibility_condition`, and the existing system-settings `put`/`put_locale` APIs.
- [ ] Migration numbering reflects the current repository (`000049` and `000050` already registered; this plan uses `000051`).
- [ ] The plan does not authorize destructive worktree operations and explicitly preserves unrelated dirty changes.
