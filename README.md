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
management, plus the Google Drive/OneDrive Personal OAuth and root-selection flows;
the compatibility matrix in
[`docs/api-parity.md`](docs/api-parity.md) remains the authoritative record of
implemented behavior.

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
stored in `sessionStorage`; logout clears that browser session but does not revoke
the server-side token.

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
using non-loopback plain HTTP or private literal IP addresses are rejected.
Loopback HTTP remains available for local development providers. DNS names still
require deployment-level outbound allowlisting because their resolved addresses
can change after configuration validation.

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
Deployments should restrict outbound network access so the configured provider
URL can reach only approved AI endpoints.

## Run the management TUI

The repository includes a Rust terminal console for local service management and
status checks:

```bash
cargo run -p tjxy-tui
```

It discovers TJXY server processes from this workspace even when they were
started outside the TUI, and shows their PID/port state, build artifacts,
database backend, masked `TJXY_*` configuration, and recent server logs. Actions
include starting or stopping a server process launched by the TUI, building the
server or Admin application, and running focused checks.

The database view follows `TJXY_DATABASE_URL` when it is available. Otherwise,
it infers PostgreSQL on its default port from a running server's live database
connections and falls back to the server's default SQLite configuration. SQLite
maintenance actions
require the `sqlite3` CLI and are hidden for PostgreSQL and in-memory databases.
The TUI only stops a server that it started and recorded in
`target/tjxy-server.pid`. Process inspection and signalling currently target
local macOS and Linux environments and require `ps` and `lsof`.

The default bind address is `127.0.0.1:8096`; override it with `TJXY_BIND`.
The default database is `sqlite://tjxy.db?mode=rwc`; override it with
`TJXY_DATABASE_URL`. Original image assets default to `./data/assets`; override
that root with `TJXY_ASSETS_DIR`. The asset store validates actual image format,
encoded size, dimensions, pixel count, and decoder allocation before an
fd-confined atomic SHA-256 write; JPEG, PNG, GIF, WebP, and BMP originals are
accepted. Lazy Expand/Source requests wait up to 2500 ms
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
starts an account-scoped inventory worker and shares the serial Probe worker. A
server-side folder picker can be enabled by saving one or more media browser
roots in **Admin > System settings**, or by setting `TJXY_MEDIA_BROWSER_ROOTS`
to a platform path-list of allowed parent directories (colon-separated on Unix
and semicolon-separated on Windows). An explicitly present environment variable,
including an empty value, takes precedence over the database setting; database
changes apply after TJXY restarts. The Admin API exposes opaque root IDs and
relative paths only; every selection is canonicalized again before it can be
attached to a Library, and symlinks cannot escape an allowed root.

A Library's metadata source is independent of its scan profile. The default
`automatic_scrape` mode reads local NFO and artwork first and uses configured
remote providers only to fill missing metadata. The `local_only` mode reads NFO
and naming metadata without invoking remote providers. Supported local artwork
names include `poster`, `folder`, `cover`, and `*-poster` for Primary images,
plus `fanart` and `backdrop` for Backdrop images, using JPEG, PNG, GIF, WebP, or
BMP extensions. Malformed NFO and artwork produce work warnings and fall back to
the remaining permitted metadata sources.

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
the published children into the same scan lifecycle. Hybrid background refresh
selects at most 20 unexpanded Series from the Lazy title layer, ranks
administrator pins, watching, engaged NextUp, and favorite signals ahead of
recently added items, and schedules the same durable Expand work at lower
priority than interactive requests. Engaged NextUp requires one user to have
both a played Episode and an unplayed, unstarted Episode in the Series' active
structure publication, so retired projections and unrelated users cannot create
a false signal. The chosen batch is staged under the parent FullScan and remains
fixed across worker retries, so one refresh cannot advance through successive
batches. A production-process TCP smoke verifies that refresh completes the
low-priority expansion before any Series browse. Signals on existing Episodes
are attributed to their owning Series. Administrator-pinned background
candidates are stored on the library membership and rank ahead of derived
signals. Administrators can page, pin, and unpin them from the Libraries screen;
changing away from `background` keeps the preference dormant, and unpinning does
not cancel work that was already submitted. The production scheduler
periodically submits the same policy-aware scan at the lowest queue priority and
delays missed ticks instead of creating a catch-up burst.

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
TJXY_TEST_DATABASE_URL=mysql://root:tjxy@127.0.0.1:3306/tjxy_test \
  cargo test -p tjxy-test-support -p tjxy-db --tests
```

The `postgres-contracts` CI job runs all database, application, import, and
server contracts on pinned PostgreSQL 17. This is the release gate for
workspace tests that create a database fixture through `tjxy-test-support`.
The independent `mysql-smoke` job runs only the test-support and database
contracts on pinned MySQL 8.4, is allowed to fail, and is not a release gate or
a production-support claim.
