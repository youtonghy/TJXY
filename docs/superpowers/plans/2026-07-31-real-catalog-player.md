# Real Catalog And Player Fixture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps
> use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Populate two real 100-plus-title development libraries, attach bounded playable
fixtures, add source/subtitle/progress behavior to the player, and render watched/favorite
poster state.

**Architecture:** Keep TMDB metadata, catalog items, media sources, playback tickets,
playstate, and user data as separate authoritative boundaries. A one-off idempotent
development command publishes real filesystem-backed fixtures; the client consumes only
authenticated TJXY APIs and never receives provider paths or credentials.

**Tech Stack:** Rust, Tokio, SeaORM/SeaQuery, SQLite/PostgreSQL/MySQL-compatible repository
queries, Axum, React 19, TypeScript, HeroUI v3, React Router, Vitest, Testing Library,
Playwright, native HTML video, TMDB v3, ffmpeg.

## Global Constraints

- Use fixed source-controlled TMDB IDs: at least 100 Movies and 100 Series.
- Count only top-level Movie and Series items toward the 100-title targets.
- Import every Season and Episode declared by each selected Series.
- Run catalog and media fixture imports only against the current development database.
- Do not add a production startup seed or scheduled synchronization.
- Do not expose `Demo`, `metadata only`, local paths, provider object IDs, or credentials
  in ordinary-user UI or playback DTOs.
- Do not add a database migration; the existing schema already supports the required data.
- Use HeroUI v3 compound components and `onPress`.
- Preserve unrelated dirty worktree changes and do not commit unless the user requests it.

---

### Task 1: Freeze And Publish The 100-Plus TMDB Catalog

**Files:**
- Modify: `crates/metadata/src/tmdb_catalog.rs`
- Modify: `crates/metadata/tests/tmdb_catalog_client_contract.rs`
- Modify: `crates/server/src/bin/import_tmdb_demo.rs`
- Modify: `crates/db/src/demo_catalog.rs`
- Modify: `crates/db/tests/demo_catalog_repository_contract.rs`
- Modify: `docs/superpowers/specs/2026-07-30-tmdb-demo-catalog-design.md`

**Interfaces:**
- Produces: fixed `MOVIE_IDS` and `SERIES_IDS` arrays with at least 100 unique nonzero IDs.
- Produces: `DemoCatalogRepository::publish` updating the existing deterministic library
  identities with user-facing names `Movies` and `TV Shows`.
- Preserves: deterministic `demo_catalog_item_id(kind, provider_id)` identities.

- [ ] **Step 1: Add failing manifest and library-name tests**

  Assert `movie_ids.len() >= 100`, `series_ids.len() >= 100`, no duplicates, no zero IDs,
  and repository-published names exactly equal `Movies` and `TV Shows`.

- [ ] **Step 2: Run focused red tests**

  Run:

  ```bash
  cargo test -p tjxy-server --bin import_tmdb_demo manifest_contains
  cargo test -p tjxy-db --test demo_catalog_repository_contract
  ```

  Expected: the count and public-name assertions fail against the 12/6 manifest and current
  `TMDB Demo ...` names.

- [ ] **Step 3: Add bounded TMDB popular-page ID retrieval for manifest generation**

  Add a private wire DTO containing `page`, `total_pages`, and `results[].id`, validate a
  page range of 1 through 500 and at most 20 unique nonzero IDs, and expose test-only or
  importer-only methods that request `/movie/popular` and `/tv/popular`. Do not store
  popularity data in the catalog.

- [ ] **Step 4: Generate, review, and freeze five or more pages of IDs**

  Use the configured encrypted TMDB credential to print candidate IDs once. Remove
  duplicates, retain the already approved IDs, verify every frozen ID through the existing
  `movie(id)` or `series(id)` detail method, then store the final arrays in source.

- [ ] **Step 5: Rename the public libraries and correct publication policies**

  Keep the existing deterministic library keys, change only names to `Movies` and
  `TV Shows`, and replace non-schema values with the established manual/filesystem policy
  literals accepted by the admin library DTO.

