# TJXY

TJXY is a Jellyfin-compatible media catalog server that separates logical
media identity from the local or cloud objects that provide its bytes.

The repository is being implemented incrementally from [`PLAN.md`](PLAN.md).
The current foundation includes:

- path-independent `CatalogItem`, `MediaSource`, and `MediaLocation` contracts;
- a provider-neutral streaming `StorageBackend` interface;
- the initial SQLite/PostgreSQL-compatible SeaORM migration set;
- versioned catalog/user/probe cache key contracts;
- a pinned direct-play `PlaybackInfo` JSON golden; and
- a root-confined `FilesystemBackend` with bounded byte-range streaming.

No runnable Jellyfin server or React admin is available yet. The compatibility
matrix in [`docs/api-parity.md`](docs/api-parity.md) remains the authoritative
record of implemented behavior.

## Development

The workspace requires Rust 1.85 or newer.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```
