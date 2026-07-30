# TJXY HeroUI Media Client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a production-connected HeroUI ordinary-user media client with safe direct playback while preserving the existing administrator application.

**Architecture:** One React/Vite artifact contains independent `/admin/*` and `/app/*` route trees that share theme primitives but not authorization or state controllers. Rust adds richer catalog projections and durable, session-scoped playback tickets before the client exposes media URLs to native browser elements.

**Tech Stack:** Rust 2024, Axum, SeaORM/SeaQuery, SQLite/MySQL migrations, React 19.2.7, TypeScript 6.0.3, React Router 7.18.1, HeroUI React/Styles 3.2.2, Tailwind CSS 4.3.3, Vite 8.1.5, Vitest 4.1.10, Testing Library, Playwright 1.61.1.

## Global Constraints

- Follow `docs/superpowers/specs/2026-07-30-client-heroui-media-app-design.md` exactly.
- Write and run a failing behavioral test before each production behavior.
- Use HeroUI v3 compound APIs and `onPress`; do not add a provider or v2 packages.
- Keep ordinary-user auth independent from `ra-core` and `tjxy.admin.*` storage.
- Never put a login session token in an image, video, audio, or subtitle URL.
- Never log or attach raw session tokens, playback tickets, passwords, or Authorization headers.
- Preserve all existing `/admin/*` routes and current backend API authorization semantics.
- Keep stream access direct-play only; unsupported browser formats fail explicitly.
- Stage and commit only files belonging to the current task; preserve the existing dirty E2E work.

## File Map

```text
admin/src/client/
  ClientApp.tsx                 ordinary-user route composition
  api/clientApi.ts              configured JSON/blob HTTP boundary
  api/catalogApi.ts             DTO validation and catalog requests
  api/playbackApi.ts            PlaybackInfo, tickets, subtitles, playstate
  auth/ClientAuthContext.tsx    login/current-user/logout state machine
  auth/clientSession.ts         tjxy.web.* token and device identity
  auth/clientDestination.ts     safe /app return-target validation
  auth/ClientLoginPage.tsx      HeroUI login surface
  layout/ClientShell.tsx        desktop/mobile navigation and account menu
  catalog/HomePage.tsx          home rows
  catalog/LibraryPage.tsx       paginated hierarchical media grid
  catalog/SearchPage.tsx        URL-owned search
  catalog/ItemPage.tsx          metadata and user actions
  playback/PlayerPage.tsx       native direct-play orchestration
  playback/srtToVtt.ts          bounded subtitle conversion
  playback/sourceSelection.ts   browser-compatible source ranking
  ui/MediaImage.tsx             authenticated Blob image lifecycle
  ui/MediaTile.tsx              stable poster/result presentation
  ui/MediaRow.tsx               unframed horizontal media section
crates/db/src/playback_ticket.rs durable hashed ticket repository
crates/application/src/playback_ticket.rs issue/validate/revoke service
crates/api/src/playback.rs       ticket and richer playback wire DTOs
crates/server/src/playback_ticket.rs HTTP ticket routes
```

---

### Task 1: Persist And Validate Playback Tickets

**Files:**
- Create: `crates/db/src/migration/m20260730_000038_playback_tickets.rs`
- Create: `crates/db/src/playback_ticket.rs`
- Modify: `crates/db/src/migration/mod.rs`
- Modify: `crates/db/src/lib.rs`
- Test: `crates/db/tests/schema_contract.rs`
- Test: `crates/db/tests/playback_ticket_repository_contract.rs`

**Interfaces:**
- Consumes: existing `auth_sessions`, `users`, UUID helpers, and SHA-256 token digests.
- Produces: `PlaybackTicketDraft`, `PlaybackTicketGrant`, `PlaybackTicketRepository::issue`, `authorize`, and `revoke`.

- [ ] **Step 1: Write migration tests that fail because `playback_tickets` is absent**

```rust
for column in ["id", "auth_session_id", "user_id", "item_id", "media_source_id",
    "play_session_id", "token_digest", "expires_at", "revoked_at", "created_at"] {
    assert!(schema.has_column("playback_tickets", column).await.unwrap());
}
```

