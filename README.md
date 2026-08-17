# TJXY

TJXY is a Jellyfin-compatible media catalog server that separates logical
media identity from the local or cloud objects that provide its bytes.

The repository is being implemented incrementally from [`PLAN.md`](PLAN.md).
The current foundation includes:

- path-independent `CatalogItem`, `MediaSource`, and `MediaLocation` contracts;
- a provider-neutral streaming `StorageBackend` interface;
- the initial SQLite/PostgreSQL-compatible SeaORM migration set;
- transactional field-level UserData updates with per-user revisions;
- a fenced, retryable storage outbox with contiguous reconciliation watermarks;
- durable WorkJob single-flight/lease/staging primitives and replay-safe scoped
  inventory page commits with root-local object state;
- durable root-scoped title discovery and metadata resolution workers, including
  SQL-only NFO selection, Naming fallback, and an optional bounded `TMDb` provider;
- publication-owned Series structure projections with a validated seal step and
  atomic active-pointer/catalog-generation switching;
- publication-owned MediaSource/MediaLocation/Subtitle projections that retain
  stable presentation keys and Probe state across atomic Source re-indexes;
- a leased Filesystem Probe worker that reads bounded Matroska head/tail ranges,
  persists canonical stream metadata, and retains tombstoned delivery indexes;
- versioned catalog/user/probe cache key contracts;
- a pinned direct-play `PlaybackInfo` JSON golden;
- a root-confined `FilesystemBackend` with bounded byte-range streaming;
- a native Google Drive adapter with server-side authorization-code/PKCE setup,
  My Drive/Shared Drive scoping, OAuth refresh single-flight, Changes pagination,
  strict Range reads, and `Retry-After`;
- a native `OneDrive` Personal adapter with encrypted rotating OAuth credentials,
  Delta pagination, same-origin continuation validation, and bearer-safe content redirects;
- Argon2id local authentication with durable, digest-only session storage;
- an Axum server with L0 discovery, L1 login/current-user, session capability
  reporting, SQL-backed L2 library/item browsing, and root-confined original
  image streaming; and
- health routes that probe the SQL source of truth.

The server is not yet a complete Jellyfin implementation. A HeroUI v3 administrator
application is available for sign-in, local-user, library, durable-task, and access
management, QR login/authorization, personal session management, plus the Google
Drive/OneDrive Personal OAuth and root-selection flows;
the compatibility matrix in
[`docs/api-parity.md`](docs/api-parity.md) remains the authoritative record of
implemented behavior.

The ordinary React client supports administrator-selected, compiled themes with
per-theme options. See [`docs/themes.md`](docs/themes.md) for the registry contract,
versioning rules, HTTP API, fallback behavior, and the steps for adding a theme.

## Build the Admin application

The Admin application requires Node.js 22.12 or newer; CI pins Node.js 22.22.3.
It uses React 19, headless `ra-core` controllers, HeroUI v3, and Tailwind CSS v4.
HeroUI owns the shell, forms, tables, overlays, feedback, and responsive presentation;
Material UI, Emotion, and the React Admin Material UI package are not runtime
dependencies.
Install its locked dependencies and run the same static checks used by CI:

```bash
npm --prefix admin ci
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build
```

The production server serves the resulting application at `/admin/` and the
ordinary-user media client at `/app/` from the same origin as the API. `/` redirects
to `/app/`. It reads `admin/dist` by default, or the directory named
by `TJXY_ADMIN_DIST_DIR`, and fails startup explicitly when that distribution or
its `index.html` is missing or invalid. Build the Admin application before
starting the server. The current UI covers sign-in, Users list/create/edit/delete,
Libraries list/create/rename/delete and versioned scan-policy editing, ScheduledTasks
start/cancel, recent durable-job status, scoped Validate/Discover/Resolve/Expand/Index/Probe commands, device
rename/revocation, API key lifecycle management, plus the Google Drive and OneDrive
Personal authorization, paginated folder-selection, and binding flows. Bootstrap setup remains
environment-driven, and storage status/reauthorization, task-log/cache-state, metadata,
migration, and conflict pages are not yet implemented. The browser session is
stored in `sessionStorage`; normal logout and personal session revocation also
invalidate the server-side session. If another device revokes a session, the
client exits on the next request or periodic account check.

The `/app/` client has an independent `tjxy.web.*` browser session and HeroUI shell
for user sign-in, expanded home/library browsing, popular search suggestions, item details,
favorites/played state, direct-play preparation, TMDB/server rankings, a self-service
profile with range-based viewing statistics, and a grounded AI media assistant at `/app/ai`.
Profile edits are confirmed with the current
password; username or password changes revoke the browser session and require a fresh login.
Playback progress accumulates bounded watched time so seeks are not counted as viewing time.
Browser media uses a short-lived, session-scoped playback ticket rather than placing the login
token in a `<video>` or `<audio>` URL. Ticket URLs are restricted to currently visible
direct-play sources; unsupported containers remain visible as an actionable browser-compatibility
message. TMDB ranking credentials are decrypted only on the server, refreshed at most once per
UTC day while the process is running, and fall back to the last successful in-memory result when
a refresh fails.

## Run the HTTP server

`TJXY_SERVER_ID` is required so the externally visible server identity remains
stable across restarts. Generate and persist one UUID in deployment config, then
run:

```bash
TJXY_SERVER_ID=018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11 \
TJXY_SERVER_NAME="Living Room" \
TJXY_BOOTSTRAP_ADMIN_USERNAME=admin \
TJXY_BOOTSTRAP_ADMIN_PASSWORD='replace-me' \
cargo run -p tjxy-server
```

## Configure the AI assistant

Administrators configure the assistant at `/admin/settings/ai`. TJXY accepts an
OpenAI-compatible provider whose chat endpoint is
`POST {base_url}/chat/completions`; the administrator controls the upstream model
IDs, user-facing model names, visibility, default selection, per-model reasoning
effort (`off`, `low`, `medium`, `high`, `xhigh`, or `max`), and an additional
system prompt. `off` omits `reasoning_effort` from upstream Chat Completions
requests. Persistent AI settings require `TJXY_CREDENTIAL_KEYRING`. The API
key is write-only in the browser, encrypted before it enters SQL, and never
returned by the settings API.

Reusing the stored key is allowed only while the provider origin is unchanged;
changing the scheme, host, or port requires entering a new key. Provider URLs
must use HTTPS. Immediately before each provider operation, TJXY resolves the
hostname, rejects the entire DNS answer set if any address is loopback, private,
link-local, multicast, documentation, benchmark, reserved, or otherwise
non-public, and pins the accepted addresses for that operation without changing
the HTTP Host header or TLS SNI. Redirects are disabled and system proxy settings
are ignored. Loopback and private-network providers are not supported, including
for local development.

