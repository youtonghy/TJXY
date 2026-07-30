# TMDB Rich Demo Catalog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend TJXY's real catalog and HeroUI client with rich movie, series, season, and episode metadata, then import one idempotent TMDB-backed demonstration catalog into the current development database without playable media.

**Architecture:** Add normalized query-facing catalog fields and associations while retaining bounded provider snapshots for traceability. A private TMDB catalog client produces validated import records, a transactional repository publishes deterministic catalog identities, existing Jellyfin-compatible browse routes expose bounded list and rich detail projections, and the ordinary-user HeroUI routes render the same data. The one-off command reads the encrypted database TMDB setting and never creates `MediaSource` or `MediaLocation` rows.

**Tech Stack:** Rust 2024, SeaORM/SeaQuery migrations, SQLite/PostgreSQL/MySQL contracts, Reqwest with rustls, Serde, Axum, React 19, TypeScript, HeroUI v3, Tailwind CSS v4, Vitest, Playwright.

## Global Constraints

- Work only in `.worktrees/admin-heroui-rebuild`; preserve unrelated dirty files.
- Use red-green-refactor for every production behavior and record the focused failing and passing commands.
- Keep TMDB wire DTOs private to `tjxy-metadata`; public catalog DTOs remain provider-neutral.
- Store display metadata in normalized fields and associations; never query `MetadataSnapshot` from browse routes.
- Use deterministic UUID v5 identities for every imported library, item, person, association, and snapshot.
- Import exactly 12 fixed Movie IDs and 6 fixed Series IDs from the approved design.
- Create no valid media source, media location, placeholder video, or fake stream URL.
- Bound root credits to 24 cast and 12 crew; bound Episode cast to 12 plus director/writer credits.
- Never log or serialize the TMDB access token, authorization headers, raw upstream errors, or decrypted settings.
- Use HeroUI v3 compound components and `onPress`; no HeroUI provider or v2 APIs.

---

### Task 1: Rich Catalog Schema

**Files:**
- Create: `crates/db/src/migration/m20260730_000040_rich_catalog_metadata.rs`
- Modify: `crates/db/src/migration/mod.rs`
- Modify: `crates/db/tests/browse_schema_contract.rs`
- Modify: `crates/db/tests/schema_contract.rs`

**Interfaces:**
- Produces nullable `catalog_items` columns `tagline`, `community_rating`, `vote_count`, `runtime_ticks`, `premiere_date`, `end_date`, `release_status`, `official_rating`, `original_language`, and `index_number`.
- Produces normalized `countries`, `item_countries`, `languages`, `item_languages`, `person_provider_ids`, `person_assets`, and `metadata_snapshots`.
- Preserves existing table and index names and supports complete `down`.

- [x] **Step 1: Write the failing migration contract**

Add a test that migrates an empty SQLite database, asserts every new column/table/index,
and verifies checks reject `community_rating < 0`, `community_rating > 10`,
`vote_count < 0`, `runtime_ticks < 0`, and `index_number < 0`.

```rust
assert!(schema.has_column("catalog_items", "community_rating").await);
assert!(schema.has_table("metadata_snapshots").await);
assert!(schema.has_index("item_people", "idx_item_people_order").await);
```

- [x] **Step 2: Run the focused test and verify RED**

Run: `cargo test -p tjxy-db --test browse_schema_contract rich_catalog --locked`

Expected: FAIL because the new column and tables do not exist.

- [x] **Step 3: Implement the migration**

Use one migration with explicit `Table::alter` statements and normalized foreign keys.
`metadata_snapshots` stores `provider`, `entity_kind`, `provider_entity_id`, `language`,
`fetched_at`, and bounded JSON `payload`. Add ordered browse indexes on
`(parent_id, index_number, sort_key, id)` and `item_people(catalog_item_id, sort_order, id)`.

- [x] **Step 4: Verify GREEN and rollback**

Run:

```text
cargo test -p tjxy-db --test browse_schema_contract --locked
cargo test -p tjxy-db --test schema_contract --locked
```

Expected: PASS, including migration down/up replay.

- [x] **Step 5: Commit the schema slice**

```text
git add crates/db/src/migration crates/db/tests/browse_schema_contract.rs crates/db/tests/schema_contract.rs
git commit -m "feat(db): add rich catalog metadata schema"
```

### Task 2: Validated TMDB Catalog Transport

**Files:**
- Create: `crates/metadata/src/tmdb_catalog.rs`
- Modify: `crates/metadata/src/lib.rs`
- Create: `crates/metadata/tests/tmdb_catalog_contract.rs`