- [ ] **Step 2: Run the focused schema tests and confirm the missing-table failure**

Run: `cargo test -p tjxy-db --test schema_contract playback_ticket --locked`

- [ ] **Step 3: Add migration 38 with portable UUID/binary/date columns, foreign keys, unique digest, session-state lookup index, and reversible down migration**

```rust
manager.create_table(
    Table::create().table(Alias::new("playback_tickets")).if_not_exists()
        .col(uuid(Alias::new("id")).primary_key())
        .col(uuid(Alias::new("auth_session_id")))
        .col(uuid(Alias::new("user_id")))
        .col(uuid(Alias::new("item_id")))
        .col(uuid(Alias::new("media_source_id")))
        .col(uuid(Alias::new("play_session_id")))
        .col(binary(Alias::new("token_digest")))
        .col(timestamp_with_time_zone(Alias::new("expires_at")))
        .col(timestamp_with_time_zone_null(Alias::new("revoked_at")))
        .col(timestamp_with_time_zone(Alias::new("created_at"))).to_owned(),
).await?;
```

- [ ] **Step 4: Write repository tests for issue, active-session authorization, wrong item/source, expiry, revoked ticket, disabled user, revision change, and 32-ticket capacity**

```rust
let grant = repository.authorize(&digest, now, item_id, source_id).await.unwrap().unwrap();
assert_eq!(grant.user_id(), user_id);
assert_eq!(grant.auth_session_id(), session_id);
```

- [ ] **Step 5: Run repository tests and confirm they fail because the repository API is absent**

Run: `cargo test -p tjxy-db --test playback_ticket_repository_contract --locked`

- [ ] **Step 6: Implement transactional issue/authorize/revoke with an active auth-session join and a hard 32-active-ticket session cap**

```rust
pub async fn authorize(
    &self,
    token_digest: &[u8; 32],
    now: DateTime<Utc>,
    item_id: CatalogItemId,
    media_source_id: PresentationKey,
) -> Result<Option<PlaybackTicketGrant>, PlaybackTicketRepositoryError>;
```

- [ ] **Step 7: Run DB contract and rollback suites**

Run: `cargo test -p tjxy-db --test playback_ticket_repository_contract --locked`

Run: `cargo test -p tjxy-db --test schema_contract --locked`

- [ ] **Step 8: Commit the durable ticket boundary**

```bash
git add crates/db/src/migration/m20260730_000038_playback_tickets.rs crates/db/src/migration/mod.rs crates/db/src/playback_ticket.rs crates/db/src/lib.rs crates/db/tests/schema_contract.rs crates/db/tests/playback_ticket_repository_contract.rs
git commit -m "feat: persist scoped playback tickets"
```

### Task 2: Expose Ticket Issue, Validation, Revocation, And Correct Stream MIME

**Files:**
- Create: `crates/application/src/playback_ticket.rs`
- Modify: `crates/application/src/lib.rs`
- Modify: `crates/api/src/playback.rs`
- Create: `crates/server/src/playback_ticket.rs`
- Modify: `crates/server/src/lib.rs`
- Modify: `crates/server/src/stream.rs`
- Modify: `crates/server/src/session.rs`
- Test: `crates/application/tests/playback_ticket_service_contract.rs`
- Test: `crates/server/tests/browse_routes.rs`

**Interfaces:**
- Consumes: Task 1 repository, `AuthenticatedPrincipal`, `CatalogQueryService::playback_sources`, and `MediaReadService`.
- Produces: `POST /Items/{id}/PlaybackTicket`, `DELETE /PlaybackTickets/{id}`, and ticket-authenticated GET/HEAD media streaming.

- [ ] **Step 1: Add failing service tests for login-session requirement, source ownership, six-hour expiry bound, redacted Debug, and revocation ownership**

```rust
let issued = service.issue(&principal, item_id, source_id, play_session_id).await.unwrap();
assert_eq!(format!("{issued:?}"), "IssuedPlaybackTicket([REDACTED])");
assert!(issued.expires_at() <= now + chrono::Duration::hours(6));
```

- [ ] **Step 2: Run the service test and verify the missing-service failure**

Run: `cargo test -p tjxy-application --test playback_ticket_service_contract --locked`