Authenticated administrators use `GET`, `PUT`, and `DELETE /Admin/Ai/Settings`,
`POST /Admin/Ai/Settings/Test`, and `POST /Admin/Ai/Settings/Models`. The model
discovery endpoint calls the provider's `GET {base_url}/models` route on the
server and returns a bounded, sorted list of model IDs without exposing the API
key to the browser. Ordinary users receive only enabled,
administrator-visible aliases from `GET /Ai/Models`, and use the owner-scoped
`/Ai/Conversations` routes plus `POST /Ai/Chat`. Chat responses use a bounded SSE
contract and are persisted only after a complete assistant response. New chat
requests include a client-generated conversation UUID so an interrupted stream
can be reconciled with an atomically committed first exchange. Pending IDs are
kept in tab-scoped session storage and checked again after reload or reconnect.
Conversation detail is bounded to the latest 200 messages, returned in chronological order;
older messages remain stored but are not currently exposed through pagination.

AI admission defaults to 10 new chat requests per user per minute, 2 concurrent
SSE streams per user, 8 concurrent SSE streams server-wide, and 100 requests per
user per UTC day. Configure these limits with
`TJXY_AI_REQUESTS_PER_MINUTE`,
`TJXY_AI_MAX_CONCURRENT_STREAMS_PER_USER`,
`TJXY_AI_MAX_CONCURRENT_STREAMS`, and `TJXY_AI_DAILY_QUOTA`. Administrators can
set server-wide and per-user daily Token limits under **Admin > AI assistant**;
these limits use upstream-reported `total_tokens`, reset at server-local
midnight, and treat `0` as unlimited. Admission rejections
return `429 Too Many Requests` with `Retry-After` in delta seconds and
`Cache-Control: no-store`; provider I/O is not started for rejected requests.

The server applies a non-overridable media-only policy and gives the Agent a
bounded, read-only MCP-style tool registry for catalog search, media details,
recent viewing history, aggregate preferences, favorites, resume items, and
recommendation candidates. Every user-context lookup derives identity from the
authenticated session; the browser never supplies an arbitrary user ID and never
receives provider credentials, upstream model IDs, tool arguments, raw metadata,
or internal reasoning. This registry is an internal Agent boundary, not a public
MCP transport.

Retrieval currently uses structured SQL queries over TJXY catalog and user data.
It does not yet provide embeddings, a vector index, or semantic similarity search.
Deployments should still restrict outbound network access so the configured
provider URL can reach only approved AI endpoints as a defense-in-depth control.

## Run the diagnostic TUI

The repository includes a Rust terminal console for local service control,
status, and backend log diagnostics:

```bash
./tjxy
```

The root launcher uses `target/release/tjxy-tui` when available and falls back to
`target/debug/tjxy-tui`. Build it once with
`cargo build --release --locked -p tjxy-tui`; subsequent launches do not invoke
Cargo.

It discovers TJXY server processes from this workspace even when another launcher
started them, and shows process, listener, HTTP, installation-config, frontend
artifact, and log-file status. Use `1` to start, `2` to stop, and `3` to restart
the single server instance that belongs to this workspace. Ambiguous multi-instance
operations are refused. The TUI does not perform setup, connect to the database,
build artifacts, or perform database maintenance. Process control currently targets
local macOS and Linux environments and requires `ps`, `lsof`, and `kill`.

Use `g` to switch between Chinese and English. Set `TJXY_TUI_LANGUAGE=en-US` to
start in English; Chinese is the default. Backend logs are read from
`TJXY_LOG_FILE`, or `data/server.log` when it is unset. The process launcher must
redirect backend stdout and stderr to that file for logs to appear in the TUI.
The server also writes structured JSON Lines logs to `data/logs`; set
`TJXY_LOG_DIR` to override that directory. **Admin > Logs** switches file logging
between the default Error mode and a Debug mode covering scan, metadata,
expansion, source indexing, probe, import, and Lazy-click workflows. Files rotate
on UTC dates and retain 1-365 days as configured by an administrator. Log fields
must not include authentication headers, credentials, response bodies, or raw
provider payloads.
Development builds pair binaries by Cargo profile: a debug TUI starts only the
debug server, and a release TUI starts only the release server. The diagnostics
view shows the selected server path. Portable release bundles continue to use
the sibling `tjxy-server` binary.
Before installation is complete, the TUI exposes only service startup and a prompt
to finish setup in the desktop application; status and log content unlock after the
installation manifest reaches the completed state.

The default bind address is `127.0.0.1:8096`; override it with `TJXY_BIND`.
During first-run setup, `TJXY_SETUP_BIND` takes precedence when set, otherwise
setup inherits `TJXY_BIND`.
The default database is `sqlite://tjxy.db?mode=rwc`; override it with
`TJXY_DATABASE_URL`. TJXY uses SeaORM's `seaql_migrations` history as the
database schema version. A newer server automatically applies every pending
migration before accepting traffic. An older server refuses to open a database
that contains migrations it does not recognize; restore a database backup from
the matching release before intentionally downgrading. Do not delete migration
history to bypass this check. Startup also rejects a current migration history
whose critical tables, columns, or indexes are missing instead of guessing how
to repair potentially damaged data.

Original image assets default to `./data/assets`; override
that root with `TJXY_ASSETS_DIR`. The asset store validates actual image format,
encoded size, dimensions, pixel count, and decoder allocation before an
fd-confined atomic SHA-256 write; JPEG, PNG, GIF, WebP, and BMP originals are
accepted. Administrators can inspect linked, orphaned, missing, and unregistered
content-addressed assets under `/admin/settings/metadata`, clean unreferenced
assets, and select a new writable absolute storage location. A database-selected
location takes effect after restart; existing roots remain readable for stored
assets. `TJXY_ASSETS_DIR` has runtime precedence and disables location editing
until the override is removed. Lazy Expand/Source requests wait up to 2500 ms
for joined work by default; set `TJXY_LAZY_WAIT_MS` to `0..=30000` to override
it. The production binary submits a lowest-priority policy-aware media refresh
every 900 seconds by default. Set `TJXY_MEDIA_REFRESH_INTERVAL_SECONDS` to a
value from `1..=2592000` to change the period, or `0` to disable the scheduler.
The first refresh waits for one complete interval, and durable single-flight
prevents overlapping scans for the same Library. Administrators can create a
Filesystem-backed VirtualFolder by supplying
one existing directory in the `paths` query parameter. TJXY canonicalizes and
persists that root, creates its storage identity and initial sync job atomically,
and reloads every active Filesystem root after restart. The legacy
`TJXY_FILESYSTEM_ACCOUNT_ID` plus `TJXY_FILESYSTEM_ROOT` pair remains an explicit
runtime override; both variables are required together. Each configured root
that is currently available starts an account-scoped inventory worker and shares
the serial Probe worker. Missing, unmounted, invalid, or unreadable Filesystem
roots are kept in the database but reported offline instead of preventing server
startup. Their scans remain paused and their media returns unavailable until the
root is restored and TJXY is restarted. A
server-side folder picker can be enabled by saving one or more media browser
roots in **Admin > System settings**, or by setting `TJXY_MEDIA_BROWSER_ROOTS`
to a platform path-list of allowed parent directories (colon-separated on Unix
and semicolon-separated on Windows). An explicitly present environment variable,
including an empty value, takes precedence over the database setting; database
changes apply after TJXY restarts. Missing, unmounted, unreadable, and duplicate
browser roots are skipped during startup instead of preventing the server from
running. The system settings page reports each skipped root so an administrator
can repair or remove it; saving new settings continues to reject unavailable
roots. The Admin API exposes opaque root IDs and relative paths only; every
selection is canonicalized again before it can be attached to a Library, and
symlinks cannot escape an allowed root.

