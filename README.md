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
HTTP server, and a terminal diagnostic console. It can run natively or as a
Docker Compose deployment with either a managed PostgreSQL instance or an
existing database.

> [!IMPORTANT]
> TJXY is under active development and does not yet implement the complete
> Jellyfin API. Check the [API compatibility matrix](docs/api-parity.md) before
> depending on a specific Jellyfin client or endpoint.

## Features

- **Unified media catalog:** model titles independently from their physical
  files, alternate copies, subtitles, and storage locations.
- **Local and cloud storage:** browse root-confined local files, Google Drive,
  Shared Drives, and OneDrive Personal without exposing provider credentials to
  the browser.
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

## Quick Start with Docker

The recommended end-user deployment pulls a published image from GHCR. The
launcher creates persistent storage, starts the Compose services, and waits until
they are healthy. Source builds are available for maintainers who need to build
from a checkout.

### Requirements

- Docker Engine with a recent Compose v2 plugin
- Source builds additionally need Node.js 22.12 or newer, npm, and prepared
  frontend dependencies in `admin/node_modules`.

### Published Image

Release CI publishes multi-platform images to
`ghcr.io/youtonghy/tjxy:latest` and a version tag such as
`ghcr.io/youtonghy/tjxy:0.1.0`. Pull the image through the launcher without
running a frontend build:

```bash
TJXY_IMAGE=ghcr.io/youtonghy/tjxy:latest ./tjxy-setup \
  --runtime docker \
  --database postgres \
  --media /path/to/media
```

The equivalent flag is `--image ghcr.io/youtonghy/tjxy:latest`. If the GHCR
package is private, authenticate first with `docker login ghcr.io`. Published
image users do not need Node.js, npm, or `HEROUI_KEY`.

For a pinned release, replace `latest` with an immutable version tag. Rerun the
same command to pull a newer image and recreate the application container.

### Source Build

HeroUI Pro is a licensed build dependency. Maintainers must store `HEROUI_KEY`
as a GitHub Actions repository secret before publishing a release. For a local
source build, install it without committing the key:

```bash
cd admin
HEROUI_KEY="<your-key>" npx -y hpsetup@latest --auto
npm run build
cd ..
```

The `Release` workflow uses that secret only while producing `admin/dist`; it
then publishes the completed image. The secret is never needed on a deployment
host. After the first release, set the GHCR package visibility to public if
anonymous pulls are required and the HeroUI Pro license permits that
distribution. To build from the checkout instead of pulling an image, omit
`TJXY_IMAGE` and `--image` from the launcher commands below.

For a manual release, open **Actions > Release > Run workflow** and enter a
version such as `0.2.0` or `v0.2.0`. CI builds the current `main` branch and
creates the `v0.2.0` tag, GitHub Release, portable archives, and container image.
An existing tag is not required. Pushing a matching `vX.Y.Z` tag remains
supported and keeps the stricter Cargo workspace version check.

### Managed PostgreSQL

Run the interactive launcher:

```bash
./tjxy-setup
```

Choose **Docker** and **managed PostgreSQL**, or provide the choices directly:

```bash
TJXY_IMAGE=ghcr.io/youtonghy/tjxy:latest ./tjxy-setup \
  --runtime docker \
  --database postgres \
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
TJXY_IMAGE=ghcr.io/youtonghy/tjxy:latest ./tjxy-setup \
  --runtime docker \
  --database external \
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
| `--media PATH` | `./media` | `/media`, visible to the library browser |
| `--media-mode ro` | `rw` | Mount media read-only |
| `--port PORT` | `8096` | Publish the TJXY HTTP service |
| `--image IMAGE` | source build | Pull a published image and skip frontend build |

TJXY publishes only on `127.0.0.1` by default. Keep this setting when using an
SSH tunnel or reverse proxy on the same host. For direct LAN access, explicitly
publish on every interface and protect the port with a firewall:

```bash
TJXY_PUBLISH_HOST=0.0.0.0 \
TJXY_IMAGE=ghcr.io/youtonghy/tjxy:latest \
./tjxy-setup \
  --runtime docker \
  --database postgres \
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
new version tag. The launcher pulls the image before recreating TJXY. For source
deployments, stop TJXY, update the checkout, refresh frontend dependencies when
the lockfile changes, and rerun the source-build command.

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

Complete setup at `http://127.0.0.1:8096/setup/`. The installation manifest is
stored at the platform configuration path by default; set `TJXY_CONFIG_FILE` to
use an explicit location.

## Linux Release Archive

[GitHub Releases](https://github.com/youtonghy/TJXY/releases) provide
`linux-x86_64-gnu` and `linux-aarch64-gnu` archives for systems with glibc 2.35
or newer. The archives include the web assets, server, and TUI, so they do not
require a Rust or Node.js toolchain.

```bash
sha256sum -c --ignore-missing SHA256SUMS
tar -xzf tjxy-v0.1.0-linux-x86_64-gnu.tar.gz
cd tjxy-v0.1.0-linux-x86_64-gnu
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