- [ ] **Step 6: Run focused green tests**

  Run the two commands from Step 2 plus:

  ```bash
  cargo test -p tjxy-metadata --test tmdb_catalog_client_contract
  ```

  Expected: all focused tests pass and remote response bytes or credentials are absent from
  failures.

### Task 2: Publish Idempotent Development Media Fixtures

**Files:**
- Create: `crates/db/src/development_media.rs`
- Modify: `crates/db/src/lib.rs`
- Create: `crates/db/tests/development_media_repository_contract.rs`
- Create: `crates/server/src/bin/attach_development_media.rs`
- Modify: `crates/server/Cargo.toml`

**Interfaces:**
- Produces:

  ```rust
  pub struct DevelopmentMediaPublication {
      pub root_path: PathBuf,
      pub items: Vec<DevelopmentPlayableItem>,
  }

  pub struct DevelopmentPlayableItem {
      pub catalog_item_id: CatalogItemId,
      pub variants: Vec<DevelopmentMediaVariant>,
  }

  pub async fn DevelopmentMediaRepository::publish(
      &self,
      publication: &DevelopmentMediaPublication,
  ) -> Result<DevelopmentMediaReport, DevelopmentMediaError>;
  ```

- Consumes: existing `CatalogItem`, source publication, storage account, filesystem
  configuration, storage object, stream, subtitle, and catalog-generation tables.
- Produces: deterministic active Source publications readable by `CatalogQueryService` and
  `MediaReadService`.

- [ ] **Step 1: Write failing repository contracts**

  Seed one Movie and one Episode, publish default plus alternate plus damaged variants,
  publish Chinese and English VTT subtitles, re-run publication, and assert:

  - stable source and presentation identities;
  - no duplicate objects, locations, streams, subtitles, or active publications;
  - only Movie and Episode owners are accepted;
  - the default is ordered first and damaged is never default;
  - active sources resolve only through the configured fixture root;
  - one catalog generation increment per successful publication.

- [ ] **Step 2: Run the repository contract red**

  ```bash
  cargo test -p tjxy-db --test development_media_repository_contract
  ```

  Expected: compilation fails because the development media repository does not exist.

- [ ] **Step 3: Implement bounded fixture validation and deterministic identities**

  Validate an absolute canonical directory, a maximum of 20,000 playable owners, at most
  four variants and eight subtitle rows per owner, unique variant keys, positive sizes for
  valid sources, and exactly one default valid source. Derive UUIDv5 identities from
  `catalog_item_id + variant key + row kind`.

- [ ] **Step 4: Implement one atomic publication**

  Upsert the development filesystem account/configuration, root and root membership;
  replace only deterministic fixture-owned storage/source rows; insert canonical and
  publication rows; set active Source pointers; and advance catalog generation once.
  Roll back on any failure. Do not delete unrelated scanned media sources.

- [ ] **Step 5: Implement the explicit fixture command**

  Require:

  ```text
  TJXY_DATABASE_URL
  TJXY_DEVELOPMENT_MEDIA_DIR
  ```

  Reject a database URL that does not target the explicitly supplied development database.
  Invoke local `ffmpeg` to create short black/silent valid sources, verify output with
  `ffprobe`, create item-specific hard links or bounded copies, write English and Chinese
  VTT files, and create zero-byte damaged files only for 12 deterministic owners.

- [ ] **Step 6: Run repository and command tests**

  ```bash
  cargo test -p tjxy-db --test development_media_repository_contract
  cargo test -p tjxy-server --bin attach_development_media
  ```

  Expected: idempotency, bounds, rollback, and command argument tests pass.

### Task 3: Expand PlaybackInfo Source And Subtitle Metadata

**Files:**
- Modify: `crates/application/src/catalog.rs`
- Modify: `crates/db/src/catalog_query.rs`
- Modify: `crates/api/src/playback.rs`
- Modify: `crates/api/tests/playback_info_golden.rs`
- Modify: `crates/api/tests/golden/playback_info_direct_play.json`
- Modify: `crates/server/src/browse.rs`
- Modify: `crates/server/tests/browse_routes.rs`
- Modify: `crates/server/tests/golden/playback/cloud-multi-source-playback-info.response.json`