A Library's metadata source is independent of its scan profile. The default
`automatic_scrape` mode reads local NFO and artwork first and uses configured
remote providers only to fill missing metadata. The `local_only` mode reads NFO
and naming metadata without invoking remote providers. Supported local artwork
names include `poster`, `folder`, `cover`, and `*-poster` for Primary images,
plus `fanart` and `backdrop` for Backdrop images, using JPEG, PNG, GIF, WebP, or
BMP extensions. Malformed NFO and artwork produce work warnings and fall back to
the remaining permitted metadata sources. Movie and Series detail access retries
`Partial` metadata in `automatic_scrape` mode with durable single-flight and a
short cooldown; `local_only` libraries never invoke remote providers. Title
discovery accepts both `Movie(2026)` and `Movie (2026)` directory names.
The normalized title and four-digit year are stored separately in
`catalog_items.name` and `catalog_items.production_year`, and remote searches
always pass those values as separate parameters. Upgrades repair legacy Partial
Movie/Series rows that still contain a trailing year in `name`, while preserving
rows that already have remote title provenance or provider identities.

`local_only` libraries can additionally select `import` or `direct` local
metadata access. `import` preserves the existing behavior: parsed NFO fields are
published to the catalog and local artwork is copied into TJXY's asset store.
`direct` is available only for filesystem roots. It keeps the lightweight item,
source, playback, subtitle, and user-data indexes required by the Jellyfin API,
but stores only revision-fenced references to NFO, Primary, and Backdrop files.
Detail requests parse the referenced NFO in memory and image requests stream the
original file without copying bytes into TJXY. Lazy Direct libraries create
those references only when an item is opened; lists therefore keep the naming
placeholder until then. Search, sort, filters, similar-items, and AI continue to
use the lightweight SQL fields rather than NFO-only rich fields. If an item is
also visible through an Import or automatic library, imported canonical metadata
takes precedence. Switching to Direct does not immediately delete historical
asset blobs; normal reference-safe asset cleanup remains responsible for them.
Administrator detail pages show an accent scan-in-progress status while the
item remains Partial, then distinguish a confirmed provider no-match from a
generic unavailable result. These operational messages are not rendered for
regular users, and the underlying task observation API remains administrator-only.

A native recursive filesystem event monitor is
enabled by default; it coalesces event bursts for 500 ms and schedules durable
inventory only for already-materialized parent directories. Set
`TJXY_FILESYSTEM_REALTIME=false` to disable this hint path; explicit Validate
remains the authoritative repair operation because platform watchers can lose or
only partially describe rename events. The inventory worker claims only that account's
durable scoped-sync jobs and persists paged object/outbox updates. The Probe
worker reads at most 1 MiB from each end of an object and currently supports Matroska;
unsupported or incomplete containers are recorded as terminal Probe failures.
Remote metadata is disabled by default. Set
`TJXY_ENABLE_REMOTE_PROVIDERS=true` and provide the secret
`TJXY_TMDB_ACCESS_TOKEN` to enable `TMDb`; this credential must be the TMDB API
Read Access Token used as an HTTPS Bearer token, not a legacy v3 API key.
`TJXY_TMDB_LANGUAGE` defaults to `zh-CN`. Authenticated administrators can manage
the same setting through `/Admin/Metadata/Providers/Tmdb`; persistent saves
require `TJXY_CREDENTIAL_KEYRING`, are encrypted before entering SQL, and apply
to subsequent metadata jobs without a restart. A database setting takes
precedence over the environment fallback, including when it disables TMDB;
deleting it restores the fallback. A missing database row preserves the
environment behavior, while a missing token keeps the provider disabled and
performs no remote metadata requests. Selected Movie/Series posters are
downloaded only from the fixed TMDb image host with redirects disabled and
bounded time/bytes, validated by the asset store, and published as local Primary
images. Image failures are recorded as work warnings without discarding usable
text metadata.

Music Libraries resolve discovered Audio items through TheAudioDB followed by
MusicBrainz when their metadata mode is `automatic_scrape`. Common names such as
`Artist - Track` and `01 - Artist - Track` are split into artist and recording
evidence; MusicBrainz title search remains available when the artist is absent.
The providers publish recording, artist, and release-group identities, year,
description, genre, artist credit, and a square Primary image when available.
MusicBrainz requests use an identifying User-Agent and are serialized to at most
one request per second; override it with `TJXY_MUSICBRAINZ_USER_AGENT`.
TheAudioDB uses its public development key `2` by default; production installs
should set `TJXY_THEAUDIODB_API_KEY` to their own key. Remote artwork is accepted
only from pinned HTTPS TheAudioDB hosts and passes through bounded asset
validation. Authenticated administrators can also manage both values from
**Admin > Metadata**. Database overrides are encrypted with
`TJXY_CREDENTIAL_KEYRING`, hot-applied without a restart, and take precedence
over the environment fallback. Deleting an override restores the environment
configuration. TheAudioDB keys are write-only in the Admin API; MusicBrainz has
no API token and instead exposes its non-secret identifying User-Agent.

For local UI development, a one-off importer can populate the current database
with fixed manifests of 100 Movies and 100 Series. It reads only the enabled,
encrypted database TMDB setting, imports normalized metadata associations and a
bounded hierarchy of up to three Seasons with twelve Episodes per Season, and
stores validated local poster/backdrop images. The public libraries are named
`Movies` and `TV Shows`. Re-running the command updates the same deterministic
catalog identities:

```bash
TJXY_DATABASE_URL='sqlite://tjxy.db?mode=rwc' \
TJXY_ASSETS_DIR='./data/assets' \
TJXY_CREDENTIAL_KEYRING='{"active_version":1,"keys":{"1":"<base64-32-bytes>"}}' \
cargo run -p tjxy-server --bin import_tmdb_demo
```

