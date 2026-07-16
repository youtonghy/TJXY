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
- versioned catalog/user/probe cache key contracts;
- a pinned direct-play `PlaybackInfo` JSON golden;
- a root-confined `FilesystemBackend` with bounded byte-range streaming;
- Argon2id local authentication with durable, digest-only session storage; and
- an Axum server with L0 discovery, L1 login/current-user, and health routes.

The server is not yet a complete Jellyfin implementation and no React admin is
available. The compatibility matrix in [`docs/api-parity.md`](docs/api-parity.md)
remains the authoritative record of implemented behavior.

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

The default bind address is `127.0.0.1:8096`; override it with `TJXY_BIND`.
The default database is `sqlite://tjxy.db?mode=rwc`; override it with
`TJXY_DATABASE_URL`. The two bootstrap administrator variables must both be set
for a new database, and the password must not be empty. They create an
administrator only when the database has no users; use one server replica for
this first startup, then remove them from deployment configuration.
Legacy `Emby` schemes and X-Emby/X-MediaBrowser token aliases are enabled by
default; set `TJXY_LEGACY_AUTH=false` to require canonical MediaBrowser auth.

Set `TJXY_PUBLIC_ADDRESS` only from trusted deployment configuration when the
discovery response should advertise an address. Readiness becomes true only
after the database connection, migrations, and authentication service succeed,
and each readiness request probes the database connection.
Available compatibility routes now include `GET /System/Info/Public`,
`GET /System/Ping`, `POST /Users/AuthenticateByName`, and `GET /Users/Me`, plus
`GET /health/live` and `GET /health/ready`.

## Development

The workspace requires Rust 1.85 or newer.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```
