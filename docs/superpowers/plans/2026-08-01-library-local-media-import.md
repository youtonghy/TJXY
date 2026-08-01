# Library Local Media Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe server-folder selection and support both TMDB-assisted and strictly local
Jellyfin/Emby/Kodi-style Library imports.

**Architecture:** Keep scan depth, metadata completeness, and metadata source mode independent.
A configured filesystem-browser module resolves root IDs plus relative paths into validated server
directories, while durable ResolveMetadata jobs capture whether remote providers are allowed. NFO
and local artwork publish through existing metadata and AssetBlob interfaces.

**Tech Stack:** Rust, Tokio, Axum, SeaORM/SeaQuery, SQLite/PostgreSQL/MySQL migrations, React 19,
TypeScript, HeroUI v3, lucide-react, Vitest, Testing Library.

## Global Constraints

- Preserve unrelated dirty worktree changes.
- Use HeroUI v3 compound components and `onPress`; use lucide-react rather than emoji.
- Never expose canonical absolute filesystem paths in Admin responses.
- Restrict browsing and binding to canonical descendants of `TJXY_MEDIA_BROWSER_ROOTS`.
- Existing Libraries retain AutomaticScrape behavior.
- LocalOnly never invokes a remote MetadataProvider when it is the effective job mode.
- Require Movies or TV Shows for new local filesystem bindings; do not guess mixed content.
- Use public-interface tests at Library API, filesystem browser API, Metadata Resolve, and Admin UI.

---

### Task 1: Persist Metadata Source Mode

**Files:**
- Create: `crates/db/src/migration/m20260801_000044_metadata_source_mode.rs`
- Modify: `crates/db/src/migration/mod.rs`
- Modify: `crates/domain/src/scan_policy.rs`
- Modify: `crates/db/src/library.rs`
- Modify: `crates/application/src/library.rs`
- Modify: `crates/api/src/library.rs`
- Test: `crates/domain/tests/catalog_contract.rs`
- Test: `crates/db/tests/library_repository_contract.rs`
- Test: `crates/api/tests/virtual_folder_golden.rs`

**Interfaces:**
- Produces: `MetadataSourceMode::{AutomaticScrape, LocalOnly}`.
- Produces: persisted strings `automatic_scrape` and `local_only` on `libraries`.
- Produces: `LibraryOptionsDto.MetadataSourceMode` and update parsing with AutomaticScrape default.

- [ ] Write a failing domain mapping test asserting both values and the AutomaticScrape default.
- [ ] Run `cargo test -p tjxy-domain --test catalog_contract metadata_source_mode` and confirm red.
- [ ] Add the enum, migration with a non-null `automatic_scrape` default, and repository read/write.
- [ ] Run the domain and repository contracts and confirm green.
- [ ] Add failing API golden assertions for `MetadataSourceMode` on read and update DTOs.
- [ ] Extend application/API/server policy parsing and run `cargo test -p tjxy-api --test virtual_folder_golden`.

### Task 2: Capture Source Mode On Metadata Work

**Files:**
- Modify: `crates/db/src/migration/m20260801_000044_metadata_source_mode.rs`
- Modify: `crates/db/src/work_job.rs`
- Modify: `crates/db/src/discover.rs`
- Modify: `crates/db/src/full_scan.rs`
- Modify: `crates/db/src/metadata_work.rs`
- Modify: `crates/application/src/metadata.rs`
- Test: `crates/db/tests/work_job_repository_contract.rs`
- Test: `crates/application/tests/metadata_resolve_service_contract.rs`

**Interfaces:**
- Produces: `WorkJobSpec::with_metadata_source_mode(MetadataSourceMode)`.
- Produces: `WorkJob::metadata_source_mode() -> Option<MetadataSourceMode>`.
- Join rule: AutomaticScrape dominates LocalOnly for the same active ResolveMetadata work.
- Metadata Resolve consumes the captured mode and uses an empty provider list for LocalOnly.