Playable development media is attached explicitly after the metadata import.
The command publishes ordinary filesystem-backed media sources through the same
storage and source-projection tables used by the server. Every Movie and Episode
receives a short valid H.264/AAC MP4. Twelve deterministic items additionally
receive 1080p and 720p choices, Chinese and English WebVTT subtitles, and one
low-priority zero-byte source for exercising player error recovery. The command
is idempotent and is never run during normal server startup:

```bash
TJXY_DATABASE_URL='sqlite://tjxy.db?mode=rwc' \
TJXY_DEV_MEDIA_ROOT='/absolute/path/to/development-media' \
cargo run -p tjxy-server --bin attach_dev_media
```

Native Google accounts reference an encrypted `storage_credentials` row and are
loaded automatically. Supply only the deployment keyring through the secret
environment variable `TJXY_CREDENTIAL_KEYRING`, for example
`{"active_version":2,"keys":{"1":"<base64-32-bytes>","2":"<base64-32-bytes>"}}`.
New writes use the active version while historical keys remain available for
rotation. The same keyring encrypts recoverable API keys. A database with no API
keys may start without it, but API key creation and listing return `503` until a
valid keyring is configured. Once any API key exists, the keyring is required at
startup, and every historical version still referenced by an API key envelope must
remain configured. Configure the Google OAuth application only on the server by setting
`TJXY_GOOGLE_OAUTH_CLIENT_ID`, `TJXY_GOOGLE_OAUTH_CLIENT_SECRET`, and
`TJXY_GOOGLE_OAUTH_REDIRECT_URI` together. The redirect URI must exactly match
`/Admin/Storage/OAuth/GoogleDrive/Callback` at the externally registered origin.
The client secret is never accepted by an Admin DTO, and the resulting refresh
token is stored only inside the AEAD payload. The same encrypted credential
boundary applies to OneDrive Personal. Configure its Microsoft consumer OAuth
application with `TJXY_ONEDRIVE_OAUTH_CLIENT_ID` and
`TJXY_ONEDRIVE_OAUTH_REDIRECT_URI`, plus optional
`TJXY_ONEDRIVE_OAUTH_CLIENT_SECRET`; the redirect URI must end in
`/Admin/Storage/OAuth/OneDrive/Callback`. An active cloud binding without a
valid keyring or credential envelope prevents readiness instead of silently
starting an unusable backend.
Authenticated administrators start Google authorization with
`POST /Admin/Storage/OAuth/GoogleDrive/Start` and `TargetLibraryId`, then open
the returned `AuthorizationUrl`. TJXY validates one-time `state` and S256 PKCE
at the callback. After authorization, the same administrator can browse all folder pages
through `GET /Admin/Storage/OAuth/GoogleDrive/{state}/Directories`, enumerate
paginated Shared Drives through the sibling `SharedDrives` route, and commit the
chosen root through `POST /Admin/Storage/OAuth/GoogleDrive/{state}/Bind`.
`Scope` is `MyDrive` or `SharedDrive`, with `SharedDriveId` only for the latter.
Directory responses expose only a session-bound UUID `NextPageToken`; supplying it as
`PageToken` resumes the opaque provider page without exposing Google tokens to Admin.
The Shared Drive list retains its existing provider-token pagination contract.
The server obtains the account identity from Google, validates the selected
root, obtains the initial Changes cursor, and atomically commits the encrypted
credential, target-library root membership,
and one non-recursive Strict Lazy inventory job. The response reports
`InitialSyncJobId` and `RestartRequired: false`; the backend and its workers are
activated in the current process immediately after the durable binding commits.
The former Google endpoint that accepted `ClientSecret` and
`RefreshToken` request fields no longer exists. OneDrive Personal follows the
same session-bound authorization-code flow at
`/Admin/Storage/OAuth/OneDrive/Start`, `Callback`, `{state}/Directories`, and
`{state}/Bind`, including UUID-cursor pagination of every folder page. Microsoft Graph derives the account, Personal drive ID, and
root; the Admin submits only `DisplayName` and the chosen `RootObjectId`. The
legacy direct OneDrive binding endpoint no longer exists. Business and
SharePoint are rejected before binding.
Emby migration uses the same keyring. `POST /Admin/Imports/Emby` accepts
`BaseUrl`, `EmbyUserId`, `ApiKey`, `SourceInstanceId`, `DryRun`,
`TargetLibraryId`, and `TargetUserId`; the API key is immediately wrapped in a
versioned AEAD payload. The durable worker pages into replay-safe staging and
renews its lease. Dry runs complete without Catalog writes. Non-dry runs stop at
`ReadyToPublish`; review the status/counters before calling the explicit
`Publish` command, which commits Catalog rows, normalized metadata, Legacy IDs,
and UserData in one transaction.
The two bootstrap administrator variables must
both be set for a new database, and the password must not be empty. They create an
administrator only when the database has no users; use one server replica for
this first startup, then remove them from deployment configuration.
Legacy `Emby` schemes and X-Emby/X-MediaBrowser token aliases are enabled by
default; set `TJXY_LEGACY_AUTH=false` to require canonical MediaBrowser auth.

Redis defaults to `auto` at `redis://127.0.0.1:6379`: a failed local probe
continues without cache, while `TJXY_REDIS_MODE=enabled` makes connection/PING
failure block startup. `disabled` never connects. Configure
`TJXY_REDIS_URL`, `TJXY_REDIS_KEY_PREFIX`,
`TJXY_REDIS_CONNECT_TIMEOUT_MS`, `TJXY_REDIS_HOME_TTL_SECONDS`,
`TJXY_REDIS_ITEM_TTL_SECONDS`, and `TJXY_REDIS_EMPTY_TTL_SECONDS` as needed.
Auto mode rejects non-loopback endpoints. Runtime Redis errors are cache misses,
mark cache health degraded immediately, and open a short circuit after repeated
failures; SQL remains authoritative.