- [ ] **Step 3: Implement random 256-bit ticket generation, digest-only persistence, source visibility verification, and fixed error mapping**

```rust
pub struct IssuedPlaybackTicket {
    id: Uuid,
    secret: Zeroizing<String>,
    expires_at: DateTime<Utc>,
}
```

- [ ] **Step 4: Add failing HTTP tests for issue response, no-store headers, query redaction, cross-item denial, expiry, revoke, logout invalidation, and MP4/audio MIME with Range and HEAD**

```rust
assert_eq!(response.headers()[header::CONTENT_TYPE], "video/mp4");
assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
assert!(!format!("{response:?}").contains(raw_ticket));
```

- [ ] **Step 5: Run focused HTTP tests and confirm ticket routes are 404 and stream MIME is octet-stream**

Run: `cargo test -p tjxy-server --test browse_routes playback_ticket --locked`

Run: `cargo test -p tjxy-server --test browse_routes media_stream_supports_get_head_range_if_range_and_416 --locked -- --exact`

- [ ] **Step 6: Implement strict JSON request validation, ticket response DTO, no-store/referrer headers, ticket query parsing, and stream MIME mapping**

```rust
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct PlaybackTicketRequest {
    pub media_source_id: PresentationKey,
    pub play_session_id: Uuid,
}
```

- [ ] **Step 7: Preserve Authorization as the preferred stream auth path and accept `PlaybackTicket` only when Authorization is absent**

```rust
let access = match authenticated_principal(...) {
    Ok(principal) => MediaAccess::Principal(principal),
    Err(_) if headers.get(header::AUTHORIZATION).is_none() => validate_ticket(...).await?,
    Err(response) => return response,
};
```

- [ ] **Step 8: Run service and server playback contracts**

Run: `cargo test -p tjxy-application --test playback_ticket_service_contract --locked`

Run: `cargo test -p tjxy-server --test browse_routes playback --locked`

- [ ] **Step 9: Commit the safe media transport contract**

```bash
git add crates/application/src/playback_ticket.rs crates/application/src/lib.rs crates/api/src/playback.rs crates/server/src/playback_ticket.rs crates/server/src/lib.rs crates/server/src/stream.rs crates/server/src/session.rs crates/application/tests/playback_ticket_service_contract.rs crates/server/tests/browse_routes.rs
git commit -m "feat: authorize browser direct playback"
```

### Task 3: Enrich Catalog And Search DTOs Without N+1 Reads

**Files:**
- Modify: `crates/db/src/catalog_query.rs`
- Modify: `crates/api/src/browse.rs`
- Modify: `crates/server/src/browse.rs`
- Test: `crates/db/tests/catalog_query_repository_contract.rs`
- Test: `crates/server/tests/browse_routes.rs`

**Interfaces:**
- Consumes: existing catalog metadata, association, source publication, image, and user-data tables.
- Produces: `CatalogPersonRecord`, richer `CatalogItemRecord`, richer `BaseItemDto`, and poster-capable `SearchHint`.

- [ ] **Step 1: Write failing repository tests with two items proving bounded association attachment and distinct metadata/runtime values**

```rust
assert_eq!(page.items()[0].genres(), &["Drama"]);
assert_eq!(page.items()[0].people()[0].name(), "A. Director");
assert_eq!(page.items()[0].runtime_ticks(), Some(36_000_000_000));
```

- [ ] **Step 2: Run the focused query tests and confirm the missing accessors fail compilation**

Run: `cargo test -p tjxy-db --test catalog_query_repository_contract enriched_item --locked`

- [ ] **Step 3: Extend the record and attach original title, genres, studios, ordered people, and effective-source runtime in set-based page queries**

```rust
pub struct CatalogPersonRecord {
    name: String,
    role: Option<String>,
    person_type: String,
}
```

- [ ] **Step 4: Add failing HTTP literal assertions for detail, list, latest, resume, next-up, and search metadata**

```rust
assert_eq!(body["OriginalTitle"], "Original title");
assert_eq!(body["Genres"], json!(["Drama"]));
assert_eq!(search["SearchHints"][0]["PrimaryImageTag"], image_tag);
```