- [ ] Add a failing WorkJob contract for persistence and LocalOnly-to-AutomaticScrape upgrade.
- [ ] Run the focused WorkJob contract and confirm the missing field/method failure.
- [ ] Add the nullable work_jobs column, typed accessors, validation, insert, join, and claim mapping.
- [ ] Run the focused WorkJob contract and confirm green.
- [ ] Add a failing Metadata Resolve contract with a counting provider and LocalOnly NFO fixture;
  assert zero provider calls and naming fallback for missing NFO fields.
- [ ] Propagate mode from Discover/FullScan into the WorkJob and filter providers in execution.
- [ ] Run discover, full-scan, and metadata service contracts and confirm green.

### Task 3: Add The Restricted Filesystem Browser Module

**Files:**
- Create: `crates/application/src/filesystem_browser.rs`
- Modify: `crates/application/src/lib.rs`
- Test: `crates/application/tests/filesystem_browser_contract.rs`

**Interfaces:**
- Produces: `FilesystemBrowser::from_path_list(OsString) -> Result<Self, FilesystemBrowserError>`.
- Produces: `roots() -> Vec<FilesystemBrowserRoot>`.
- Produces: `list(root_id, relative_path) -> Result<FilesystemDirectoryPage, ...>`.
- Produces: `resolve(root_id, relative_path) -> Result<ResolvedFilesystemDirectory, ...>`.
- Page entries contain only root ID, relative path, name, optional modified time, and child hint.

- [ ] Add failing contracts using TempDir roots for stable ordering and opaque root IDs.
- [ ] Add failing traversal, absolute-relative-path, symlink-escape, unreadable, and stale tests.
- [ ] Run `cargo test -p tjxy-application --test filesystem_browser_contract` and confirm red.
- [ ] Implement canonical allowlist construction, normalized relative paths, bounded enumeration,
  descendant verification, and sanitized errors.
- [ ] Run the browser contract and confirm green.

### Task 4: Expose Browser And Filesystem Binding Admin APIs

**Files:**
- Create: `crates/api/src/filesystem_browser.rs`
- Modify: `crates/api/src/lib.rs`
- Create: `crates/server/src/filesystem_admin.rs`
- Modify: `crates/server/src/lib.rs`
- Modify: `crates/server/src/startup.rs`
- Modify: `crates/server/src/library.rs`
- Modify: `crates/application/src/library.rs`
- Modify: `crates/db/src/library.rs`
- Test: `crates/server/tests/browse_routes.rs`
- Test: `crates/db/tests/library_repository_contract.rs`

**Interfaces:**
- Produces: `GET /Admin/Filesystem/Roots`.
- Produces: `GET /Admin/Filesystem/Directories?RootId=...&Path=...`.
- Produces: `POST /Admin/Libraries/{library_id}/FilesystemRoots`.
- Extends AddVirtualFolder DTO with optional `{RootId, RelativePath}` selection.

- [ ] Add authenticated route tests for root listing and one directory page with no absolute paths.
- [ ] Add rejection tests for unauthenticated, traversal, stale, and unknown-root requests.
- [ ] Wire startup configuration and route DTO mapping; run focused server tests green.
- [ ] Add repository contract for atomic attach, duplicate rejection, and initial sync submission.
- [ ] Implement attach-by-Library-ID and runtime activation with explicit rollback/fencing behavior.
- [ ] Add create-with-selection and existing-Library attach route tests and run them green.

### Task 5: Discover And Publish Local Artwork

**Files:**
- Modify: `crates/db/src/metadata_work.rs`
- Modify: `crates/application/src/metadata.rs`
- Modify: `crates/db/src/metadata.rs`
- Test: `crates/application/tests/metadata_resolve_service_contract.rs`

**Interfaces:**
- Extends MetadataWorkSnapshot with optional Primary and Backdrop storage candidates.
- Selection follows the exact precedence and ambiguity rules in the approved design.
- Asset publication consumes bounded StorageBackend bytes and existing AssetWriteService validation.