**Interfaces:**
- Produces `TmdbCatalogClient::movie(id) -> Result<RichCatalogItem, MetadataProviderError>`.
- Produces `TmdbCatalogClient::series(id) -> Result<RichSeries, MetadataProviderError>`.
- Produces provider-neutral `RichCatalogItem`, `RichSeries`, `RichSeason`,
  `RichEpisode`, `RichCredit`, and `RemoteImage` values.
- Consumes one bearer token and language as zeroized constructor inputs.

- [x] **Step 1: Write complete wire fixtures and a failing parsing test**

Use local JSON fixtures that include the complete documented response shape for details,
credits, images, classifications, external IDs, Seasons, and Episodes. Assert hand-derived
values such as `community_rating == 8.1`, `runtime_ticks == 6_960_000_000`,
Season index `1`, Episode index `2`, and provider-ordered credits.

- [x] **Step 2: Verify RED**

Run: `cargo test -p tjxy-metadata --test tmdb_catalog_contract --locked`

Expected: FAIL because `TmdbCatalogClient` and rich records do not exist.

- [x] **Step 3: Implement private wire DTOs and validation**

Build request paths for:

```text
/3/movie/{id}?append_to_response=credits,release_dates,external_ids,images
/3/tv/{id}?append_to_response=aggregate_credits,content_ratings,external_ids,images
/3/tv/{series_id}/season/{season_number}?append_to_response=credits,images
```

Validate response bytes, association counts, IDs, dates, nonnegative durations, ratings
within `0..=10`, image paths beginning with `/`, and Season/Episode indices. Prefer the
configured language and perform one `en-US` fallback only when localized title or overview
is empty.

- [x] **Step 4: Add retry and redaction contracts**

Add tests proving 429 honors a bounded retry delay, transient 5xx retries stop at the
configured bound, permanent 4xx does not retry, and formatted errors contain neither the
token nor upstream response body.

- [x] **Step 5: Verify GREEN**

Run:

```text
cargo test -p tjxy-metadata --test tmdb_catalog_contract --locked
cargo test -p tjxy-metadata --locked
```

- [x] **Step 6: Commit the transport slice**

```text
git add crates/metadata/src crates/metadata/tests
git commit -m "feat(metadata): fetch rich tmdb catalog records"
```

### Task 3: Transactional Demo Publication And Command

**Files:**
- Create: `crates/db/src/demo_catalog.rs`
- Modify: `crates/db/src/lib.rs`
- Create: `crates/db/tests/demo_catalog_repository_contract.rs`
- Create: `crates/server/src/bin/import_tmdb_demo.rs`
- Modify: `crates/server/Cargo.toml`
- Modify: `README.md`

**Interfaces:**
- Produces `DemoCatalogRepository::publish(DemoCatalogPublication)`.
- Produces `DemoCatalogPublication` with libraries, item hierarchy, credits, normalized
  associations, snapshots, and staged asset publications.
- Produces the one-off binary `import_tmdb_demo`.
- Consumes `MetadataProviderSettingsRepository`, the configured credential keyring,
  `TmdbCatalogClient`, and `AssetWriteService`.

- [x] **Step 1: Write a failing repository contract**

Publish a two-item fixture twice into real SQLite. Assert literal counts remain one
Movie library, one Television library, one Series, one Season, one Episode, and one shared
Person; assert all descendants have Television membership; assert `catalog_state.generation`
increases once per successful publication and no media source rows exist.

- [x] **Step 2: Verify RED**

Run: `cargo test -p tjxy-db --test demo_catalog_repository_contract --locked`

Expected: FAIL because the repository and publication types do not exist.

- [x] **Step 3: Implement deterministic transactional publication**

Derive UUIDs from the namespace plus provider kind and ID. Upsert the two libraries and
items, replace only associations owned by the deterministic demo item set, publish
provider IDs/provenance/snapshots/assets, then increment generation once in the same
transaction. A forced association error must roll back all catalog changes.

- [x] **Step 4: Add and verify rollback behavior**

Inject one invalid parent in the fixture and assert item, membership, association, and
generation counts remain unchanged.

- [x] **Step 5: Write a failing command-manifest test**

Test a pure `demo_manifest()` consumer result: 12 unique Movie IDs, 6 unique Series IDs,
and exact inclusion of the approved IDs. Test missing/disabled/undecryptable TMDB settings
fail before a fake transport records any request.

