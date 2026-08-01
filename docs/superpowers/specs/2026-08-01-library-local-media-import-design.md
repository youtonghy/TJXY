# Library Local Media Import Design

## Status

Approved on 2026-08-01.

## Context

TJXY already separates Storage Sync, Media Scan, metadata resolution, source indexing, and
publication. The server can create a Library with one filesystem root through the Jellyfin-compatible
`paths` query parameter, but the Admin client deliberately creates empty Libraries and has no local
filesystem binding workflow. Cloud storage has folder browsers; local storage does not.

Metadata resolution already discovers conventional NFO sidecars and gives them field precedence over
TMDB. It still calls enabled providers to fill missing fields, and it does not ingest local poster or
backdrop files automatically. The approved feature completes the two intended workflows:

- classified media directories whose missing metadata may be scraped from TMDB;
- Jellyfin/Emby/Kodi-style directories whose NFO and local artwork are authoritative and must not
  cause remote metadata requests.

## Goals

- Add a server-side folder picker with a lazy File Tree and current-directory List View.
- Allow Library creation and existing Library settings to bind local filesystem directories.
- Keep filesystem browsing inside explicitly configured allowed roots.
- Add an independent metadata source mode without changing scan-depth profile semantics.
- Support `AutomaticScrape` and `LocalOnly` modes.
- Import conventional Movie, Series, Season, and Episode NFO sidecars.
- Import conventional primary and backdrop artwork through the existing AssetBlob pipeline.
- Preserve current behavior for existing Libraries.

## Non-Goals

- Browser-native file upload or selecting directories from the administrator's client machine.
- Exposing unrestricted server filesystem paths.
- Plex bundles, Jellyfin databases, Emby databases, or provider-specific plugin hosts.
- Automatic Movie/Series classification inside a mixed Library.
- Per-Library copies of a shared CatalogItem's canonical metadata.
- Writing metadata or artwork back into media directories.

## Metadata Source Mode

`MetadataSourceMode` is orthogonal to `ScanProfile` and metadata completeness. It has two values:

- `AutomaticScrape`: use NFO and local artwork first, allow enabled remote providers to fill missing
  fields or artwork, then apply naming fallback.
- `LocalOnly`: use NFO and local artwork, skip all remote providers, then apply naming fallback.

Existing Libraries migrate to `AutomaticScrape`. New Libraries default to `AutomaticScrape` unless
the administrator selects `LocalOnly`.

The mode is captured on each ResolveMetadata WorkJob. Joining jobs merge toward
`AutomaticScrape`, just as metadata requirements merge toward the stronger requirement. Therefore a
Library configured as `LocalOnly` never independently schedules remote work; an already-shared
CatalogItem may still receive canonical remote metadata because another Library requested automatic
resolution. This preserves shared CatalogItem identity without a Library-local projection layer.

## Filesystem Browser

The server loads an allowlist from `TJXY_MEDIA_BROWSER_ROOTS`. It is a platform path-list value using
the operating system separator. Every entry must canonicalize to a readable directory at startup.
Duplicate canonical paths are rejected. Existing attached filesystem roots remain visible in Library
settings but do not implicitly broaden the browser allowlist.

The browser interface exposes:

- opaque stable root ID and display label;
- a normalized relative directory path;
- child directory name, relative path, modification time when available, and whether children may
  exist.

It never returns the canonical absolute root. Every list and bind operation canonicalizes the
selection, verifies that it is a directory, and verifies that it remains under the configured root.
Symlink escape, `..`, absolute relative paths, unreadable directories, stale selections, and duplicate
Library bindings are explicit errors.

The Admin API has three operations:

- list allowed filesystem browser roots;
- list one bounded, name-sorted page of child directories;
- attach a root/relative-path selection to a Library.

Library creation accepts the same selection and uses the existing atomic Library + filesystem root
repository operation. Existing `paths` compatibility remains server-side but the new Admin UI never
sends an absolute filesystem path.

## Folder Picker Experience

The approved HeroUI modal uses:

- a lazy File Tree on the left;
- a directory-only List View on the right;
- breadcrumb path, Back, Up, and Refresh commands;
- a stable selected-folder footer and `Select this folder` command;
- HeroUI v3 Button, Modal, Tooltip, Table/ListBox primitives and `lucide-react` icons;
- explicit loading, empty, permission, stale-selection, and retry states.

Icon-only commands have `aria-label` and Tooltip names. Files are never selectable. Library creation
allows an optional local directory so empty Libraries remain available for later cloud binding.
Library settings display current opaque storage roots, allow adding a local root, and retain the
existing safe detach command.

## Local Metadata And Artwork

NFO selection retains current rules:

- Movie: `movie.nfo`;
- Series: `tvshow.nfo`;
- Season: `season.nfo`;
- Episode: one `.nfo` whose stem exactly matches the active video stem, case-insensitively.

Primary artwork precedence is:

1. `<video-stem>-poster.jpg` or `.png` for Movie/Episode;
2. `poster.jpg` or `.png`;
3. `folder.jpg` or `.png`;
4. `cover.jpg` or `.png`.

Backdrop precedence is `fanart.jpg`/`.png`, then `backdrop.jpg`/`.png`. Multiple candidates at the
same precedence are an ambiguity error. Files are read through StorageBackend with a 25 MiB encoded
limit. Existing image decoding, dimension limits, content hashing, and AssetBlob deduplication remain
authoritative. Local artwork provenance records the storage object reference, never a filesystem path.

A missing or malformed NFO produces a bounded warning and naming fallback. An invalid local image
produces a bounded warning and does not discard usable NFO metadata. `LocalOnly` never falls through
to a remote image or metadata provider.

## Content Classification

Filesystem Libraries used by this workflow require `movies` or `tvshows`. The Admin UI explains the
directory convention and does not offer `mixed` for a new local binding. Existing mixed Libraries are
not migrated or reclassified; their current behavior remains compatible.

## Failure Handling

- Invalid browser-root configuration fails startup with the offending entry index, not its full path.
- Directory enumeration returns bounded access-denied, stale-selection, and unavailable errors.
- Library/root binding is atomic and does not leave an empty Library after failure.
- Metadata jobs fence both requirement and source mode before publication.
- Provider errors remain warnings for AutomaticScrape and are impossible in LocalOnly execution.
- NFO and artwork reads revalidate object revision before publication.

## Verification

- Domain/API golden tests cover source-mode serialization and defaults.
- Repository migration and WorkJob contracts cover persistence, joining, and stale fencing.
- Server route tests cover root browsing, traversal/symlink rejection, create-with-selection, attach,
  detach, authentication, and absence of absolute paths.
- Metadata service contracts cover LocalOnly provider suppression, NFO precedence, primary/backdrop
  selection, bounded reads, warnings, and AssetBlob publication.
- Vitest covers folder navigation, keyboard-accessible selection, creation, settings attachment,
  metadata mode selection, errors, and pending states.
- Full Rust, TypeScript, lint, build, and browser visual checks run before completion.