- [ ] Add failing Movie contracts for poster, folder, cover, and stem-poster precedence.
- [ ] Add failing backdrop, ambiguity, oversized, changed-object, and invalid-image warning contracts.
- [ ] Run the metadata service contract and confirm red for missing artwork discovery.
- [ ] Extend the SQL snapshot to select bounded direct-child image candidates with revision facts.
- [ ] Read and revalidate local images through StorageBackend and publish Primary/Backdrop assets.
- [ ] Run metadata service and publication repository contracts green.

### Task 6: Add Admin Filesystem APIs And Folder Picker

**Files:**
- Create: `admin/src/libraries/filesystemApi.ts`
- Test: `admin/src/libraries/filesystemApi.test.ts`
- Create: `admin/src/libraries/FolderPickerDialog.tsx`
- Test: `admin/src/libraries/FolderPickerDialog.test.tsx`

**Interfaces:**
- Produces: `listFilesystemRoots`, `listFilesystemDirectories`, and `attachFilesystemRoot`.
- Produces: controlled `FolderPickerDialog` returning `{rootId, relativePath, displayName}`.

- [ ] Add failing API validation tests for opaque roots, relative entries, and sanitized errors.
- [ ] Implement typed request/response validation and run the API test green.
- [ ] Add failing UI tests for tree expansion, List View navigation, Back/Up/Refresh, selection,
  loading, empty, permission, stale, retry, keyboard, and pending states.
- [ ] Implement the HeroUI modal with lucide-react icons, tooltips, and stable responsive columns.
- [ ] Run the FolderPickerDialog tests, typecheck, and lint green.

### Task 7: Integrate Library Creation And Settings

**Files:**
- Modify: `admin/src/libraries/libraryApi.ts`
- Modify: `admin/src/libraries/libraryUi.ts`
- Modify: `admin/src/libraries/LibraryCreateDialog.tsx`
- Modify: `admin/src/libraries/LibraryEditPage.tsx`
- Test: `admin/src/libraries/libraryApi.test.ts`
- Test: `admin/src/libraries/LibraryCreateDialog.test.tsx`
- Test: `admin/src/libraries/LibraryEditPage.test.tsx`

**Interfaces:**
- CreateLibraryRequest gains `metadataSourceMode` and optional `filesystemSelection`.
- LibraryOption gains `metadataSourceMode`.
- Settings can attach and detach opaque roots without exposing server paths.

- [ ] Change API tests first for mode serialization and optional browser selection; confirm red.
- [ ] Extend the client contracts and response validator; run API tests green.
- [ ] Add creation-dialog tests for Automatic/LocalOnly selection, local content-type restriction,
  folder selection, optional empty creation, preservation after failure, and pending state.
- [ ] Integrate the picker and source-mode controls using HeroUI; run creation tests green.
- [ ] Add settings tests for current roots, attach, detach confirmation, reload, and errors.
- [ ] Implement the settings Storage roots section and run settings tests green.

### Task 8: Documentation, Full Verification, And Review

**Files:**
- Modify: `README.md`
- Modify: `PLAN.md`
- Modify: `.gitignore`

**Interfaces:**
- Documents `TJXY_MEDIA_BROWSER_ROOTS`, both metadata modes, supported NFO/artwork names,
  security scope, and Movies/TV Shows directory requirement.

- [ ] Update nearby storage and metadata documentation and ignore `.superpowers/` artifacts.
- [ ] Run `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `npm test -- --run`, `npm run typecheck`, `npm run lint`, and `npm run build` in `admin/`.
- [ ] Exercise create, browse, attach, LocalOnly, and AutomaticScrape in the running app; inspect
  desktop/mobile screenshots and console output.
- [ ] Run `git diff --check`, review security/performance/error handling, and commit only files from
  this plan without staging unrelated worktree changes.