- [x] **Step 6: Implement the command**

Read `TJXY_DATABASE_URL`, the existing asset root configuration, and the existing
`TJXY_CREDENTIAL_KEYRING` parser. Fetch and validate every manifest record, stage assets,
build one publication, publish it, and print only counts and sanitized warnings.

- [x] **Step 7: Verify GREEN**

Run:

```text
cargo test -p tjxy-db --test demo_catalog_repository_contract --locked
cargo test -p tjxy-server --bin import_tmdb_demo --locked
```

- [ ] **Step 8: Commit the importer slice**

```text
git add crates/db/src/demo_catalog.rs crates/db/src/lib.rs crates/db/tests/demo_catalog_repository_contract.rs crates/server/src/bin/import_tmdb_demo.rs crates/server/Cargo.toml README.md
git commit -m "feat(server): import an idempotent tmdb demo catalog"
```

### Task 4: Rich Browse Query And API Contract

**Files:**
- Modify: `crates/db/src/catalog_query.rs`
- Modify: `crates/db/tests/catalog_query_repository_contract.rs`
- Modify: `crates/application/src/catalog.rs`
- Modify: `crates/application/tests/catalog_query_service_contract.rs`
- Modify: `crates/api/src/browse.rs`
- Modify: `crates/api/tests/browse_golden.rs`
- Modify: `crates/server/src/browse.rs`
- Modify: `crates/server/tests/browse_routes.rs`

**Interfaces:**
- Extends list records with `community_rating` and `index_number`.
- Adds a set-based `CatalogItemDetailRecord` with normalized facts, bounded ordered credits,
  image tags, `has_media_sources`, and UserData.
- Extends `BaseItemDto` with Jellyfin-compatible PascalCase rich detail properties while
  omitting absent optional fields.
- Keeps `GET /Items?ParentId=` as the Season/Episode child contract.

- [ ] **Step 1: Write a failing repository detail and ordering contract**