**Interfaces:**
- Produces each `MediaSourceInfo` with `Edition`, `Bitrate`, `RunTimeTicks`, `IsDefault`,
  `Container`, `DirectStreamUrl`, and `MediaStreams`.
- Produces each subtitle `MediaStream` with `Language`, `Codec`, `Index`, `IsDefault`,
  `IsForced`, and authenticated local `DeliveryUrl`.
- Preserves source-order policy and current local-route safety validation.

- [ ] **Step 1: Extend golden expectations first**

  Add explicit source fields and subtitle default/forced flags to the API unit golden and
  both filesystem and cloud server response goldens. Assert no local path, account ID,
  provider object ID, or credential marker appears.

- [ ] **Step 2: Run playback golden tests red**

  ```bash
  cargo test -p tjxy-api --test playback_info_golden
  cargo test -p tjxy-server --test browse_routes playback_info
  ```

  Expected: DTO fields are missing.

- [ ] **Step 3: Carry existing persisted values through the read model**

  Add bounded getters and query projection for edition, bitrate, runtime ticks, source
  default, subtitle default, and subtitle forced. Keep nullable values nullable rather than
  inventing labels.

- [ ] **Step 4: Serialize the expanded DTO**

  Map source and subtitle fields in `media_source_info`. Keep embedded Video and Audio
  streams unchanged and external subtitle delivery URLs local.

- [ ] **Step 5: Run focused green tests**

  Run both commands from Step 2 and the cloud multi-source contract. Expected: all explicit
  goldens pass without normalization that reorders MediaSources or MediaStreams.

### Task 4: Build The Multi-Source HeroUI Player

**Files:**
- Modify: `admin/src/client/api/clientApi.ts`
- Modify: `admin/src/client/api/playbackApi.ts`
- Create: `admin/src/client/api/playstateApi.ts`
- Create: `admin/src/client/playback/playbackLabels.ts`
- Create: `admin/src/client/playback/usePlaybackSession.ts`
- Modify: `admin/src/client/playback/sourceSelection.ts`
- Modify: `admin/src/client/playback/sourceSelection.test.ts`
- Modify: `admin/src/client/playback/PlayerPage.tsx`
- Modify: `admin/src/client/playback/PlayerPage.test.tsx`

**Interfaces:**
- Produces `sendPlaybackStarted`, `sendPlaybackProgress`, `sendPlaybackStopped`, and
  `fetchSubtitleBlob`.
- Produces a `usePlaybackSession` hook that owns selected source, ticket, subtitle Blob URL,
  stale-request cancellation, progress throttling, error state, and cleanup.
- Consumes `PlaybackInfo.MediaSources` without reordering the server's policy order.

- [ ] **Step 1: Write failing client tests**

  Cover:

  - the first compatible server-ordered source is recommended;
  - selecting another source revokes the prior ticket and preserves current time;
  - stale ticket responses are revoked and never installed;
  - subtitle selection fetches authenticated VTT, creates one Blob URL, and revokes it on
    replacement or unmount;
  - play emits Started once, periodic Progress while playing, pause/seek Progress, and Exit
    or cleanup Stopped;
  - ended records Stopped and marks the item played without revoking the still-replayable
    ticket;
  - damaged source `error` displays recovery and the recovery action selects recommended.

- [ ] **Step 2: Run frontend player tests red**

  ```bash
  npm test -- --run src/client/playback
  ```

  Expected: source selector, telemetry, subtitle, recovery, and cleanup assertions fail.

- [ ] **Step 3: Implement typed API helpers**

  Expand the PlaybackSource and PlaybackStream interfaces to match Task 3. Add JSON
  playstate requests using seconds-to-ticks conversion. Add an authenticated Blob response
  helper that preserves existing authorization and sanitized error handling.

- [ ] **Step 4: Implement the playback session hook**

  Follow React effect cleanup rules: ignore stale async results, clear intervals, revoke
  tickets and Blob URLs exactly once, and never set state after cleanup. Use the media
  element's real events as the source of telemetry.

