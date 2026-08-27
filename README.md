# TJXY

<p align="center">
  <img src="admin/public/brand/tjxy-mark.webp" alt="TJXY" width="128">
</p>

<p align="center">
  A self-hosted, Jellyfin-compatible media catalog for local and cloud libraries.
</p>

<p align="center">
  <strong>English</strong> | <a href="README.zh-CN.md">简体中文</a>
</p>

TJXY separates a media title from the files and storage providers that supply
it. One movie, episode, or track can therefore keep a stable catalog identity
while its playable sources live on local disks, Google Drive, or OneDrive.

The project includes a browser media client, an administrator console, a Rust
HTTP server, and a terminal diagnostic console. The recommended installation
pulls a published Docker image. Native builds, source builds, and Linux
archives remain available for maintainers.

> [!IMPORTANT]
> TJXY is under active development and does not yet implement the complete
> Jellyfin API. Check the [API compatibility matrix](docs/api-parity.md) before
> depending on a specific Jellyfin client or endpoint.

## Features

- **Unified media catalog:** model titles independently from their physical
  files, alternate copies, subtitles, and storage locations.
- **Local and cloud storage:** browse root-confined local files, Google Drive,
  Shared Drives, and OneDrive Personal without exposing provider credentials to
  the browser. On Unix, a verified device-number change for the same local root
  inode preserves existing storage identities across restarts.
- **Metadata pipeline:** discover movies, series, episodes, and music from file
  names and local NFO/artwork, with optional TMDb, MusicBrainz, and TheAudioDB
  enrichment.
- **Direct playback:** prepare session-scoped playback URLs, stream bounded byte
  ranges, select subtitles and sources, and retain play state, favorites, and
  viewing progress.
- **Web applications:** use the responsive `/app/` media client and `/admin/`
  administration console from the same server.
- **Library operations:** create libraries, bind storage roots, choose scan
  policies, run durable scan tasks, inspect progress, and retry failed work.
- **Users and access:** Argon2id passwords, server-side sessions, QR sign-in,
  device/session revocation, API keys, and administrator-managed users.
- **Optional AI assistant:** connect an OpenAI-compatible provider for media
  discovery grounded in the authenticated user's visible catalog and history.
- **Multiple databases:** use SQLite, PostgreSQL, or MySQL; PostgreSQL can be
  provisioned automatically by `tjxy-setup`.
- **Operational tooling:** health checks, structured rotating logs, a diagnostic
  TUI, Docker health checks, and persistent installation configuration.

## Quick Start

The recommended end-user installation pulls the published Docker image. The
launcher creates persistent storage, starts Compose, and waits until the
services are healthy. You do not need Node.js, Rust, or `HEROUI_KEY` on the
deployment host.

### Requirements

- Docker Engine with a recent Compose v2 plugin

### Published Docker Image

Current release: `ghcr.io/youtonghy/tjxy:0.0.1` (`linux/amd64` and
`linux/arm64`). The `latest` tag points at the same image. Pin the version tag
in production.

The repository and GHCR package are currently private, so authenticate before
the first pull:

```bash
docker login ghcr.io
./tjxy-setup \
  --runtime docker \
  --database postgres \
  --image ghcr.io/youtonghy/tjxy:0.0.1 \
  --media /path/to/media
```

`TJXY_IMAGE=ghcr.io/youtonghy/tjxy:0.0.1` is equivalent to `--image`. Rerun the
same command to pull a newer tag and recreate the application container.

### Managed PostgreSQL

The command above provisions PostgreSQL automatically. You can also run the
interactive launcher and pass the published image:

```bash
./tjxy-setup --image ghcr.io/youtonghy/tjxy:0.0.1
```

Choose **Docker** and **managed PostgreSQL**, or provide the choices directly:

```bash
./tjxy-setup \
  --runtime docker \
  --database postgres \
  --image ghcr.io/youtonghy/tjxy:0.0.1 \
  --media /path/to/media \
  --port 8096
```

The launcher stores configuration under `.tjxy/config` and application data
under `.tjxy/data` unless other paths are supplied. On Linux, run it as the
account that should own those directories. Open
`http://127.0.0.1:8096/setup/` after startup and create the first administrator.

Managed PostgreSQL credentials are generated locally in
`.tjxy/postgres-password`, are never printed, and are not sent to the browser.
The database is available only on the Compose network. Its data remains in the
`tjxy-setup_tjxy-postgres` Docker volume when the application is stopped.

### External Database

Use an existing SQLite, PostgreSQL, or MySQL database by selecting external mode:

```bash
./tjxy-setup \
  --runtime docker \
  --database external \
  --image ghcr.io/youtonghy/tjxy:0.0.1 \
  --media /path/to/media
```

