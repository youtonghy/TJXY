# TJXY Media Catalog

TJXY models logical media independently from storage and exposes the resulting catalog through Jellyfin-compatible interfaces.

## Language

**CatalogItem**:
A logical work or hierarchy node such as a Movie, Series, Season, or Episode. Its identity and UserData do not depend on a file path.
_Avoid_: BaseItem, media file, path item

**MediaSource**:
A concrete playable version of a CatalogItem, such as a 4K encode or director's cut. Container and stream metadata belong to this concept; its persisted presentation key is the stable Jellyfin MediaSourceId across ordinary re-indexing. PlaybackInfo may return multiple sources; the server applies a formal default-source ranking when the client omits a selection. Client version-picker UI is not a release gate.
_Avoid_: File, path, location, single-source-only DTO

**PresentationKey**:
The immutable external MediaSourceId assigned on first publication and preserved through re-index when stable object identity, trusted content identity, legacy mapping, or administrator confirmation identifies the same source.
_Avoid_: Path hash, current revision

**MediaLocation**:
One actual local or cloud location that can supply the bytes of a MediaSource. Multiple locations may mirror the same source.
_Avoid_: MediaSource, direct URL

**StorageObject**:
A provider-owned file or directory record synchronized from a StorageBackend. It describes storage facts without deciding what media the object represents.
_Avoid_: CatalogItem, media entry

**StorageBackend**:
The provider-neutral interface for object lookup, child listing, change listing, and byte-range reads. v1 cloud backends are Google Drive and OneDrive Personal; OneDrive Business and SharePoint are out of v1 scope.
_Avoid_: Scanner, media provider, rclone requirement

**Storage Sync**:
The process that synchronizes StorageObject state and opaque provider cursors for a full root or requested subtree. Storage Sync does not classify titles, resolve metadata, or probe media.
_Avoid_: Media Scan, library scan

**Strict Lazy**:
The locked Google Drive initial-object strategy: only the title layer is inventoried at bind time; deeper subtrees are materialized by scoped Storage Sync on access before any Media Scan stage. Full-tree Inventory First is not the default path.
_Avoid_: Inventory First default, Media Scan listing the backend

**Storage Change Reconciler**:
The leased, at-least-once durable outbox consumer that translates committed StorageObject changes into MediaLocation availability, item revisions, Probe Stale/detach decisions, catalog generation, and cache invalidation. It advances only a contiguous reconciled sync watermark.
_Avoid_: Storage adapter, Media Scan

**SyncRevision**:
A monotonic StorageRoot revision assigned to each committed sync batch. A Media job may consume a scoped result only after child indexing and the reconciled watermark both cover that revision.
_Avoid_: Provider cursor, catalog generation

**Media Scan**:
The staged process that classifies StorageObjects, resolves metadata, expands structure, discovers sources, and optionally probes media.
_Avoid_: Storage Sync, file sync

**Structure Expansion**:
Atomic publication of a Series' complete Season and Episode subtree from already synchronized StorageObjects.
_Avoid_: Movie source indexing, Storage Sync, Probe

**Source Indexing**:
Atomic discovery and publication of a Movie or Episode's MediaSources, MediaLocations, and subtitles without probing container streams. Series Expansion already indexes its child Episodes; a later Episode task runs only for missing, independent, stale, or explicitly rebuilt sources.
_Avoid_: Structure Expansion, Media Probe

**ActivePublication**:
The fully validated structure or source result currently visible to catalog queries. Workers may build a replacement in bounded staging batches, but only a short pointer-switch transaction makes it visible.
_Avoid_: Partial scan result, Redis generation

**WorkJob**:
A persistent, leased, revision-bound unit of Storage Sync, Structure Expansion, Source Indexing, Probe, or import work. Concurrent callers join the same active job and crashed workers may resume it.
_Avoid_: In-memory mutex, HTTP request state

**Media Probe**:
A bounded read of one MediaLocation's container head or tail that determines MediaSource stream metadata without decoding frames. Other locations may reuse the result only when their content identity is trusted to match.
_Avoid_: Transcode, full scan

**ContentIdentity**:
Trusted evidence that multiple MediaLocations contain the same playable bytes, based on a provider checksum, verified identity, or administrator confirmation. It is required for cross-location reuse, not for probing one location by itself.
_Avoid_: File name match, path match

**LibraryMembership**:
The explicit association that makes a shared CatalogItem and its descendants visible in a Library without duplicating the CatalogItem.
_Avoid_: File path ownership

**PresenceState**:
A StorageObject fact distinguishing Present, TemporarilyUnavailable, and ConfirmedAbsent. Only confirmed absence can drive media detach through the Storage Change Reconciler.
_Avoid_: MediaLocation health, HTTP status

**IdentityQuality**:
The strength of a StorageObject identity. Cloud provider IDs and reliable filesystem file IDs are stable; canonical-path fallback is weak and may only create relink candidates after a move.
_Avoid_: CatalogItem identity, title match

**Subtitle**:
A source subtitle associated with a MediaSource and backed by a StorageObject. PlaybackInfo exposes it as an external MediaStream with a stable delivery index and an authenticated TJXY OpenAPI 12 subtitle URL; TJXY transfers only the original format without timeline rewriting.
_Avoid_: Embedded stream extraction, rendered subtitle, converted subtitle

**StreamIndexMap**:
The persistent MediaSource-wide mapping from stable embedded/external stream identity to Jellyfin delivery index, with a separate container index where needed. Tombstoned delivery indexes are never reused.
_Avoid_: Array position, transient ffprobe order

**UserData**:
A user's playback position, played state, play count, and favorite state bound to a stable CatalogItem. Every committed mutation increments that user's SQL revision in the same transaction so Redis projections cannot hide it.
_Avoid_: Item metadata, session state

**MetadataProvider**:
A process-local Rust provider that resolves basic title metadata and images into SQL with provenance. v1 remote support is TMDb only; NFO/local images are importers, not runtime sources of truth. There is no Jellyfin/Emby plugin host.
_Avoid_: Plugin runtime, TVDB remote in v1, media-folder as SoT

**EffectiveScanPolicy**:
The SQL-persisted object selection, metadata, expansion, and probe policies used by Admin, VirtualFolders, and schedulers. A profile name is a preset label, not a hidden runtime default.
_Avoid_: Process-local setting, implicit Lazy behavior

**CatalogGeneration**:
The committed SQL catalog revision used to isolate Redis results and publish atomic catalog changes.
_Avoid_: Storage sync cursor, cache version

**AssetBlob**:
A content-addressed image stored once and referenced by CatalogItems.
_Avoid_: Item image file, media-folder image

**Proxy Stream**:
TJXY's authenticated, backpressured forwarding of an original local or cloud byte range to a client without exposing upstream credentials or URLs.
_Avoid_: Redirect, video cache, transcode