Set `TJXY_PUBLIC_ADDRESS` only from trusted deployment configuration when the
discovery response should advertise an address. Readiness becomes true only
after the database connection, migrations, and authentication service succeed,
and each readiness request probes the database connection.
Available compatibility routes now include `GET /System/Info/Public`,
`GET /System/Language`, setup-time `PUT /System/Language` before the first user,
and administrator-only `GET/PUT /Admin/System/Language` for the persisted interface locale,
the public branding document `GET /System/Settings`, administrator `GET/PUT /Admin/System/Settings`,
administrator `GET/PUT /Admin/System/Theme` for the ordinary React client theme,
administrator image uploads at `PUT /Admin/System/Branding/{logo|icon}`, and
administrator self-restart at `POST /Admin/System/Restart`.
System settings keep the PostgreSQL database as the source of truth; listen
address, port, and media browser root changes are applied on the next
self-restart, while explicit `TJXY_BIND`, `TJXY_SERVER_NAME`,
`TJXY_PUBLIC_ADDRESS`, and `TJXY_MEDIA_BROWSER_ROOTS` environment variables take
precedence.
authenticated `GET /System/Endpoint`,
`GET /Branding/Configuration`,
`GET /System/Ping`, `POST /Users/AuthenticateByName`, and `GET /Users/Me`, plus
administrator local-user management at `GET /Users`, `GET|DELETE /Users/{userId}`,
`POST /Users/New`, `POST /Users?userId=...`, and the `Password` and `Policy`
commands,
authenticated `GET /Sessions` and `POST /Sessions/Logout`,
administrator `GET|DELETE /Devices`, `GET /Devices/Info`, and
`GET|POST /Devices/Options`,
administrator `GET|POST /Auth/Keys` and `DELETE /Auth/Keys/{accessToken}`,
`POST /Sessions/Capabilities/Full`, the legacy client-compatible
`POST /Sessions/Capabilities`, `GET /UserViews`, `GET /Search/Hints`, `GET /Items`,
`GET /Items/{itemId}`,
authenticated `GET|POST /DisplayPreferences/{displayPreferencesId}`,
`GET|POST /Items/{itemId}/PlaybackInfo`,
`GET|HEAD /Videos/{itemId}/stream?static=true&mediaSourceId=...`,
`GET|HEAD /Audio/{itemId}/stream?static=true&mediaSourceId=...`,
the two authenticated `/Videos/{itemId}/{mediaSourceId}/Subtitles/...` forms,
`GET|POST /UserItems/{itemId}/UserData`,
`GET /UserItems/Resume`,
`GET /Items/Latest`, `GET /Shows/NextUp`,
authenticated `GET /socket` WebSocket events,
private playlist routes under `/Playlists` and authenticated shared collection reads under
`/Collections` with administrator writes under `/Admin/Collections`,
`GET|POST|DELETE /Library/VirtualFolders`,
`POST /Library/VirtualFolders/LibraryOptions`,
`POST /Library/VirtualFolders/Name`,
`DELETE /Library/VirtualFolders/Paths`,
the Google and OneDrive OAuth start/callback/directory/bind routes under
`/Admin/Storage/OAuth/GoogleDrive/...` and `/Admin/Storage/OAuth/OneDrive/...`,
administrator PathWeak relink review at
`GET /Admin/Storage/RelinkCandidates` and
`POST /Admin/Storage/RelinkCandidates/{id}/Confirm|Reject`,
administrator Emby import routes at `POST /Admin/Imports/Emby`,
`GET /Admin/Imports/{jobId}`, and the `Pause`, `Resume`, and `Publish` commands,
administrator NFO metadata import at
`POST /Admin/Items/{itemId}/Metadata/Nfo` with `application/xml` or `text/xml`,
`GET /ScheduledTasks`, `GET /ScheduledTasks/{taskId}`,
`POST|DELETE /ScheduledTasks/Running/{taskId}`, `POST /Library/Refresh`,
the safe, bounded durable-job observation route `GET /Admin/Tasks/Jobs?Limit=...`,
whose optional `Outcome` reports `NoMetadataMatch` or `CompletedWithWarnings`
without exposing provider diagnostics,
explicit administrator tasks at `POST /Admin/Tasks/ValidateStorage/{rootId}`,
`POST /Admin/Tasks/DiscoverTitles/{rootId}`,
`POST /Admin/Tasks/ResolveMetadata/{itemId}`, and
`POST /Admin/Tasks/ProbeMedia/{itemId}`,
Favorite/Played `POST|DELETE` routes,
`POST /Sessions/Playing`, `/Progress`, `/Stopped`, and `/Ping`,
`GET|HEAD /Items/{itemId}/Images/{type}`,
`GET /health/live`, and `GET /health/ready`.

Virtual-folder creation without `paths` creates an empty SQL library; attach native
cloud roots through the storage-account binding routes. Supplying one existing
directory creates a root-confined Filesystem-backed library. Deleting a virtual
folder detaches its CatalogItem memberships and StorageRoot mappings but preserves
those shared entities; active Emby import references block deletion with `409`.

Music virtual folders recursively discover `aac`, `flac`, `m4a`, `mp3`, `oga`,
`ogg`, `opus`, `wav`, `wave`, and `webm` audio files and publish each file as an
`Audio` catalog item with a directly playable source. Music artwork is presented
with a square aspect ratio in the client. Movie/series metadata resolution remains
unchanged; embedded audio-tag enrichment and the `MusicAlbum` hierarchy are not
yet implemented.

The current browse slice remains SQL-authoritative. Every authenticated enabled
user sees every enabled library because per-library grants are not yet modeled.
`/Items` supports root views, optional parent scope, case-insensitive literal
`searchTerm`, item-type filtering with or without a parent, exact `genre` and
`productionYear` filters, bounded pagination, and explicit `SortName`,
`DateCreated`, `ProductionYear`, or `Runtime` ordering. Filters apply before the
reported total and page offset, and participate in the complete cache descriptor.
`recursive=true` walks canonical or active-publication descendants with cycle
protection. A library-parent query with an item-type filter defaults to recursive
only when `recursive` is absent, matching Jellyfin; an explicit false value wins.
Search and recursion otherwise remain independent options. Published Series
projections inherit the active publication owner's enabled library memberships.
Requests for an unexpanded Series enqueue
or join one durable high-priority Expand job and wait for a bounded interval;
timeouts continue to return only the current active publication. Other sort
modes and image transforms remain unimplemented. Known malformed pagination,
boolean, and identifier parameters return `400`; unknown client hints and
unsupported collection members are ignored for Jellyfin compatibility. The
`fields` parameter is accepted, but list responses remain a compact projection.
Item pages expose priority-zero `ImageTags`, all stable Backdrop tags, primary
image aspect ratio, `DateCreated`, and `LocationType`; image GET/HEAD serves only authorized
original `ItemAsset` files with strong ETags and private revalidation, including
items visible through an active Series publication. Unix builds open each
relative path component from a pinned root directory descriptor without
following symlinks. Asset ingestion and SHA-256 deduplicating writes are not yet
connected to local-image or migration downloaders; the TMDb Primary collector
uses the bounded, format-validating, SHA-256-deduplicating atomic write service.