Enter the database settings in the browser setup. A database running on the
Docker host must be addressed as `host.docker.internal`, not `localhost`. The
Compose configuration adds the Linux `host-gateway` mapping and also works with
Docker Desktop.

### Storage and Ports

| Option | Default | Container path / behavior |
| --- | --- | --- |
| `--config-dir PATH` | `.tjxy/config` | `/config`, including `tjxy.toml` |
| `--data-dir PATH` | `.tjxy/data` | `/data`, including assets and logs |
| `--media PATH` | `./media` | `/media`, available for media libraries and STRM targets |
| `--media-mode ro` | `rw` | Mount media read-only |
| `--port PORT` | `8096` | Publish the TJXY HTTP service |
| `--image IMAGE` | unset (source build) | Recommended: `ghcr.io/youtonghy/tjxy:0.0.1`. Omit only when building from a checkout |

TJXY publishes only on `127.0.0.1` by default. Keep this setting when using an
SSH tunnel or reverse proxy on the same host. For direct LAN access, explicitly
publish on every interface and protect the port with a firewall:

```bash
TJXY_PUBLISH_HOST=0.0.0.0 ./tjxy-setup \
  --runtime docker \
  --database postgres \
  --image ghcr.io/youtonghy/tjxy:0.0.1 \
  --port 8096
```

Use a TLS reverse proxy before exposing TJXY outside a trusted network.

### Operations and Upgrades

```bash
./tjxy-setup status
./tjxy-setup logs
./tjxy-setup stop
```

`stop` removes containers and the Compose network but preserves bind-mounted
files and the managed PostgreSQL volume. To update a published-image deployment,
back up the database and host directories, then rerun the same command with the
new version tag. The launcher pulls the image before recreating TJXY.

Do not run `docker compose down --volumes` unless the managed PostgreSQL database
is intentionally being deleted.

## Native Installation

Native builds require Rust 1.88 or newer, Node.js 22.12 or newer, npm, and the
prepared frontend dependencies. Docker is additionally required when native
TJXY uses managed PostgreSQL.

Use the launcher for a guided local installation:

```bash
./tjxy-setup --runtime local --database external --media /path/to/media
```

To let the launcher provision PostgreSQL in Docker while TJXY runs natively:

```bash
./tjxy-setup \
  --runtime local \
  --database postgres \
  --media /path/to/media \
  --postgres-port 5433
```

For a manual build:

```bash
npm --prefix admin run build
cargo build --release --locked -p tjxy-server --bin tjxy-server
TJXY_ADMIN_DIST_DIR=admin/dist ./target/release/tjxy-server
```

Local builds report product version `0.0.0`. To stamp a build manually, pass the
same version to both build pipelines:

```bash
VITE_TJXY_VERSION=0.2.0 npm --prefix admin run build
TJXY_BUILD_VERSION=0.2.0 cargo build --release --locked -p tjxy-server --bin tjxy-server
```

Optionally set `VITE_TJXY_COMMIT` to show the build commit hash on the administrator About page.

The release workflow injects its validated release version automatically.

Complete setup at `http://127.0.0.1:8096/setup/`. The installation manifest is
stored at the platform configuration path by default; set `TJXY_CONFIG_FILE` to
use an explicit location.

Work completed by the running version is retained for 7 days by default. Set
`TJXY_WORK_HISTORY_RETENTION_DAYS` to a value from 1 through 3650, or set
`TJXY_WORK_HISTORY_RETENTION_ENABLED=false` to suspend retention. The retention
worker enrolls terminal work left by earlier versions in batches of up to 1,000,
then clears at most 100 jobs per short transaction. PostgreSQL and SQLite keep
the work-claim index limited to pending and running jobs; PostgreSQL builds its
replacement index concurrently. Legacy processed outbox rows are removed in
bounded background batches; failed storage events are kept for seven days after
they become dead letters.

Deleting rows does not immediately shrink existing database files. After a large
history cleanup, run `VACUUM (ANALYZE)` during normal operations. Reclaiming file
system space requires a separately planned maintenance window for `VACUUM FULL` or
an online PostgreSQL reorganization tool such as pg_repack.

### Jellyfin Media Player

Jellyfin Media Player 1.11 and newer loads a server-hosted Jellyfin Web client.
TJXY can mount an operator-supplied Jellyfin Web distribution at `/web/` while
keeping the TJXY application at `/app/` and the administrator UI at `/admin/`:

```bash
TJXY_ADMIN_DIST_DIR=admin/dist \
TJXY_JELLYFIN_WEB_DIST_DIR=/usr/share/jellyfin/web \
./target/release/tjxy-server
```

