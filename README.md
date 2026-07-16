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
- a pinned direct-play `PlaybackInfo` JSON golden; and
- a root-confined `FilesystemBackend` with bounded byte-range streaming; and
- an Axum server skeleton with honest public system information and health routes.

The server is not yet a complete Jellyfin implementation and no React admin is
available. The compatibility matrix in [`docs/api-parity.md`](docs/api-parity.md)
remains the authoritative record of implemented behavior.

## Run the HTTP skeleton

`TJXY_SERVER_ID` is required so the externally visible server identity remains
stable across restarts. Generate and persist one UUID in deployment config, then
run:

```bash
TJXY_SERVER_ID=018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11 \
TJXY_SERVER_NAME="Living Room" \
cargo run -p tjxy-server
```

The default bind address is `127.0.0.1:8096`; override it with `TJXY_BIND`.
Set `TJXY_PUBLIC_ADDRESS` only from trusted deployment configuration when the
discovery response should advertise an address. Readiness remains false until
future dependency initialization explicitly marks the shared state ready.
Available routes are `GET /System/Info/Public`, `GET /System/Ping`,
`GET /health/live`, and `GET /health/ready`.

## Development

The workspace requires Rust 1.85 or newer.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```
