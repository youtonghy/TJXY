# Cloud Multi-Source Playback Contract Design

## Status

Approved design for closing the deterministic cloud and multi-source portion of the
Jellyfin playback release gate in `PLAN.md` section 18.10. This slice strengthens test
evidence only. It does not change the public API, playback policy, storage model, or
production runtime configuration.

## Context

TJXY already has three relevant but separate layers of evidence:

- an API-level, single-source `PlaybackInfo` response golden;
- a real-process Filesystem TCP chain covering authentication, browse, item detail,
  `PlaybackInfo`, full and ranged delivery, subtitle delivery, playstate, Resume, and
  stable identities across re-index;
- in-process cloud tests covering local TJXY URLs, one Range GET, subtitle bytes, bounded
  Probe reads, availability transitions, and selected secret-exclusion assertions.

Those tests do not prove the complete section 18.10 contract. No cloud request or HTTP
response golden exists. Every server `PlaybackInfo` test exposes one MediaSource, so the
server boundary does not prove that a multi-source list remains complete while its default
source is usable. The current cloud fixture also inserts an already-Probed publication
directly, which bypasses Source Index, Probe, publication replacement, and re-index identity
reuse.

## Decision

Add one deterministic, real-TCP cloud multi-source contract to the existing server test
harness. Build its media graph through the production `SourceIndexService`, Probe both
sources through `ProbeService` and the existing provider-neutral in-memory cloud backend,
select the default through the authenticated Admin playback-policy route, and then exercise
the advertised Jellyfin routes over TCP.

Pin request and response goldens for both the existing Filesystem contract and the new
cloud multi-source contract. Dynamic identifiers are normalized by semantic identity from
the database, never by response array position. Re-index must create a newer active
publication while preserving presentation IDs, source ordering, playback policy, subtitle
delivery index, and all client URLs.

The test uses Axum 0.8's normal `axum::serve(TcpListener, Router)` path with a one-shot
graceful-shutdown future. Shutdown and task completion are bounded by a Tokio timeout so a
failed assertion cannot leave an accepting task running indefinitely.

### Alternatives Considered

1. The selected design combines real Source Index, Probe, TCP, and deterministic cloud
   bytes. It covers the server contract and remains reliable in offline CI.
2. A DTO-only cloud golden was rejected because it cannot prove request parsing, source
   ordering, authorization, route advertisement, or actual byte delivery.
3. A production-process test hook for injecting a fake cloud backend was rejected. It would
   widen production startup configuration solely for tests while adding little evidence
   beyond the existing Axum router and service graph.
4. Live Google Drive or Microsoft Graph acceptance was rejected as a release-gate fixture.
   It would make CI depend on external credentials, network state, and provider availability.
   Provider adapter contracts remain separate evidence.

## Scope

This slice includes:

- Filesystem `PlaybackInfo` request and normalized response goldens;
- cloud multi-source `PlaybackInfo` request and normalized response goldens;
- one cloud Movie backed by two distinct video StorageObjects in one active publication;
- real Source Index and Probe execution for both cloud sources;
- authenticated TCP login, detail, `PlaybackInfo`, media, subtitle, and Admin policy calls;
- complete two-source response and deterministic default-source ordering;
- full GET, HEAD, Range GET, Range HEAD, and subtitle delivery from advertised local URLs;
- an additional delivery request against the alternate advertised source;
- exact media/subtitle byte assertions and recorded backend range assertions;
- real re-index followed by the same response and delivery contract;
- response body and header checks for backend identity and credential leakage;
- compatibility status documentation after the contract passes.

This slice excludes live provider credentials, upstream OAuth/redirect behavior already
covered by adapter contracts, production log capture, multi-instance execution, concurrent
playback during pointer switch, source removal/tombstone cases, and more than two sources.
Those exclusions remain visible residual risks and must not be described as completed by
this slice.

## Test Architecture

Keep the contract in `crates/server/tests/browse_routes.rs` so it can reuse `TestApp`,
`MemoryCloudBackend`, authentication setup, service wiring, and the existing storage failure
fixtures. Do not add a second application builder or a production-only test injection path.

Add focused test helpers with one responsibility each:

- a cloud inventory fixture inserts reconciled SQL object facts for one Movie directory,
  two videos, and one sidecar subtitle, but no Source publication;
- a work helper enqueues, claims, and executes Source Index or Probe through the production
  application services;
- a bounded TCP harness owns the listener, server task, shutdown sender, and base URL;
- a golden normalizer maps database presentation identities to semantic placeholders and
  validates all dynamic UUIDs before replacement;
- a delivery assertion consumes URLs from `PlaybackInfo` and validates HTTP metadata,
  bytes, and backend reads.

`MemoryCloudBackend` gains a test-only `take_ranges` operation that atomically drains its
recorded calls. The contract drains Probe reads before playback assertions and between the
pre- and post-re-index delivery passes, so each expected range sequence has one meaning.

Test-only helpers stay in the integration test module. No cleanup method or observability
hook is added to production classes.

## Fixture Model

The cloud fixture contains one reconciled Movie directory with:

- `Remote Default.mkv`, a 17-byte object with a unique provider object ID;
- `Remote Alternate.mkv`, a different 17-byte object with a unique provider object ID;
- `Remote Default.eng.srt`, an external subtitle associated only with the default video.

Both videos use the same active cloud storage account and root but remain distinct logical
MediaSources. The in-memory backend stores different literal media bytes so routing one
presentation key to the other object cannot pass. Credential reference, account identity,
provider drive ID, object IDs, and a fake upstream URL/token are distinctive secret markers
for leak assertions.

