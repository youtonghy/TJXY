# TJXY First-Run Setup Implementation Plan

> **For agentic workers:** Execute one task at a time with red-green TDD. Preserve
> unrelated working-tree changes and review every shared interface in the primary
> agent before integration.

**Goal:** Deliver a secure first-run browser setup for native and Docker deployments,
with HeroUI v3 screens, durable configuration, recoverable installation, and formal
SQLite, PostgreSQL, and MySQL support.

**Architecture:** The production executable selects either a database-independent
setup runtime or the existing application runtime from a local installation config.
The setup runtime validates drafts, migrates the selected database, creates the first
administrator, persists system settings, atomically completes local configuration,
and restarts into the ordinary application. A completed installation never falls
back to setup when its database is unavailable.

**Tech Stack:** Rust 1.88, Axum 0.8, SeaORM/SeaORM Migration 1.1.14, serde/TOML,
React 19, TypeScript 6, HeroUI React 3.2.2, React Router 7, Vitest, Testing Library,
Playwright, SQLite, PostgreSQL 17, MySQL 8.4.

**Design:** `docs/superpowers/specs/2026-08-04-first-run-setup-design.md`

## Global Constraints

- Keep setup and application routers mutually exclusive. A database outage must not
  be interpreted as an uninstalled system.
- Configuration precedence is environment variables, then config file, then defaults.
- Never log or return database passwords, administrator passwords, connection URLs
  containing credentials, credential keyrings, SQL, or raw driver errors.
- Setup is available without an installation code only to loopback/private source
  addresses and must not trust forwarding headers by default.
- Use the installed `@heroui/react` v3 compound APIs and the current TJXY theme. Do
  not create a second form/control system or hand-roll accessible primitives.
- SQLite, PostgreSQL, and MySQL are all release-gated supported backends before the
  setup UI labels them supported.
- Setup lists, streams, payloads, connection attempts, uploads, and error bodies must
  be explicitly bounded.
- Follow red-green TDD at the configuration, database, HTTP, frontend API, component,
  startup, and end-to-end seams.

---