Seed one Series with Season 0, Season 2, Season 1 and Episode 2, Episode 1. Assert returned
IDs are ordered `0,1,2` and `1,2`. Seed genres, studios, countries, languages, duplicated
Person roles, and 30 cast entries; assert detail returns normalized values and exactly the
bounded ordered credits without N+1 behavior.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p tjxy-db --test catalog_query_repository_contract rich_detail --locked`

- [ ] **Step 3: Implement set-based repository projections**

Keep list/search queries lightweight. Load detail associations in bounded set queries keyed
by one visible item. Compute source availability with `EXISTS`, never by loading source URLs.
Order children with nullable index last and deterministic `sort_key, id` tie-breaking.

- [ ] **Step 4: Write failing API golden and route tests**

Assert literal PascalCase output for rating, vote count, ticks, ISO dates, arrays, credits,
image tags, source availability, and UserData. Assert snapshots are absent. Assert hidden
library items remain 404 and a mismatched `userId` remains 403.

- [ ] **Step 5: Implement DTO and route mapping**

Use separate constructors for list and detail projections so list routes do not accidentally
serialize empty rich arrays. Extend search hints with `PrimaryImageTag`, year, type, and
community rating.

- [ ] **Step 6: Verify GREEN**

Run:

```text
cargo test -p tjxy-db --test catalog_query_repository_contract --locked
cargo test -p tjxy-application --test catalog_query_service_contract --locked
cargo test -p tjxy-api --test browse_golden --locked
cargo test -p tjxy-server --test browse_routes --locked
```

- [ ] **Step 7: Commit the browse slice**

```text
git add crates/db/src/catalog_query.rs crates/db/tests/catalog_query_repository_contract.rs crates/application/src/catalog.rs crates/application/tests/catalog_query_service_contract.rs crates/api/src/browse.rs crates/api/tests/browse_golden.rs crates/server/src/browse.rs crates/server/tests/browse_routes.rs
git commit -m "feat(catalog): expose rich item and episode details"
```

### Task 5: HeroUI Rich Detail And No-Source Player

**Files:**
- Modify: `admin/src/client/api/catalogApi.ts`
- Modify: `admin/src/client/catalog/ItemPage.tsx`
- Create: `admin/src/client/catalog/ItemPage.test.tsx`
- Create: `admin/src/client/catalog/MediaFacts.tsx`
- Create: `admin/src/client/catalog/CreditsSection.tsx`
- Create: `admin/src/client/catalog/EpisodeList.tsx`
- Modify: `admin/src/client/ui/MediaImage.tsx`
- Modify: `admin/src/client/ui/MediaTile.tsx`
- Modify: `admin/src/client/playback/PlayerPage.tsx`
- Create: `admin/src/client/playback/PlayerPage.test.tsx`
- Modify: `admin/src/styles.css`

**Interfaces:**
- `MediaItem` mirrors the rich PascalCase DTO with typed facts, credits, image tags, and
  `HasMediaSources`.
- `getChildren(parentId)` uses the existing paginated items route.
- `ItemPage` renders Movie, Series, Season, and Episode variants from one route.
- `PlayerPage` distinguishes no-source, unsupported, unauthorized, and transient errors.

- [ ] **Step 1: Fetch current HeroUI component documentation**

Use the HeroUI MCP in the required order: list components, then fetch Button, Chip, Tabs,
Avatar, Alert, Skeleton, and any selected list/surface primitive. Use only confirmed v3
compound APIs and semantic tokens.

- [ ] **Step 2: Write failing rich-detail component tests**

Render a complete Series response in a real `MemoryRouter`. Assert accessible labels expose
rating, runtime, content rating, country, language, overview, crew, 24 cast entries, Season
selector, and ordered Episode rows. Add a sparse Movie response and assert absent fields
do not render invented placeholders.

- [ ] **Step 3: Verify RED**

Run: `cd admin && npm test -- --run src/client/catalog/ItemPage.test.tsx`

- [ ] **Step 4: Implement the rich detail page**

Use an authenticated backdrop image with a stable header height, 2:3 poster constraints,
compact facts, semantic chips, unframed overview and crew sections, repeated cast surfaces,
and a responsive Season/Episode section. Preserve breadcrumbs and favorite/played behavior.

- [ ] **Step 5: Write failing no-source player test**

Return `PlaybackInfo` with no sources. Assert the page says `No playable file is attached`,
has a details link, contains no `<video>`, and never requests a playback ticket.

- [ ] **Step 6: Implement player and tile behavior**

Map an empty source list to the expected non-danger unavailable state. Show rating and
episode codes on tiles without changing fixed poster/still dimensions.

- [ ] **Step 7: Verify GREEN**

Run:

```text
cd admin && npm test -- --run src/client/catalog/ItemPage.test.tsx src/client/playback/PlayerPage.test.tsx
cd admin && npm run typecheck
cd admin && npm run lint
cd admin && npm run build
```

- [ ] **Step 8: Commit the HeroUI slice**

```text
git add admin/src/client admin/src/styles.css
git commit -m "feat(client): render rich movie and series details"
```

### Task 6: Real Import And End-To-End Verification

**Files:**
- Modify: `admin/e2e/client-catalog.spec.ts` or the nearest existing ordinary-client spec.
- Modify: `docs/api-parity.md` and `README.md` only for actual behavior/commands introduced.

**Interfaces:**
- Consumes the migrated development database, configured encrypted TMDB setting, and built
  importer.
- Produces a browser-visible demonstration catalog at `/app/`.

- [ ] **Step 1: Run the complete automated gate**

```text
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cd admin && npm test -- --run
cd admin && npm run typecheck
cd admin && npm run lint
cd admin && npm run build
git diff --check
```

- [ ] **Step 2: Run the one-off import**

Run the binary against the same `TJXY_DATABASE_URL`, asset root, and credential keyring as
the preview server. Confirm its sanitized summary reports 12 movies, 6 series, every
fetched Season/Episode, zero media sources, and no failed required assets.

- [ ] **Step 3: Restart the preview on the existing available port**

Restart with the migrated database and rebuilt frontend, preserving the current configured
credential environment. Confirm readiness before browser navigation.

- [ ] **Step 4: Add and run browser journeys**

Cover ordinary-user login, home, Movie library, Television library, search with imagery,
one Movie detail, one Series, Season selection, Episode detail, no-source playback, mobile
layout, keyboard focus, and navigation. Assert no failed API responses other than the
expected no-source playback response and no console errors.

- [ ] **Step 5: Perform visual QA**

Capture desktop `1440x1000`, tablet `820x1180`, and mobile `390x844` screenshots. Check
nonblank artwork pixels, stable poster/still aspect ratios, no overlap or horizontal
overflow, readable backdrop text, visible focus, and correctly localized metadata.

- [ ] **Step 6: Review final diff and document residual risks**

Inspect every touched file, secret-scan logs and generated output, rerun focused failures
if any fix is needed, and report any external TMDB data omissions or pre-existing clippy
warnings separately.