When Redis is enabled, UserViews, Items pages, item details, Resume, and
PlaybackInfo source metadata use cache-aside keys containing the SQL catalog
generation and user revision. PlaybackInfo entries also carry a stable digest of
the active MediaSource probe revisions, so a re-probe cannot reuse a prior
source list.
Query shapes are SHA-256 digests, concurrent misses share one bounded in-process
fill, and invalid cache payloads are deleted before falling back to SQL.
Generation or user revision changes make old keys unreachable without depending
on Redis deletion. Every catalog generation commit also records a durable cache
invalidation in the same SQL transaction. Cache writes register their key in a
TTL-bounded generation set; a fenced startup worker atomically removes at most
100 registered keys per batch without scanning the Redis keyspace. Disconnects
retry with categorized backoff, while disabled Redis completes as an explicit no-op.
After ready state is published, an enabled Redis runtime best-effort warms the
default `UserViews`, global `Latest`, per-library `Latest` for at most 64 enabled
libraries, `Resume`, and `NextUp` entries for at most 128 enabled users. Warmup
uses the normal SQL-authoritative cache-aside queries and never reads media
bytes; selection or per-user fill failures are logged and never block ready.

Administrators can read Jellyfin-shaped VirtualFolders backed by the SQL
Library/StorageRoot relationship. Locations are opaque
`tjxy://storage-root/{id}` values and never expose local paths, provider object
IDs, account identities, or credentials. LibraryOptions reads and updates the
named scan profile plus all four effective policies using `profile_version`
compare-and-swap; preset changes expand through the domain policy table, while
advanced overrides must provide all four policies together.
Filesystem creation stores the canonical path only in SQL/runtime configuration;
VirtualFolders continues to return only `tjxy://storage-root/{id}`. Renaming
updates the SQL sort key and catalog generation atomically. Root detach removes
only the requested membership; the last detach disables the Filesystem account
without deleting storage objects or user data, and reattaching the unchanged
canonical root reuses its durable identities. Runtime activation takes effect
in the current process immediately after the durable binding commits.

`GET /Items/{itemId}` returns one authenticated, enabled-library-visible item
from the canonical catalog or its active Structure publication. It does not read
staging/retired rows. A visible Movie without active sources enqueues or joins a
durable Source Index job and waits for the configured bounded interval; this
path never performs Media Probe. Rich details include normalized genres, people,
studios, provider IDs, dates, image metadata, and every currently Probed and
Available MediaSource plus the leading source's MediaStreams. Unprobed, hidden,
or unavailable sources are omitted without scheduling Probe work.

PlaybackInfo accepts an optional request or session `DeviceProfile`; without one
it returns every currently Probed and Available source, while an explicit profile
filters sources to declared direct-play containers. The obsolete query form takes
precedence over body `UserId`, `MediaSourceId`, and `EnableDirectPlay` values as
required by the Jellyfin contract. Responses emit only authenticated local TJXY
media/subtitle routes. Missing probes
join durable MediaSource-scoped Probe jobs. When a Filesystem backend is
configured, the leased worker parses bounded Matroska head/tail ranges and
atomically publishes canonical stream metadata, stable delivery indexes, and a
catalog generation. Until successful completion, the response safely contains
no invented Direct Play source. Runtime-selected storage backends can be bound
programmatically to an account and provider drive; each drive receives an
isolated scoped-sync worker while media reads and Probe remain provider-neutral.
Encrypted Google/OneDrive credential-store loading is automatic. MP4/M4V Probe
parses ISO-BMFF movie and track metadata from the same bounded head/tail input
used by other provider-neutral media inspection.

External subtitle delivery resolves the active publication by stable media
source ID and delivery index, then streams the indexed source format
byte-for-byte. Format conversion and nonzero subtitle time offsets are rejected.
UserData GET returns protocol defaults for visible items; field-level POST
updates preserve omitted values and atomically lock enabled-library visibility,
write SQL state, and increment the user's revision exactly once.
Playback events use a durable `(auth session, PlaySessionId)` identity: retried
starts do not increment `PlayCount` twice, unchanged progress does not bump the
user revision, and a stopped session cannot accept later progress. Jellyfin's
`ItemId`, `MediaSourceId`, and `PlaySessionId` are optional on these DTOs:
telemetry without an item is accepted as a no-op, while an item-only event uses
the preferred playable source and a deterministic session identity.

NFO metadata import accepts at most 2 MiB, rejects DTDs and unknown entities,
and supports Movie, Series, Season, and Episode documents. Basic fields and
Provider IDs publish with field-level source references and value hashes in the
same catalog-generation transaction. Partial NFO documents preserve unmentioned
SQL fields and other provider identities. Reconciled title roots publish
deterministic lightweight items and enqueue metadata without entering title
directories. Later Source publication discovers direct-child NFO sidecars;
`TMDb` can fill missing Movie/Series fields and a Primary poster before Naming
fallback. Local image ingestion, Season/Episode parent-aware metadata, and
association-field publication remain pending.

The storage runner can inventory one explicitly requested backend directory,
follow opaque pagination, atomically persist each page with an outbox marker,
and complete its fenced WorkJob only after the final page. It does not recurse
or infer deletion from an incomplete/failed page; after a complete scope, only
direct children not observed in that claim attempt become root-local
`ConfirmedAbsent`. Retry attempts use distinct page generations, so a changed
first page cannot conflict with a partially committed earlier attempt.
Filesystem and programmatically registered provider-drive scoped inventory are
wired at startup; each completed scope projects its versioned outbox events, invalidates
matched item revisions, and advances the contiguous reconciled watermark before
the job completes. A backend-independent startup reconciler also resumes durable
outbox backlog after a crash, scans roots with a bounded keyset cursor, and backs
off failed events without blocking unrelated roots. Retryable storage failures use capped exponential backoff,
and provider 429 responses preserve delta-seconds and HTTP-date `Retry-After`.
Retryable inventory and validation listing failures atomically mark only the
accessed root-local scope `TemporarilyUnavailable`, advance its durable outbox
revision, clear its stale materialization marker, and keep the scope eligible
for retry. The next successful inventory
page restores `Present` in that page's transaction without invalidating Probe
metadata for an availability-only transition.
Ordinary media and subtitle range reads, Probe object checks, and NFO metadata
reads use the same root-local availability protocol. A retryable stream failure
before response headers returns `503`; a backend error yielded after headers
terminates the response body because its status can no longer change. Read
failures persist only a sanitized reason and project every affected root
revision before exposing the failure. A successful object get or range open
restores `Present`, while dropping a response body without a backend error does
not change availability. A one-off backend `NotFound` is recorded as
`backend-object-not-found-unconfirmed`, never as `ConfirmedAbsent`. Playback
prefers an `Available` copy but may retry a `TemporarilyUnavailable` copy when
no healthy location remains.
Backends with a Changes capability run one low-frequency worker per provider
drive: opaque cursors, object updates, confirmed removals, root revisions, and
outbox markers commit atomically, while additions and moves are admitted only
below already materialized parents. A known object moved below an unmaterialized
parent keeps its global object fact `Present` but marks the old root relation
`TemporarilyUnavailable`; move events invalidate both the old and new matched
parents, and root-local presence is aggregated into canonical Location
availability so stale paths are excluded from playback. A
provider `410 Gone` pauses the cursor,
captures a fresh cursor, and schedules one non-recursive root inventory; the
cursor is reactivated only when that exact root-scoped `RecoverStorageCursor`
job commits. Recovery jobs use a distinct natural key so they cannot join an
inventory that started before cursor invalidation.
Recovery completion invalidates deeper materialization markers so later access
refills those scopes on demand. A terminal recovery failure atomically fails the
WorkJob and changes the cursor to `RecoveryFailed` for operational diagnosis.
After review, the application recovery API creates a newly fenced recovery job
and atomically resumes that cursor. Explicit root validation recursively reuses
the same paged inventory path, defers absence until every scope succeeds, and
then confirms omitted relations plus every still-attached descendant in one
root-revision transaction. This prevents a concurrently refreshed child from
remaining playable below a directory that the completed validation proved absent.
Structure publication is available as a repository primitive;
Source publication has the same staged seal and short pointer-switch boundary,
and Series Structure publications can atomically include every Episode's
Source/Location/Subtitle graph. The request coordinator creates and joins the
jobs. When a matched title StorageObject has not been reconciled, a browse request
creates or joins Scoped Storage Sync first and waits for its result and watermarks.
An explicit administrator Expand/Index command instead persists and returns the
final Media job immediately with a durable sync dependency; the job cannot be
claimed until the dependency revision is committed and reconciled, and a failed
dependency terminally fails the waiting Media job without copying provider errors.
The configured Filesystem worker executes
request-triggered Scoped Sync, and the SQL-only Source Index worker publishes
Movie/Episode video locations and matching external subtitles. The Series
Expand worker recursively schedules missing directory inventories, derives
restart-stable child identities from stable storage records, and atomically
publishes every Season/Episode source graph.