### Task 1: Local Installation Configuration Contract

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/server/Cargo.toml`
- Create: `crates/server/src/installation_config.rs`
- Modify: `crates/server/src/lib.rs`
- Create: `crates/server/tests/installation_config.rs`

**Interfaces:**
- Produces `InstallationConfigStore`, `InstallationConfig`, `PendingInstallation`,
  `CompletedInstallation`, `DatabaseConfiguration`, `NetworkConfiguration`, and
  `InstallationState::{Unconfigured, Pending, Completed}`.
- Loads `TJXY_CONFIG_FILE` or the platform-native default; container documentation
  selects `/config/tjxy.toml` through that same override.
- Produces resolved runtime values without exposing secrets through `Debug`.

- [ ] Write failing tests for absent config, valid pending/completed config, unsupported
  format version, invalid UUID/address/backend/TLS mode, environment precedence, and
  secret-redacted debug/error output.
- [ ] Write failing filesystem tests for atomic create/replace, interrupted temporary
  file cleanup, restrictive native permissions, symlink refusal, non-regular targets,
  and a non-writable parent.
- [ ] Run `cargo test -p tjxy-server --test installation_config -- --nocapture` and
  confirm the module/API is missing.
- [ ] Add a focused config-format dependency after checking its current documentation;
  use a proven platform-directory helper rather than duplicating per-OS path rules.
- [ ] Implement strict serde DTOs with `deny_unknown_fields`, bounded strings, explicit
  format version, backend-tagged database fields, generated server UUID, generated
  credential keyring, and `Zeroizing` secret ownership.
- [ ] Implement same-directory temporary write, flush, permission application, atomic
  rename, and parent-directory sync where supported. Never follow a config-file symlink.
- [ ] Keep `Pending` separate from `Completed`; database failure cannot delete or
  downgrade a completed file.
- [ ] Run the focused test plus `cargo clippy -p tjxy-server --tests -- -D warnings`.

### Task 2: Durable Database Installation Marker

**Files:**
- Create: `crates/db/src/installation.rs`
- Create: `crates/db/src/migration/m20260804_000053_installation.rs`
- Modify: `crates/db/src/lib.rs`
- Modify: `crates/db/src/migration/mod.rs`
- Create: `crates/db/tests/installation_repository_contract.rs`
- Modify: `crates/db/tests/schema_contract.rs`

**Interfaces:**
- Produces `InstallationRepository`, `InstallationRecord`, and idempotent operations
  `begin`, `attach_initial_admin`, `complete`, and `find`.
- Stores installation ID, server ID, safe status, initial administrator ID, created/
  completed timestamps, and revision. It stores no password, keyring, or database URL.

- [ ] Write a failing repository contract covering begin, same-ID replay, conflicting
  installation/server IDs, administrator attachment, completion, completed replay,
  stale revision, and invalid transitions.
- [ ] Run `cargo test -p tjxy-db --test installation_repository_contract -- --nocapture`
  and confirm the missing API failure.
- [ ] Add one installation table with a singleton constraint, bounded status, UUID
  uniqueness, administrator foreign key, revision fence, and supporting indexes.
- [ ] Implement every transition transactionally and map uniqueness races to explicit
  conflict errors instead of database text.
- [ ] Add forward/down migration assertions and user-deletion behavior. The initial
  administrator referenced by a completed install must not be silently orphaned.
- [ ] Run `cargo test -p tjxy-db --test installation_repository_contract --test schema_contract`.

### Task 3: Promote MySQL To A Formal Backend

**Files:**
- Modify as required: `crates/db/src/**`
- Modify as required: `crates/application/src/**`
- Modify as required: `crates/import/src/**`
- Modify as required: `crates/server/src/**`
- Modify as required: affected backend-aware contract tests
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`

**Interfaces:**
- Preserves existing repository and service APIs while making their SQL behavior
  portable across SQLite, PostgreSQL, and MySQL.
- Converts `mysql-smoke` into a required full-stack release gate equivalent to the
  PostgreSQL contract job.

- [ ] Start disposable MySQL 8.4 and run the current database smoke command unchanged;
  capture actual failures before changing code.
- [ ] Run application, import, and server test packages against
  `TJXY_TEST_DATABASE_URL=mysql://...`; classify each failure as SQL syntax, type,
  transaction/isolation, identifier, ordering, or fixture behavior.
- [ ] Add the smallest failing regression test for every distinct portability defect.
- [ ] Fix backend-specific SQL inside migration/repository helpers. Do not branch in
  setup handlers or weaken constraints to make tests pass.
- [ ] Run the full MySQL suite until green:
  `cargo test -p tjxy-test-support -p tjxy-db -p tjxy-application -p tjxy-import -p tjxy-server --tests --locked`.
- [ ] Remove `continue-on-error`, run all five packages in `mysql-contracts`, and retain
  pinned MySQL health checks.
- [ ] Update README support language only after the required CI-equivalent suite passes.

### Task 4: Setup Validation And Safe Error Contract

**Files:**
- Create: `crates/server/src/setup/model.rs`
- Create: `crates/server/src/setup/validation.rs`
- Create: `crates/server/src/setup/database.rs`
- Create: `crates/server/src/setup/mod.rs`
- Create: `crates/server/tests/setup_validation.rs`

**Interfaces:**
- Produces strict setup inputs, `SetupErrorCode`, `DatabaseTestResult`,
  `EnvironmentCheck`, and `SetupValidator`.
- Database inputs are structured per backend; connection URLs are assembled only on
  the server and are held in zeroizing secret containers.

- [ ] Write failing tests for SQLite path confinement and file creation, PostgreSQL/
  MySQL defaults, invalid host/port/database/user/TLS values, malformed public URL,
  conflicting listener, timeouts, unavailable database, wrong credentials, and
  sanitized error mappings.
- [ ] Add tests proving a password cannot appear in `Debug`, JSON responses, tracing
  output, or nested driver errors.
- [ ] Run `cargo test -p tjxy-server --test setup_validation -- --nocapture` and confirm
  missing setup interfaces.
- [ ] Implement bounded SeaORM `ConnectOptions`: short connect/acquire timeout, one
  setup connection, statement logging disabled, and explicit close after testing.
- [ ] Query backend/version with backend-appropriate bounded statements and report
  only type, normalized version, and elapsed milliseconds.
- [ ] Reuse existing system-settings, URL, branding-image, and listener validation;
  extract shared helpers only when no equivalent already exists.
- [ ] Run focused tests on SQLite, then repeat them through the PostgreSQL and MySQL
  contract environments.

### Task 5: Installation Coordinator And Crash Recovery

**Files:**
- Create: `crates/server/src/setup/coordinator.rs`
- Modify: `crates/server/src/setup/mod.rs`
- Modify: `crates/server/src/startup.rs`
- Create: `crates/server/tests/setup_coordinator.rs`

**Interfaces:**
- Produces `SetupCoordinator::{complete, recover}`, `SetupProgressStage`, and a bounded
  progress subscription for one active installation.
- Consumes Tasks 1, 2, and 4 plus existing migrator, auth, system-settings, asset, and
  restart services.

- [ ] Write a failing happy-path test that creates a fresh file-backed SQLite fixture,
  migrates it, creates the administrator, stores branding/settings, completes config,
  reconnects through the saved config, and authenticates.
- [ ] Write failure-injection tests before/after pending-config write, connection,
  migration, installation begin, administrator creation, settings write, config
  completion, and restart request.
- [ ] Add tests for duplicate completion, a different database containing TJXY tables,
  an existing unrelated administrator, same-install recovery, wrong recovery password,
  and completed-config database failure.
- [ ] Run `cargo test -p tjxy-server --test setup_coordinator -- --nocapture` and confirm
  missing coordinator behavior.
- [ ] Implement single-flight completion keyed by installation ID. Write pending local
  configuration before target-database mutation; never include administrator password.
- [ ] Run migrations, create/recover the first administrator, persist system settings
  and branding, advance the database marker, then atomically mark local config complete.
- [ ] Recovery must reconnect using pending config and verify the same administrator
  credentials against the target DB before adoption. It must not reset an existing password.
- [ ] Emit safe monotonic stages and retain a terminal safe failure category. Bound
  subscribers and remove abandoned process-local state.
- [ ] Run the focused coordinator suite on all three database backends.

### Task 6: Setup HTTP Router And Startup Mode Selection

**Files:**
- Create: `crates/server/src/setup/http.rs`
- Create: `crates/server/src/setup_assets.rs`
- Modify: `crates/server/src/setup/mod.rs`
- Modify: `crates/server/src/lib.rs`
- Modify: `crates/server/src/main.rs`
- Modify: `crates/server/src/admin_assets.rs`
- Create: `crates/server/tests/setup_routes.rs`
- Create: `crates/server/tests/setup_tcp_smoke.rs`

**Interfaces:**
- Produces the approved `/Setup/Status`, `/Setup/Database/Test`,
  `/Setup/Network/Validate`, `/Setup/Branding/{kind}`, `/Setup/Complete`, and
  `/Setup/Progress` routes plus `/setup/*` SPA fallback.
- Startup chooses one router before binding. Completed config always chooses
  application startup, even when application initialization fails.

- [ ] Write failing route tests for exact JSON, strict unknown-field rejection,
  body/upload bounds, content types, no-store headers, CSRF, same-site cookie,
  private-source enforcement, untrusted forwarding headers, rate/concurrency limits,
  one active completion, SSE stage order, disconnect, and secret-free failures.
- [ ] Write failing router-boundary tests proving setup mode returns 404 for login,
  `/app`, `/admin`, and media APIs, while application mode returns 404 for every
  setup operation and `/setup/*`.
- [ ] Write failing TCP tests for no-config setup boot, completed-config normal boot,
  completed-config DB failure without setup fallback, and configured listener restart.
- [ ] Implement a minimal setup state/router independent of `AppState` and database
  initialization. Use `ConnectInfo<SocketAddr>` as the source authority.
- [ ] Add process-local CSRF sessions with secure randomness, strict cookie attributes,
  bounded expiry/capacity, and constant-time token comparison.
- [ ] Extend static asset routing so setup mode serves only the shared Vite assets,
  branding fallback, `/setup/`, and `/setup/{*path}`.
- [ ] Make startup errors distinguish unconfigured setup availability from installed
  database failure; do not catch initialization errors by entering setup.
- [ ] Run `cargo test -p tjxy-server --test setup_routes --test setup_tcp_smoke` with
  local TCP permission, then `cargo check -p tjxy-server --tests`.

### Task 7: Strict Frontend Setup API And State Machine

**Files:**
- Create: `admin/src/setup/setupTypes.ts`
- Create: `admin/src/setup/setupApi.ts`
- Create: `admin/src/setup/setupApi.test.ts`
- Create: `admin/src/setup/setupMachine.ts`
- Create: `admin/src/setup/setupMachine.test.ts`
- Modify: `admin/src/api/httpClient.ts` only if a reusable anonymous-CSRF transport
  cannot be expressed in the setup API module.

**Interfaces:**
- Produces typed API calls and a reducer/state machine for screens 0 through 7.
- Keeps separate in-memory SQLite/PostgreSQL/MySQL drafts and invalidates a successful
  database test whenever its corresponding draft changes.

- [ ] Write failing parser tests for exact known-good responses and rejection of extra
  keys, invalid enums, unsafe integers, unbounded arrays/strings, invalid URLs, repeated
  or regressing progress stages, and fields that could contain secrets.
- [ ] Write failing state tests for animation skip/reduced-motion bypass, environment
  blockers, forward/back navigation, four-step counting, per-backend draft retention,
  stale database test invalidation, final-submit lock, recoverable failure, reconnect,
  and destination URL selection.
- [ ] Run `npm test -- --run src/setup/setupApi.test.ts src/setup/setupMachine.test.ts`
  and confirm missing modules.
- [ ] Implement strict request serialization/response parsing and same-origin CSRF
  handling without browser persistence of passwords or database credentials.
- [ ] Implement SSE consumption with explicit close/abort, monotonic stage validation,
  bounded reconnect, and readiness polling after restart.
- [ ] Run focused tests, `npm run typecheck`, and `npm run lint` from `admin/`.

### Task 8: HeroUI Setup Shell And Core Forms

**Files:**
- Create: `admin/src/setup/SetupApp.tsx`
- Create: `admin/src/setup/SetupLayout.tsx`
- Create: `admin/src/setup/SetupIntro.tsx`
- Create: `admin/src/setup/SetupWelcome.tsx`
- Create: `admin/src/setup/SetupBrandingStep.tsx`
- Create: `admin/src/setup/SetupNetworkStep.tsx`
- Create: `admin/src/setup/SetupAdminStep.tsx`
- Create: `admin/src/setup/SetupReviewStep.tsx`
- Create: `admin/src/setup/SetupApp.test.tsx`
- Modify: `admin/src/App.tsx`
- Modify: `admin/src/settings/locales/en-US.ts`
- Modify: `admin/src/settings/locales/zh-CN.ts`
- Modify: `admin/src/styles.css` only for setup layout tokens not expressible through
  existing utilities/theme variables.

**Interfaces:**
- Produces `/setup/*` with the approved eight-screen/four-step journey.
- Reuses the current `SystemLocaleProvider`, theme variables, brand asset rules, and
  HeroUI React 3.2.2 components.

- [ ] Before editing components, re-query current HeroUI v3 documentation for every
  compound component used and inspect nearby TJXY page patterns.
- [ ] Write failing component tests for desktop/mobile step navigation, blocking checks,
  locale changes, logo/icon upload and preview, defaults, network validation, password
  confirmation/policy, secret-masked review, keyboard traversal, focus movement after
  navigation, and accessible error announcements.
- [ ] Add animation tests for skip, media failure, reduced motion, and a nonblank static
  fallback; do not block setup on the future animation asset.
- [ ] Run `npm test -- --run src/setup/SetupApp.test.tsx` and confirm route/components
  are missing.
- [ ] Implement the shell with a desktop step rail and mobile top progress using stable
  responsive dimensions and one unframed content surface.
- [ ] Use HeroUI `TextField`/`Input`, `Select`, `RadioGroup`, `Checkbox`, `FileTrigger`,
  `Button`, `Alert`, `ProgressBar`, `Modal`, and `Skeleton`; use Lucide only for icons.
- [ ] Keep one primary action per screen, use `onPress`, prevent nested cards, and keep
  pending/error content from shifting the layout.
- [ ] Run focused tests, typecheck, lint, and build.

### Task 9: HeroUI Database, Progress, And Recovery Screens

**Files:**
- Create: `admin/src/setup/SetupDatabaseStep.tsx`
- Create: `admin/src/setup/SetupProgress.tsx`
- Create: `admin/src/setup/SetupRecovery.tsx`
- Create: `admin/src/setup/SetupDatabaseStep.test.tsx`
- Create: `admin/src/setup/SetupProgress.test.tsx`
- Modify: `admin/src/setup/SetupApp.tsx`

**Interfaces:**
- Completes the database-specific step, installation progress, restart reconnect, and
  interrupted-install recovery UI.

- [ ] Write failing tests for all three backend selectors, SQLite allowed-root behavior,
  backend defaults, password visibility control, TLS selection, advanced URL disclosure,
  connection pending/success/failure, stale-result rejection, retained backend drafts,
  and safe version/latency output.
- [ ] Write failing progress/recovery tests for ordered stages, terminal errors, retry,
  browser reconnect, new-origin handoff, wrong recovery credentials, and no back button
  after installation begins.
- [ ] Run the two focused test files and confirm missing behavior.
- [ ] Implement fields with HeroUI compound controls and explicit descriptions/errors.
  Never place a connection string containing credentials into DOM text, an input default,
  URL history, local storage, or error copy.
- [ ] Implement progress with HeroUI `ProgressBar` and `Alert`; keep known completed stages
  visible and announce only stage changes through a polite live region.
- [ ] Implement recovery as verification/adoption, never as password reset or database
  overwrite.
- [ ] Run all setup frontend tests, typecheck, lint, and build.

### Task 10: Native, Docker, TUI, And Operator Documentation

**Files:**
- Create: `Dockerfile`
- Create: `.dockerignore`
- Create: `compose.yaml`
- Modify: `tjxy-tui/src/lib.rs`
- Modify: `tjxy-tui/src/main.rs` only if required for the existing status/actions UI
- Modify: `tjxy-tui/tests/tjxy_tests.rs`
- Modify: `README.md`
- Modify: `docs/api-parity.md`

**Interfaces:**
- Produces one production image, a persistent `/config` and `/data` layout, and a
  Compose example without embedded real secrets.
- TUI reports setup/pending/application/database-failure modes and reads masked resolved
  configuration with the same precedence as the server.

- [ ] Write failing TUI tests for absent, pending, completed, malformed, and
  environment-overridden config while asserting keyring/password masking.
- [ ] Add a multi-stage Docker build pinned to the repository toolchain and Node engine;
  run as a non-root user and declare persistent config/data volumes.
- [ ] Add a health check that distinguishes setup-ready from application-ready without
  leaking installation details. Do not bake credentials into image layers or Compose.
- [ ] Document native first run, Docker Compose first run, config paths/permissions,
  environment precedence, private-network limitation, SQLite/PostgreSQL/MySQL fields,
  changed-listener redirect, crash recovery, and installed database-failure recovery.
- [ ] Document every setup endpoint, safe error category, SSE stage, no-store behavior,
  and permanent disappearance after completion.
- [ ] Run `cargo test -p tjxy-tui` and build the container from a clean context.

### Task 11: Full Verification, Visual QA, And Quality Review

**Files:**
- Create: `admin/e2e/setup.spec.ts`
- Modify: `admin/e2e/support.ts`
- Modify: `admin/e2e/visual.spec.ts`
- Modify: `admin/e2e/accessibility.spec.ts`
- Modify: `admin/e2e/secret-safety.spec.ts`
- Modify: `.github/workflows/ci.yml` if setup-specific service orchestration is needed

- [ ] Add isolated native-style E2E fixtures for clean config, pending recovery, and
  completed application boot. Never point destructive setup tests at a user database.
- [ ] Add a container-style journey using mounted temporary config/data directories.
- [ ] Cover SQLite in the standard frontend job and add targeted PostgreSQL/MySQL setup
  completion to their required backend jobs.
- [ ] Verify desktop 1440x900, tablet 768x1024, and mobile 390x844 in light/dark themes:
  all eight screens, loading/error/success states, nonblank intro fallback, keyboard
  focus, no overlap, no clipped text, and no horizontal document overflow.
- [ ] Verify setup-mode route isolation, application-mode setup removal, private-source
  enforcement, CSRF, rate limits, image bounds, config permissions, log redaction, and
  browser history/storage secret safety.
- [ ] Run Rust gates:
  `cargo fmt --all -- --check`;
  `cargo clippy --workspace --all-targets --locked -- -D warnings`;
  `cargo test --workspace --locked`;
  full PostgreSQL 17 and MySQL 8.4 contract commands.
- [ ] Run frontend gates from `admin/`:
  `npm run lint`;
  `npm run typecheck`;
  `npm test -- --run`;
  `npm run build`;
  `npm run e2e`.
- [ ] Build and run the production container with fresh mounted directories and complete
  one real browser installation without replacing the user's current port-8096 service.
- [ ] Review the final diff for setup fallback escalation, secret leakage, symlink/path
  traversal, SSRF/network-scan amplification, unbounded work, non-idempotent recovery,
  database portability, partial-file durability, environment-precedence ambiguity,
  unsafe proxy trust, and accessibility regressions.

## Delivery Order And Gates

1. Tasks 1 and 2 establish the two durable state authorities.
2. Task 3 must make MySQL a required green backend before any UI calls it supported.
3. Tasks 4 and 5 build validation and completion without HTTP or browser coupling.
4. Task 6 exposes the mutually exclusive runtime boundary.
5. Tasks 7 through 9 build the strict frontend contract and HeroUI experience.
6. Task 10 packages and documents both deployment modes.
7. Task 11 is the release gate; setup is incomplete until all three databases and the
   production browser journeys are green.

Parallel work is safe only for read-only MySQL failure classification, HeroUI test
inventory, and container documentation research. Shared config, migration, startup,
router, and public API changes remain serial and are integrated by the primary agent.