`SourceIndexService` must classify and atomically publish both sources from reconciled SQL
inventory. `ProbeService` must read each video through the registered `MemoryCloudBackend`
and publish compatible MKV/H.264 metadata. The Admin playback-policy route marks the
presentation identity associated with `Remote Default.mkv` as default. Source identity is
resolved from database ownership and StorageObject relationships rather than inferred from
the returned array order.

## Golden Contract

Add four literal fixtures under `crates/server/tests/golden/playback/`:

- `filesystem-playback-info.request.json`;
- `filesystem-playback-info.response.json`;
- `cloud-multi-source-playback-info.request.json`;
- `cloud-multi-source-playback-info.response.json`.

The request files are sent as the actual POST bodies. They pin the representative Jellyfin
DeviceProfile shape instead of duplicating request JSON inline.

The response files pin the complete PascalCase response, including every MediaSource and
MediaStream field, `Protocol`, `Path`, `Container`, `DirectStreamUrl`, `TranscodingUrl`,
`IsRemote`, and all `Supports*` flags. They use explicit semantic placeholders for the item,
default presentation, alternate presentation, and PlaySession ID.

Normalization follows these rules:

1. Read the default and alternate presentation IDs from database relationships to their
   provider object IDs.
2. Validate the response `PlaySessionId` and every expected ID as UUIDs.
3. Replace exact ID values and exact URL path/query components with their named semantic
   placeholders.
4. Do not reorder `MediaSources`, `MediaStreams`, object keys, or any collection before
   comparison.
5. Fail if an unknown presentation ID or unexpected local URL appears.

This prevents a broken default sort from passing merely because the first and second array
entries were renamed after observation.

## Contract Flow

The cloud test performs this sequence:

1. Create the authenticated test application and reconciled multi-source cloud inventory.
2. Enqueue, claim, and execute Source Index; assert exactly two active sources.
3. Enqueue, claim, and execute Probe for both sources through the cloud backend.
4. Start the cloned Router on `127.0.0.1:0` with bounded graceful shutdown.
5. Authenticate over TCP and set the selected presentation as default through the Admin
   playback-policy route.
6. Fetch item detail and POST the request golden to `PlaybackInfo`.
7. Verify the default source is first, both sources are present, and the normalized response
   exactly matches the response golden.
8. Follow the default source's advertised `DirectStreamUrl` with full GET, HEAD, Range GET,
   and Range HEAD. Follow its advertised subtitle `DeliveryUrl` with GET.
9. Request bytes from the alternate source's advertised URL to prove it is independently
   routable and not an inert list entry.
10. Compare all bodies to literal fixture bytes and compare the playback-only backend
    ranges to the expected object-specific sequence after draining Probe reads. HEAD must
    not open a byte stream.
11. Submit and execute a new Source Index job for the same reconciled facts.
12. Prove that the effective publication ID and activation generation advanced.
13. Repeat steps 6 through 10 and compare both presentation IDs, ordering, policy, subtitle
    index, and URLs with the pre-re-index snapshot.
14. Shut down the TCP server, await it with a timeout, and surface any server-task error.

The Filesystem TCP smoke sends its new request golden and compares its normalized
`PlaybackInfo` payload with the Filesystem response golden while retaining the existing
end-to-end delivery, playstate, Resume, and re-index assertions.

## Security And Failure Handling

All advertised media and subtitle URLs must be relative TJXY routes and must not contain a
scheme or authority. `Path` and `TranscodingUrl` remain null, `IsRemote=false`,
`SupportsTranscoding=false`, and `SupportsDirectStream=false`; these flags describe TJXY's
server-proxied, byte-for-byte client contract rather than backend locality.

Scan every response header and the `PlaybackInfo` JSON body for leaks; media and subtitle
bodies are compared directly with their literal fixture bytes. The checked marker set
includes provider name, provider object IDs, drive ID, account identity, display name,
credential reference, fake upstream URL, and token. A failed Source Index, Probe, TCP
request, or shutdown is explicit test failure; no helper converts it to an empty response
or silently drops an error.

The in-process test does not capture production tracing output. `docs/api-parity.md` must
continue to report log-redaction verification as incomplete until a separate bounded log
capture contract exists.

## Verification

This slice adds release-gate evidence for behavior that is expected to exist, rather than a
new production feature. Test validity is therefore established with controlled mutation
checks: after the contract first passes, temporarily corrupt one pinned `Supports*` flag,
omit the alternate source from the expected response, swap the semantic default and
alternate presentation mapping, and change the expected subtitle delivery index. Each
mutation must fail for the intended assertion before the original fixture is restored.
Any production defect exposed by the new contract then follows the ordinary failing-test,
minimal-fix, passing-test cycle.

Focused verification runs the new cloud contract, the existing Filesystem TCP smoke, and
the complete `browse_routes` integration test. Final verification runs:

```text
cargo test -p tjxy-server --test browse_routes --locked
cargo test -p tjxy-server --test jellyfin_tcp_smoke --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
git diff --check
```

The cloud TCP test needs permission to bind a loopback ephemeral port. The deterministic
backend performs no external network request and requires no provider credential.

## Documentation And Residual Risk

After verification, update `docs/api-parity.md` to record that deterministic Filesystem and
provider-neutral cloud request/response goldens, complete two-source output, default-source
delivery, and cloud re-index stability are covered. Do not claim live Google/OneDrive
server integration, production log redaction, source removal, or concurrent pointer-switch
playback.

This slice materially strengthens release gates 11 and 13. It does not close gate 6
(cross-source work identity), the remaining gate 12 log evidence, or the gate 15 storage
change scenario matrix.