Full Media Scan runs as a low-priority durable, policy-aware orchestrator over
the same SQL boundaries. It reads the persisted effective policy under the
captured `profile_version`: `all_synced_objects` roots receive a fresh recursive
validation, `title_layer` roots receive only a non-recursive root Scoped Sync,
and `library_roots` performs no implicit inventory. Only `eager` expansion and
probe policies schedule Index/Expand or Probe work. `basic` and `full` metadata
policies schedule and wait for requirement-tagged Resolve work at the current
metadata revision. A Basic job is atomically upgraded when Full joins the same
active natural key, and an in-flight Basic publisher is fenced and retried when
that upgrade wins. The resolved revision and attempted requirement advance in
the same publication transaction; a usable `Partial` result does not fail the
scan, while `metadata_policy=none` does not schedule automatic Resolve. The
Full NFO resolutions additionally replace evaluated People/Genres/Studios
relations atomically, while Basic leaves those relations untouched. Online
providers still share the basic field pipeline, and versioned completeness
evidence remains pending.
Every child is recorded under the parent scan, child failures are propagated,
and cancellation terminates children created by that scan. Discovery progress
is stored per library-root binding, so an automatic scan cannot publish into a
Manual library that shares the same storage root. Recursive validation keeps
omitted relations live until its successful final sweep. Each parent/root fixes
one inventory or validation child across retries even when that child advances
the root revision. `title_layer` scans continue to process only explicit Library
members after a Structure publication; only `all_synced_objects` scans absorb
the published children into the same scan lifecycle. Supported scan profiles are
`Full`, `Lazy`, and `Manual`. Databases that still contain the removed `Hybrid`
profile are migrated to `Lazy`; a historical `background` expansion value is
normalized to `on_browse` while other advanced policy values are preserved. The
production scheduler periodically submits policy-aware scans at the lowest
queue priority and delays missed ticks instead of creating a catch-up burst.

Lazy libraries publish Naming-only title rows during discovery and defer the
complete metadata lookup until an administrator or user opens the item detail.
Full libraries enqueue that complete lookup during the scan. A successful
Movie or Series lookup is versioned as one complete payload (summary, artwork,
ratings, runtime, genres, studios, people, countries, and languages); upgrading
that contract reopens older automatic-scrape rows instead of leaving them
permanently Ready with only search-result fields.

Video Naming uses one versioned parser shared by title discovery, Series
expansion, and source indexing. It separates title and year, recognizes common
release-name resolution/source/codec tokens, and merges missing season/episode
facts from the nearest parent directories. Flat Movie files and Series-root
`SxxExx`/`Sxxx` files are supported. Filename-derived technical values are
stored as Naming hints and never replace NFO/remote metadata or probed stream
facts. A parser version increase marks each library-root binding for durable
re-discovery while the previous active projection remains readable.

The explicit Probe command enqueues or joins one high-priority durable job for
each active MediaSource with an available location, including sources that are
already Probed. One request is bounded to 256 sources. It returns `409 Conflict`
for an empty, unavailable, or oversized source set instead of implicitly indexing
an item, so Manual stage boundaries remain explicit.

The explicit `ExpandItem/{itemId}` command accepts Series items, and
`IndexMediaSources/{itemId}` accepts Movie or Episode items. Both re-run their
stage at the current item revision, preserve durable single-flight, and return
`409 Conflict` for an incompatible item type. The explicit
`FullScan/{libraryId}/{storageRootId}` command captures the current Library
profile version and uses the `library_storage_roots.id` binding as its durable
scope. It applies fixed Full command semantics without changing the Library's
persisted Manual policy. Validate may share the physical root inventory, while
Discover and target selection remain isolated to the selected binding.

## First-run setup

TJXY starts in a database-independent setup mode until a completed local
installation manifest exists. Open `http://127.0.0.1:8096/setup/` and complete the
four configuration steps: branding, database, network, and the initial administrator.
Before installation, client and administrator page URLs redirect to `/setup/`;
after installation, setup page URLs redirect to `/app/`.
The setup router is limited to loopback/private source addresses and does not expose
login, media, client, or administrator APIs.

Native builds always read and write the installation manifest at
`~/.config/tjxy/tjxy.toml`, creating the parent directory during setup.
`TJXY_CONFIG_FILE` explicitly overrides the manifest path. `TJXY_SETUP_DATA_DIR`
only controls setup data and SQLite files (default `./data`); it does not change the
manifest path. A minimal native first run is:

```bash
npm --prefix admin ci
npm --prefix admin run build
cargo build --release --locked -p tjxy-server --bin tjxy-server
TJXY_ADMIN_DIST_DIR=admin/dist ./target/release/tjxy-server
```

The repository-level `./tjxy-setup` launcher is the supported local setup entry
point. With no arguments it interactively selects whether TJXY itself runs as a
native host process or in Docker, followed by whether setup uses an existing
database or a managed PostgreSQL 18 container. The same choices can be supplied
without prompts:

```bash
./tjxy-setup --runtime local --database external --media /path/to/media
./tjxy-setup --runtime local --database postgres --media /path/to/media
./tjxy-setup --runtime docker --database external --media /path/to/media
./tjxy-setup --runtime docker --database postgres --media /path/to/media
```