The configured directory must contain `index.html`. Jellyfin Web is GPL-2.0
licensed and is not bundled into TJXY's MIT-licensed release archives. For local
development, TJXY automatically mounts `data/jellyfin-web` when that directory
contains `index.html`; `TJXY_JELLYFIN_WEB_DIST_DIR` overrides this location.
When neither source is available, the server root continues to open `/app/`.
The current compatibility baseline is Jellyfin Web 10.11.11; newer client
releases need to be validated before changing the mounted distribution.

TJXY implements the Jellyfin direct-play subset rather than remuxing or
transcoding. Playback negotiation accepts optional or empty POST bodies and
ignores client-specific query hints that Jellyfin model binding would ignore.
The original-file endpoints accept Jellyfin's lowercase `/videos` and `/audio`
forms, their optional container suffix, and authenticated requests without a
`MediaSourceId` by selecting the first authorized playable source. A container
suffix such as `stream.mp4` is only a route alias: it neither converts the file
nor changes the response MIME type. Direct file delivery also applies when the
`static` hint is absent or false; ticket-only URLs retain an explicit
ticket-bound media source. Transcoding-shaped query parameters on these
progressive endpoints are accepted as compatibility hints but still return the
original bytes and actual MIME type.

The TJXY browser client intentionally limits direct playback to containers that
the browser can handle reliably and therefore does not advertise MKV. Jellyfin
Media Player's native MPV path can play compatible MKV sources returned by the
server. Requests for transcoding fall back to the same original-file delivery;
TJXY does not expose HLS manifests or claim that an unsupported client codec can
decode those bytes.

## Linux Release Archive

[GitHub Releases](https://github.com/youtonghy/TJXY/releases) provide
`linux-x86_64-gnu` and `linux-aarch64-gnu` archives for systems with glibc 2.35
or newer. The archives include the web assets, server, and TUI, so they do not
require a Rust or Node.js toolchain.

```bash
sha256sum -c --ignore-missing SHA256SUMS
tar -xzf tjxy-v0.0.1-linux-x86_64-gnu.tar.gz
cd tjxy-v0.0.1-linux-x86_64-gnu
./tjxy
```

The terminal console can start, stop, restart, and inspect the bundled server.
Press `g` to switch between English and Chinese.

## First-Run Setup

Before an installation manifest exists, TJXY serves only the setup application.
The wizard configures branding, database access when required, network settings,
and the initial administrator. After completion:

- `/app/` serves the ordinary media client;
- `/admin/` serves the administrator console;
- `/health/ready` reports application and database readiness; and
- setup URLs redirect to the installed application.

A completed configuration contains the installation identity and database
endpoint. Do not reuse the same `tjxy.toml` when switching between native and
Docker runtimes or between managed and external databases. `tjxy-setup` detects
this situation and requires a new `--config-dir` instead of silently rewriting
an installation.

## Build from Source

Source builds are for maintainers. They require Node.js 22.12 or newer, npm,
prepared frontend dependencies in `admin/node_modules`, and Rust 1.88 or newer
when compiling the server. HeroUI Pro is a licensed build dependency. Store
`HEROUI_KEY` as a GitHub Actions repository secret before publishing a release.
For a local source build, install it without committing the key:

```bash
cd admin
HEROUI_KEY="<your-key>" npx -y hpsetup@latest --auto
npm run build
cd ..
```

The `Release` workflow uses that secret only while producing `admin/dist`; the
published image and Linux archives already contain the built frontend. The
secret is never needed on a deployment host. To build Docker from the checkout
instead of pulling GHCR, omit `--image` and `TJXY_IMAGE`:

```bash
./tjxy-setup --runtime docker --database postgres --media /path/to/media
```

For a manual release, open **Actions > Release > Run workflow** and enter a
version such as `0.0.1` or `v0.0.1`. CI builds the current `main` branch and
creates the tag, GitHub Release, portable archives, and container image. An
existing tag is not required. Pushing a matching `vX.Y.Z` tag remains supported
and keeps the stricter Cargo workspace version check.

## Development

The backend is a Rust 2024 workspace built on Axum and SeaORM. The frontend uses
React 19, HeroUI v3, Tailwind CSS v4, and Vite.

```bash
# Frontend
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build

# Rust workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Database tests use SQLite by default. Set `TJXY_TEST_DATABASE_URL` to a
disposable PostgreSQL or MySQL instance when running cross-database contracts.
Each test creates an isolated database or schema.

## Documentation

- [API compatibility matrix](docs/api-parity.md)
- [Theme development](docs/themes.md)
- [Implementation plan](PLAN.md)
- [Chinese README / 中文说明](README.zh-CN.md)

## License

TJXY is licensed under the [MIT License](LICENSE).