- [ ] **Step 5: Implement the HeroUI player surface**

  Use documented HeroUI v3 `Select`, `Button`, and `Alert` compound APIs with `onPress`.
  Keep the video area full-width and unframed. Show source and subtitle selectors in a
  compact toolbar below the video. Keep native video controls.

- [ ] **Step 6: Run player tests green**

  ```bash
  npm test -- --run src/client/playback
  npm run typecheck
  npm run lint
  ```

  Expected: source, subtitle, telemetry, media error, and cleanup tests pass with no lint
  warnings.

### Task 5: Add Watched, Progress, And Favorite Poster Status

**Files:**
- Create: `admin/src/client/ui/MediaStatusOverlay.tsx`
- Create: `admin/src/client/ui/MediaStatusOverlay.test.tsx`
- Modify: `admin/src/client/ui/MediaTile.tsx`
- Modify: `admin/src/client/ui/MediaTile.test.tsx`

**Interfaces:**
- Produces:

  ```tsx
  <MediaStatusOverlay
    favorite={boolean}
    played={boolean}
    positionTicks={number | undefined}
    runtimeTicks={number | undefined}
  />
  ```

- Consumes only returned catalog UserData and RunTimeTicks.

- [ ] **Step 1: Write failing visual-state tests**

  Assert no overlay for untouched items, a green check for played items, a clamped green
  progress ring for positive partial progress, a pink filled heart for favorite items, and
  both badges when favorite and viewing state coexist. Assert accessible labels.

- [ ] **Step 2: Run the overlay tests red**

  ```bash
  npm test -- --run src/client/ui/MediaStatusOverlay.test.tsx src/client/ui/MediaTile.test.tsx
  ```

  Expected: the overlay component and badges are absent.

- [ ] **Step 3: Implement the overlay**

  Use the HeroUI v3 progress-circle semantics or an equivalent accessible CSS conic ring
  verified against official docs. Keep the overlay absolutely positioned inside a newly
  relative poster wrapper and prevent it from changing card dimensions.

- [ ] **Step 4: Run focused and full frontend tests**

  Run Step 2, then:

  ```bash
  npm test -- --run --testTimeout=15000
  npm run typecheck
  npm run lint
  npm run build
  ```

  Expected: all frontend tests and production build pass.

### Task 6: Import And Verify The Current Development Database

**Files:**
- Modify nearby import instructions in `README.md` or the existing development catalog
  documentation only when the command or environment contract changed.

**Interfaces:**
- Consumes the current preview database:
  `/private/tmp/tjxy-admin-preview/tjxy.db`.
- Consumes the current preview assets and explicit development media fixture directories.
- Produces the only requested real catalog and media fixture import.

- [ ] **Step 1: Back up the exact development database**

  Use SQLite's online `.backup` command to a timestamped file in `/private/tmp`. Do not copy
  a live WAL database with a raw filesystem copy.

- [ ] **Step 2: Run the catalog importer once**

  Use the same database URL, assets directory, encrypted TMDB setting, and credential
  keyring as the preview server. Capture the sanitized report counts.

- [ ] **Step 3: Run the media fixture command once**

  Target an absolute fixture directory under `/private/tmp` and the same database URL.
  Restart the preview server afterward so the persisted filesystem backend is registered.

- [ ] **Step 4: Assert database invariants**

  SQL assertions must prove:

  - `Movies` has at least 100 top-level Movie memberships;
  - `TV Shows` has at least 100 top-level Series memberships;
  - no enabled library name contains `Demo`;
  - every Movie and Episode has an active Source publication;
  - exactly 12 playable owners have more than one source;
  - damaged sources are never default;
  - the source DTO and storage rows contain no access token.

- [ ] **Step 5: Run complete automated verification**

  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace --all-targets --all-features
  npm test -- --run --testTimeout=15000
  npm run typecheck
  npm run lint
  npm run build
  git diff --check
  ```

- [ ] **Step 6: Run browser acceptance**

  Use the in-app browser at desktop, tablet, and mobile widths. Verify library counts and
  naming, valid media playback, source switching, Chinese/English/Off subtitles, damaged
  source recovery, partial progress ring, completed check, favorite heart, no page
  overflow, no failed application resources, and an empty console log.