- [ ] **Step 5: Extend wire DTOs with optional/empty authoritative values and reuse the same mapper for all BaseItem projections**

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemPersonDto {
    name: String,
    role: Option<String>,
    #[serde(rename = "Type")]
    person_type: String,
}
```

- [ ] **Step 6: Run DB and server catalog suites**

Run: `cargo test -p tjxy-db --test catalog_query_repository_contract --locked`

Run: `cargo test -p tjxy-server --test browse_routes --locked`

- [ ] **Step 7: Commit the catalog projection**

```bash
git add crates/db/src/catalog_query.rs crates/api/src/browse.rs crates/server/src/browse.rs crates/db/tests/catalog_query_repository_contract.rs crates/server/tests/browse_routes.rs
git commit -m "feat: expose media detail metadata"
```

### Task 4: Mount One Build At `/admin` And `/app`

**Files:**
- Modify: `admin/vite.config.ts`
- Modify: `admin/src/App.tsx`
- Modify: `crates/server/src/admin_assets.rs`
- Modify: `crates/server/tests/admin_assets.rs`
- Test: `admin/src/App.test.tsx`

**Interfaces:**
- Consumes: the existing Admin application and build directory.
- Produces: `/assets/*`, `/admin/*`, `/app/*`, root redirect, and a lazy `ClientApp` boundary.

- [ ] **Step 1: Add failing Rust tests for `/`, `/app`, `/app/login`, shared assets, HTML Accept guarding, and API-like non-HTML 404s**

```rust
assert_eq!(get(&app, "/").await.status(), StatusCode::PERMANENT_REDIRECT);
assert_eq!(html(&app, "/app/items/id").await.status(), StatusCode::OK);
assert_eq!(json(&app, "/app/items/id").await.status(), StatusCode::NOT_FOUND);
```

- [ ] **Step 2: Add a failing App test proving `/admin/users` renders only Admin and `/app/login` renders only Client**

Run: `npm --prefix admin test -- --run src/App.test.tsx`

- [ ] **Step 3: Change Vite base to `/`, serve `/assets`, add exact SPA fallbacks, and lazy-load `ClientApp` without moving `CoreAdmin`**

```tsx
const ClientApp = lazy(() => import('./client/ClientApp'));
<Route path="/app/*" element={<Suspense fallback={<LoadingPage />}><ClientApp /></Suspense>} />
```

- [ ] **Step 4: Run asset, App, type, and production-build checks**

Run: `cargo test -p tjxy-server --test admin_assets --locked`

Run: `npm --prefix admin run typecheck`

Run: `npm --prefix admin run build`

- [ ] **Step 5: Commit the dual application boundary**

```bash
git add admin/vite.config.ts admin/src/App.tsx admin/src/App.test.tsx crates/server/src/admin_assets.rs crates/server/tests/admin_assets.rs
git commit -m "feat: mount the user media application"
```

### Task 5: Build Ordinary-User Session And Login

**Files:**
- Create: `admin/src/client/api/clientApi.ts`
- Create: `admin/src/client/api/clientApi.test.ts`
- Create: `admin/src/client/auth/clientSession.ts`
- Create: `admin/src/client/auth/clientSession.test.ts`
- Create: `admin/src/client/auth/clientDestination.ts`
- Create: `admin/src/client/auth/clientDestination.test.ts`
- Create: `admin/src/client/auth/ClientAuthContext.tsx`
- Create: `admin/src/client/auth/ClientAuthContext.test.tsx`
- Create: `admin/src/client/auth/ClientLoginPage.tsx`
- Create: `admin/src/client/auth/ClientLoginPage.test.tsx`
- Create: `admin/src/client/ClientApp.tsx`

**Interfaces:**
- Consumes: `/Users/AuthenticateByName`, `/Users/Me`, `/Sessions/Logout`, readiness, and Task 4 routing.
- Produces: `useClientAuth`, `requireClientSession`, authenticated fetch helpers, and safe login restoration.

- [ ] **Step 1: Add failing tests for independent storage keys, `TJXY Web` identity, strict JSON/blob responses, 401 cleanup, abort behavior, and secret-safe errors**

```ts
expect(sessionStorage.getItem('tjxy.web.token')).toBe('session-token');
expect(sessionStorage.getItem('tjxy.admin.token')).toBe('admin-token');
expect(request.headers.get('Authorization')).toContain('Client="TJXY Web"');
```

- [ ] **Step 2: Run the API/session tests and verify missing modules fail**

Run: `npm --prefix admin test -- --run src/client/api/clientApi.test.ts src/client/auth/clientSession.test.ts`

- [ ] **Step 3: Implement a configured API boundary with fixed error categories and no credential interpolation**

```ts
export async function clientRequest<T>(path: string, options: ClientRequestOptions = {}): Promise<T>;
export async function clientBlob(path: string, signal?: AbortSignal): Promise<Blob>;
```

- [ ] **Step 4: Add failing auth/login tests for enabled ordinary users, disabled denial, direct-login fallback, deep-link restoration, duplicate submit, password reveal, logout request, and failed logout cleanup policy**

Run: `npm --prefix admin test -- --run src/client/auth`

- [ ] **Step 5: Implement auth context, guard, destination validation, and HeroUI v3 login using compound TextField, Button, Alert, Tooltip, and readiness status**

```tsx
<TextField isRequired name="username">
  <Label>Username</Label>
  <Input autoComplete="username" />
</TextField>
```

- [ ] **Step 6: Run auth tests, typecheck, and lint**

Run: `npm --prefix admin test -- --run src/client/auth src/client/api/clientApi.test.ts`

Run: `npm --prefix admin run typecheck`

Run: `npm --prefix admin run lint`

- [ ] **Step 7: Commit ordinary-user authentication**

```bash
git add admin/src/client
git commit -m "feat: authenticate media client users"
```

### Task 6: Implement Catalog API, Authenticated Images, Shell, Home, Library, Search, And Detail

**Files:**
- Create: `admin/src/client/api/catalogApi.ts`
- Create: `admin/src/client/api/catalogApi.test.ts`
- Create: `admin/src/client/ui/MediaImage.tsx`
- Create: `admin/src/client/ui/MediaImage.test.tsx`
- Create: `admin/src/client/ui/MediaTile.tsx`
- Create: `admin/src/client/ui/MediaRow.tsx`
- Create: `admin/src/client/layout/ClientShell.tsx`
- Create: `admin/src/client/layout/ClientShell.test.tsx`
- Create: `admin/src/client/catalog/HomePage.tsx`
- Create: `admin/src/client/catalog/HomePage.test.tsx`
- Create: `admin/src/client/catalog/LibraryPage.tsx`
- Create: `admin/src/client/catalog/LibraryPage.test.tsx`
- Create: `admin/src/client/catalog/SearchPage.tsx`
- Create: `admin/src/client/catalog/SearchPage.test.tsx`
- Create: `admin/src/client/catalog/ItemPage.tsx`
- Create: `admin/src/client/catalog/ItemPage.test.tsx`
- Modify: `admin/src/client/ClientApp.tsx`
- Modify: `admin/vite.config.ts`

**Interfaces:**
- Consumes: Tasks 3 and 5 DTO/auth boundaries.
- Produces: validated catalog models and every non-player user route.

- [ ] **Step 1: Write failing literal-validator tests for malformed wrappers, items, people, image tags, user data, libraries, latest arrays, and enriched search hints**

```ts
expect(parseItem({ ...validItem, Genres: [7] })).toEqual({ ok: false, category: 'invalid-response' });
expect(parsePage({ Items: [validItem], StartIndex: 0, TotalRecordCount: 1 }).items).toHaveLength(1);
```

- [ ] **Step 2: Implement strict validators and request builders with stable page sizes and encoded query parameters**

- [ ] **Step 3: Write failing image tests for authenticated Blob loading, lazy behavior, abort, replacement revoke, unmount revoke, and fallback alt text**

```ts
expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:poster-1');
expect(screen.getByRole('img', { name: 'Poster for Arrival' })).toHaveAttribute('src', 'blob:poster-2');
```

- [ ] **Step 4: Implement `MediaImage`, fixed 2:3 `MediaTile`, and unframed `MediaRow` without nested cards**

- [ ] **Step 5: Write failing shell/page tests for desktop/mobile navigation, focus return, empty row omission, retained-data refresh, pagination, hierarchy, debounced URL search cancelation, metadata omission, favorite, and played actions**

Run: `npm --prefix admin test -- --run src/client/catalog src/client/layout src/client/ui`

- [ ] **Step 6: Implement the HeroUI shell and catalog pages with URL-owned navigation and real server data**

```tsx
<SearchField value={query} onChange={setQuery}>
  <SearchField.Group>
    <SearchField.SearchIcon />
    <SearchField.Input placeholder="Search movies, series, and episodes" />
    <SearchField.ClearButton />
  </SearchField.Group>
</SearchField>
```

- [ ] **Step 7: Add all frontend API proxy prefixes and run focused tests, typecheck, lint, and build**

Run: `npm --prefix admin test -- --run src/client`

Run: `npm --prefix admin run typecheck`

Run: `npm --prefix admin run lint`

Run: `npm --prefix admin run build`

- [ ] **Step 8: Commit the browse experience**

```bash
git add admin/src/client admin/vite.config.ts
git commit -m "feat: add the HeroUI media catalog"
```

### Task 7: Implement Direct Player, SRT Conversion, Resume, And Progress

**Files:**
- Create: `admin/src/client/api/playbackApi.ts`
- Create: `admin/src/client/api/playbackApi.test.ts`
- Create: `admin/src/client/playback/srtToVtt.ts`
- Create: `admin/src/client/playback/srtToVtt.test.ts`
- Create: `admin/src/client/playback/sourceSelection.ts`
- Create: `admin/src/client/playback/sourceSelection.test.ts`
- Create: `admin/src/client/playback/PlayerPage.tsx`
- Create: `admin/src/client/playback/PlayerPage.test.tsx`
- Modify: `admin/src/client/ClientApp.tsx`

**Interfaces:**
- Consumes: PlaybackInfo, Task 2 ticket routes, Task 5 auth, and Task 6 item models.
- Produces: browser-compatible source selection, VTT Blob subtitles, native player orchestration, and bounded playstate reporting.

- [ ] **Step 1: Write failing source-selection tests for MP4/WebM/video, MP3/M4A/Ogg audio, incompatible MKV, unsupported codecs, and deterministic source ordering**

```ts
expect(selectBrowserSource([mkvSource, mp4Source])?.id).toBe(mp4Source.id);
expect(selectBrowserSource([mkvSource])).toBeNull();
```

- [ ] **Step 2: Implement pure source selection and strict PlaybackInfo/ticket validators**

- [ ] **Step 3: Write failing SRT tests for CRLF/LF timestamps, cue numbers, multiline text, malformed timestamps, size limit, and HTML-preserving text**

```ts
expect(srtToVtt('1\n00:00:01,250 --> 00:00:02,500\nHello')).toBe(
  'WEBVTT\n\n00:00:01.250 --> 00:00:02.500\nHello\n',
);
```

- [ ] **Step 4: Implement bounded SRT-to-VTT conversion with no DOM HTML interpretation**

- [ ] **Step 5: Write failing Player tests for preparation retry, unsupported source, ticket URL use without login token, resume after metadata, Started, 15-second Progress throttle, pause/seek flush, Stopped-before-revoke, ended played state, and object URL cleanup**

Run: `npm --prefix admin test -- --run src/client/playback src/client/api/playbackApi.test.ts`

- [ ] **Step 6: Implement `PlayerPage` around native `<video controls>` and HeroUI source/subtitle menus; keep dimensions stable and avoid custom transport controls**

- [ ] **Step 7: Run player tests, all client tests, typecheck, lint, and build**

Run: `npm --prefix admin test -- --run src/client`

Run: `npm --prefix admin run typecheck`

Run: `npm --prefix admin run lint`

Run: `npm --prefix admin run build`

- [ ] **Step 8: Commit browser playback**

```bash
git add admin/src/client
git commit -m "feat: add secure direct media playback"
```

### Task 8: Add Deterministic Browser Fixtures, Accessibility, Visual, And Real Playback Evidence

**Files:**
- Create: `admin/e2e/clientFixtures.ts`
- Create: `admin/e2e/client-login.spec.ts`
- Create: `admin/e2e/client-catalog.spec.ts`
- Create: `admin/e2e/client-playback.spec.ts`
- Modify: `admin/e2e/accessibility.spec.ts`
- Modify: `admin/e2e/visual.spec.ts`
- Modify: `admin/e2e/support.ts`
- Modify: `admin/playwright.config.ts`
- Modify: `admin/scripts/run-e2e-server.sh`
- Add: `admin/e2e/fixtures/posters/*.jpg`

**Interfaces:**
- Consumes: the completed frontend and backend routes.
- Produces: deterministic fixtures and real-server browser evidence at 1440x900, 768x1024, and 390x844.

- [ ] **Step 1: Add valid bitmap poster fixtures and a fixture router that rejects every unhandled API request with redacted diagnostics**

- [ ] **Step 2: Write login/catalog E2E for deep-link restoration, mobile drawer focus, home rows, library pagination, search race, detail metadata, favorite, played, logout, no overflow, and no console/HTTP errors**

- [ ] **Step 3: Write playback E2E proving ticket URL excludes the login token, native video loads, seek sends Range, VTT captions attach, progress posts, and exit revokes**

- [ ] **Step 4: Add Axe and stable screenshot cases for every client route and wait for route-specific loaded state before assertions**

- [ ] **Step 5: Extend the real E2E server fixture to seed the repository MP4/SRT library without printing credentials or media tickets**

- [ ] **Step 6: Run Chromium client E2E and generate reviewed baselines**

Run: `npm --prefix admin run e2e -- client-login.spec.ts client-catalog.spec.ts client-playback.spec.ts --project=chromium`

Run: `npm --prefix admin run e2e -- visual.spec.ts --project=chromium --update-snapshots`

- [ ] **Step 7: Run accessibility and focused WebKit smoke**

Run: `npm --prefix admin run e2e -- accessibility.spec.ts --project=chromium`

Run: `npm --prefix admin run e2e -- client-login.spec.ts client-catalog.spec.ts --project=webkit`

- [ ] **Step 8: Commit browser evidence**

```bash
git add admin/e2e admin/playwright.config.ts admin/scripts/run-e2e-server.sh
git commit -m "test: cover the media client lifecycle"
```

### Task 9: Documentation, Full Verification, Browser Walkthrough, And Review

**Files:**
- Modify: `README.md`
- Modify: `docs/api-parity.md`
- Modify: `docs/superpowers/specs/2026-07-30-client-heroui-media-app-design.md`
- Modify: `docs/superpowers/plans/2026-07-30-client-heroui-media-app.md`

**Interfaces:**
- Consumes: all completed tasks and their verified behavior.
- Produces: accurate deployment/user documentation and a review-ready branch.

- [ ] **Step 1: Document `/app/`, shared build assets, ordinary-user login, direct-play formats, ticket lifetime/revocation, no-transcoding behavior, and verification commands**

- [ ] **Step 2: Run formatting, Rust workspace tests, frontend unit tests, typecheck, lint, production build, and Playwright list**

Run: `cargo fmt --check`

Run: `cargo test --workspace --locked`

Run: `npm --prefix admin test -- --run`

Run: `npm --prefix admin run typecheck`

Run: `npm --prefix admin run lint`

Run: `npm --prefix admin run build`

Run: `npm --prefix admin run e2e -- --list`

- [ ] **Step 3: Start an isolated production server and inspect login, home, library, search, detail, player, and responsive navigation in the in-app browser**

- [ ] **Step 4: Run a code review against the branch merge-base, fix every validated finding through a failing test, and rerun affected checks**

- [ ] **Step 5: Confirm no raw token/ticket strings appear in source diagnostics, test artifacts, browser URLs outside scoped ticket URLs, or tracked output**

Run: `rg -n "ApiKey=|api_key=|PlaybackTicket=[A-Fa-f0-9]{64}|Authorization:" admin/output admin/playwright-report target 2>/dev/null`

- [ ] **Step 6: Commit documentation and final review fixes**

```bash
git add README.md docs/api-parity.md docs/superpowers/specs/2026-07-30-client-heroui-media-app-design.md docs/superpowers/plans/2026-07-30-client-heroui-media-app.md
git commit -m "docs: describe the HeroUI media client"
```