Managed PostgreSQL credentials are generated once under `.tjxy`, retained with
owner-only permissions, and tested entirely by the server. Its database page is
therefore omitted from the browser setup without returning the password to the
browser. Native TJXY publishes managed PostgreSQL on `127.0.0.1:5433`; a Docker
TJXY reaches it by the Compose service name `postgres`, and PostgreSQL is not
published to the host in that mode. External database mode preserves the normal
SQLite, PostgreSQL, and MySQL setup page. From Docker, use
`host.docker.internal` to reach a database on the host rather than `localhost`;
the Compose definition supplies the Linux `host-gateway` mapping as well as
working with Docker Desktop.

By default, host configuration, application data, and media are mapped as
`.tjxy/config:/config`, `.tjxy/data:/data`, and `./media:/media`. Override them
with `--config-dir`, `--data-dir`, and `--media`; use `--media-mode ro` when TJXY
must not write to the media tree. The stable container path `/media` is exposed
to the administrator file browser. `./tjxy-setup stop`, `status`, and `logs`
reuse the saved runtime selection. Stop never removes the PostgreSQL volume.
Because a completed `tjxy.toml` stores its database endpoint, changing runtime
or database mode requires a new configuration directory rather than silently
rewriting an installation.

The default Dockerfile builds the frontend from its lockfile. Maintainers with
HeroUI Pro access can instead build the licensed frontend on the host and pass
only the generated, platform-independent `admin/dist` directory to Docker. The
key is used only by `hpsetup`; it is not included in the Docker build context or
image:

```bash
cd admin
HEROUI_KEY="$HeroPro" npx -y hpsetup@latest --auto
npm run build
cd ..
docker compose -f compose.external.yaml -f compose.prebuilt.yaml up --build
```

`compose.prebuilt.yaml` uses `admin/dist` as a named BuildKit context and fails
the image build when `index.html` is missing or empty. It never copies host
`node_modules` into the Linux image. End users of a published, prebuilt image do
not need HeroUI Pro credentials; only the maintainer producing `admin/dist`
does.

The supported launcher builds `admin/dist` on the host and then builds the TJXY
image from that output, so the Docker build itself does not need HeroUI Pro
credentials:

```bash
./tjxy-setup --runtime docker --database postgres
```

To build the frontend inside Docker directly instead, use the source-build
Compose path. That environment must independently have access to all licensed
HeroUI Pro artifacts:

```bash
docker compose up --build
```

The managed database name and user both default to `tjxy`. Its generated password
is intentionally not printed. Use external mode when an operator needs to supply
different PostgreSQL or MySQL ownership, TLS, or network settings through the
browser setup.

SQLite accepts a file under the configured data root. PostgreSQL and MySQL accept
separate host, port, database, username, password, and TLS fields; connection URLs
and passwords are never echoed by the setup API. Uploaded PNG, JPEG, WebP, and icon
branding assets are limited to 2 MiB and become durable application assets.

If the process stops after database mutation but before the local manifest is
completed, the next launch enters recovery mode. Recovery requires the same
installation ID, administrator username, and password, verifies the existing
administrator, and never resets a password or adopts a different database.

Environment variables take precedence over the completed manifest. In particular,
`TJXY_DATABASE_URL`, `TJXY_SERVER_ID`, `TJXY_CREDENTIAL_KEYRING`, `TJXY_BIND`, and
`TJXY_PUBLIC_ADDRESS` remain operator overrides. A completed installation whose
database is unavailable fails startup and never falls back to setup mode.

## Linux release

Each GitHub Release provides portable `linux-x86_64-gnu` and
`linux-aarch64-gnu` archives for systems with glibc 2.35 or newer. They contain
the server, the TUI launcher, and the required `admin/dist` assets; Rust,
Node.js, Docker, and this source repository are not required on the host. Alpine
and other musl-based distributions are not supported by these GNU/Linux assets.

```bash
sha256sum -c --ignore-missing SHA256SUMS
tar -xzf tjxy-v0.1.0-linux-x86_64-gnu.tar.gz
cd tjxy-v0.1.0-linux-x86_64-gnu
./tjxy
```

The TUI starts and manages the sibling `tjxy-server`; use `1` to start, `2` to
stop, and `3` to restart it. Press `4` to rebuild the frontend and release
server/TUI with the existing lockfiles, then restart the server automatically.
This action never runs `hpsetup` or changes frontend dependencies; if a build
fails, the running server is left untouched. It writes its PID and log under `data/`. The server
defaults to the bundled `admin/dist`, `./data` for setup data, and the platform
configuration directory for `tjxy.toml`. Copy `.env.example` to `.env` only when
runtime overrides are needed; do not commit credentials or database URLs.

Maintainers publish a release by pushing a matching version tag:

```bash
git tag -a v0.1.0 -m 'v0.1.0'
git push origin v0.1.0
```

The `Release` workflow verifies the frontend and Rust workspace, packages both
Linux architectures, generates `SHA256SUMS`, and creates or updates the GitHub
Release. To rerun an existing tag from the Actions UI, choose **Release**, select
**Run workflow**, and enter that tag.

## Development

The workspace requires Rust 1.88 or newer.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Database contracts default to an isolated in-memory SQLite database. Migrated
cross-database contracts can be run against a disposable PostgreSQL or MySQL
database by setting `TJXY_TEST_DATABASE_URL`; each test connection creates its
own `tjxy_test_*` database or schema so parallel tests do not share migrations
or fixtures. Databases are retained for failed-test inspection and should only
be created in a disposable local or CI database.

```bash
TJXY_TEST_DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/tjxy_test \
  cargo test -p tjxy-test-support -p tjxy-db --tests
TJXY_TEST_DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/tjxy_test \
  cargo test -p tjxy-application --tests
TJXY_TEST_DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/tjxy_test \
  cargo test -p tjxy-import --tests
TJXY_TEST_DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/tjxy_test \
  cargo test -p tjxy-server --tests
RUST_TEST_THREADS=1 \
TJXY_TEST_DATABASE_URL=mysql://root:tjxy@127.0.0.1:3306/tjxy_test \
  cargo test -p tjxy-test-support -p tjxy-db -p tjxy-application \
    -p tjxy-import -p tjxy-server --tests --locked
```

The `postgres-contracts` CI job runs all database, application, import, and
server contracts on pinned PostgreSQL 17. This is the release gate for
workspace tests that create a database fixture through `tjxy-test-support`.
The `mysql-contracts` job runs the same database, application, import, and server
packages on pinned MySQL 8.4. It sets `RUST_TEST_THREADS=1` because MySQL can invalidate
prepared statements while another isolated test database is applying DDL; every test
still receives its own database and the full package contracts remain enabled.
