# Cloud Multi-Source Playback Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the deterministic Filesystem and provider-neutral cloud/multi-source playback evidence gap in `PLAN.md` section 18.10 without changing the public API or production runtime configuration.

**Architecture:** Extend the two existing server integration-test harnesses. The production-process Filesystem smoke test sends and compares literal normalized `PlaybackInfo` goldens. The in-process cloud harness builds reconciled storage facts, runs the real Source Index and Probe services, selects a default through the Admin HTTP route, and exercises the complete two-source contract through a bounded real-TCP Axum server. Dynamic IDs are normalized from database ownership relationships, while source order remains untouched.

**Tech Stack:** Rust 1.88.0, Axum 0.8.9, Tokio, Reqwest, SeaORM/SeaQuery 1.1.19, SQLite test databases, Serde JSON, TJXY Source Index/Probe/media services.

**Status:** Complete on 2026-07-28. Required focused checks, controlled mutations, and the serialized workspace gate passed; independent review findings for redirect handling, absolute delivery headers, and binary-safe header scanning were addressed before closure. The default-parallel workspace gate exposed two non-reproducible existing races (`startup` SQLite write locking and Hybrid background catalog revision invalidation); each exact test and complete owning target passed on rerun, and no related production or test code changed in this slice.

## Global Constraints

- Follow `docs/superpowers/specs/2026-07-27-cloud-multi-source-playback-contract-design.md` exactly.
- This is test-evidence work. Do not add a production backend injection hook, public route, DTO field, schema, configuration option, or logging behavior.
- Reuse `TestApp`, `MemoryCloudBackend`, `CloudProbeInspector`, `TestServer`, and existing auth/response helpers before adding narrowly scoped test-only helpers.
- Build cloud MediaSources through reconciled storage inventory and `SourceIndexService`; do not use `seed_playable_source_for_provider` for the new contract.
- Probe both cloud MediaSources through `ProbeService` and the registered `MemoryCloudBackend`; do not insert `Probed` rows directly.
- Resolve default and alternate presentation IDs by joining each active source to its `storage_objects.provider_object_id`. Never infer semantic identity from response array position.
- Validate every dynamic item, presentation, publication, and play-session identifier as a UUID before normalization.
- Replace only exact JSON values and exact TJXY path/query components. Do not perform broad string substitution and do not reorder `MediaSources` or `MediaStreams` before golden comparison.
- Keep cloud object bytes distinct and exactly 17 bytes so a presentation-to-object routing swap fails byte assertions.
- Drain recorded Probe ranges before delivery checks and between pre-/post-re-index passes.
- Scan the complete `PlaybackInfo` JSON plus every response header for the distinctive provider/account/object/credential/upstream markers. Compare media and subtitle bodies to literal bytes rather than scanning them as metadata.
- The TCP task must use graceful shutdown and a bounded join. A request, service, database, or shutdown failure must fail explicitly.
- Do not claim production log-redaction, live Google Drive/OneDrive integration, source removal, concurrency during publication switch, or multi-instance coverage.
- Preserve unrelated untracked `.pi/` and `.playwright-cli/` content. Stage only files named by the current task and inspect `git diff --cached` before every commit.
- Axum 0.8 server guidance was refreshed through Context7 (`/tokio-rs/axum/axum_v0_8_4`) during design: use `axum::serve(listener, router).with_graceful_shutdown(...)` and bound the spawned task with `tokio::time::timeout`.
- Because this slice captures already-implemented behavior, prove test sensitivity with the controlled golden mutations in Tasks 1 and 3. If the new contract exposes a production defect, first retain the focused failing test, then make the smallest production fix and rerun the task checks.

---

## File Map

- Create `crates/server/tests/golden/playback/filesystem-playback-info.request.json`: literal Filesystem DeviceProfile POST body.
- Create `crates/server/tests/golden/playback/filesystem-playback-info.response.json`: complete normalized single-source response.
- Create `crates/server/tests/golden/playback/cloud-multi-source-playback-info.request.json`: literal cloud DeviceProfile POST body.
- Create `crates/server/tests/golden/playback/cloud-multi-source-playback-info.response.json`: complete normalized ordered two-source response.
- Modify `crates/server/tests/jellyfin_tcp_smoke.rs`: send the Filesystem request fixture and compare the normalized full response before and after re-index.
- Modify `crates/server/tests/browse_routes.rs`: add the second cloud object, reconciled multi-source inventory, real Source Index/Probe execution, bounded TCP harness, semantic normalization, delivery/leak checks, and re-index assertions.
- Modify `docs/api-parity.md`: record only the newly verified deterministic evidence and retain residual gaps.

---

### Task 1: Pin The Filesystem PlaybackInfo HTTP Golden

**Files:**
- Create: `crates/server/tests/golden/playback/filesystem-playback-info.request.json`
- Create: `crates/server/tests/golden/playback/filesystem-playback-info.response.json`
- Modify: `crates/server/tests/jellyfin_tcp_smoke.rs`

**Interfaces:**
- Consumes: the existing `TestServer`, `assert_playback_delivery_contract`, item UUID, effective presentation UUID, and real HTTP response.
- Produces: a normalized complete `serde_json::Value` whose only placeholders are `{{item_id}}`, `{{source_id}}`, and `{{play_session_id}}`.

- [x] **Step 1: Add literal request and expected-response constants**

Use `include_str!` so missing or invalid fixtures fail the test binary deterministically:

```rust
const FILESYSTEM_PLAYBACK_REQUEST: &str =
    include_str!("golden/playback/filesystem-playback-info.request.json");
const FILESYSTEM_PLAYBACK_RESPONSE: &str =
    include_str!("golden/playback/filesystem-playback-info.response.json");
```

The request fixture is the existing representative client profile, moved without semantic changes:

```json
{
  "DeviceProfile": {
    "DirectPlayProfiles": [
      { "Type": "Video", "Container": "mp4" }
    ]
  }
}
```

Parse the fixture once per assertion with `serde_json::from_str::<Value>` and pass it to Reqwest with `.json(&request)`.

- [x] **Step 2: Add structured semantic normalization**

Add a test-only helper beside `PlaybackContractSnapshot`:

```rust
fn normalize_filesystem_playback(
    playback: &mut Value,
    item_id: Uuid,
    source_id: Uuid,
) {
    let _play_session = playback["PlaySessionId"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .expect("PlaybackInfo PlaySessionId is a UUID");
    assert_eq!(
        Uuid::parse_str(playback["MediaSources"][0]["Id"].as_str().expect("source ID"))
            .expect("source ID is a UUID"),
        source_id,
    );
    let expected_direct =
        format!("/Videos/{item_id}/stream?static=true&mediaSourceId={source_id}");
    assert_eq!(
        playback["MediaSources"][0]["DirectStreamUrl"].as_str(),
        Some(expected_direct.as_str()),
    );
    playback["PlaySessionId"] = json!("{{play_session_id}}");
    playback["MediaSources"][0]["Id"] = json!("{{source_id}}");
    playback["MediaSources"][0]["DirectStreamUrl"] = json!(
        "/Videos/{{item_id}}/stream?static=true&mediaSourceId={{source_id}}"
    );
    for stream in playback["MediaSources"][0]["MediaStreams"]
        .as_array_mut()
        .expect("media stream list")
    {
        if stream["IsExternal"] == true {
            let index = stream["Index"].as_i64().expect("subtitle index");
            let expected_subtitle =
                format!("/Videos/{item_id}/{source_id}/Subtitles/{index}/Stream.srt");
            assert_eq!(stream["DeliveryUrl"].as_str(), Some(expected_subtitle.as_str()));
            stream["DeliveryUrl"] = json!(format!(
                "/Videos/{{{{item_id}}}}/{{{{source_id}}}}/Subtitles/{index}/Stream.srt"
            ));
        }
    }
}
```

Before replacing either URL, assert its original value exactly equals the route constructed from `item_id`, `source_id`, and the advertised subtitle index. This prevents normalization from hiding a malformed or upstream URL.

- [x] **Step 3: Compare the complete response in the existing delivery helper**

In `assert_playback_delivery_contract`, retain all current field and byte assertions. Clone the actual response before extracting URLs, normalize the clone, parse the expected fixture, and compare complete JSON values:

```rust
let mut normalized = playback.clone();
normalize_filesystem_playback(
    &mut normalized,
    Uuid::parse_str(episode_id).expect("episode ID is a UUID"),
    Uuid::parse_str(&source_id).expect("source ID is a UUID"),
);
let expected: Value = serde_json::from_str(FILESYSTEM_PLAYBACK_RESPONSE)
    .expect("filesystem PlaybackInfo golden is valid JSON");
assert_eq!(normalized, expected);
```

Build the response golden from one observed response, then manually reconcile every key against `tjxy_api::PlaybackInfoResponse`, `MediaSourceInfo`, and `MediaStream`. Pin explicit nulls, every `Supports*` flag, stream order, delivery index, and local URL. Do not accept an auto-updated snapshot.

Because the helper already runs before and after Admin `IndexMediaSources`, this single comparison proves both generations against the same golden while the surrounding test continues to prove a new publication and stable presentation identity.

- [x] **Step 4: Run the focused Filesystem contract**

Run with loopback-bind permission:

```bash
cargo test -p tjxy-server --test jellyfin_tcp_smoke tcp_filesystem_library_survives_restart_and_supports_jellyfin_playback_contract --locked -- --exact
```

Expected: PASS, with the existing full GET, HEAD, Range GET, Range HEAD, subtitle, restart, re-index, playstate, and Resume assertions unchanged.

- [x] **Step 5: Prove the golden is sensitive**

Temporarily change `SupportsDirectPlay` from `true` to `false` in the response fixture and rerun Step 4. Expected: FAIL at the complete golden comparison. Restore the fixture.

Temporarily change the expected subtitle `Index` and matching URL index to a different integer and rerun Step 4. Expected: FAIL at the complete golden comparison. Restore the fixture and rerun Step 4 to green.

- [x] **Step 6: Verify and commit Task 1**

```bash
cargo fmt --all -- --check
git diff --check
git add crates/server/tests/jellyfin_tcp_smoke.rs \
  crates/server/tests/golden/playback/filesystem-playback-info.request.json \
  crates/server/tests/golden/playback/filesystem-playback-info.response.json
git diff --cached --check
git diff --cached --stat
git commit -m "test: pin filesystem playback HTTP golden"
```

---

### Task 2: Build A Real Cloud Multi-Source Publication

**Files:**
- Modify: `crates/server/tests/browse_routes.rs`

**Interfaces:**
- Produces: `CloudMultiSourceFixture` containing item, root, default/alternate StorageObject IDs, provider object IDs, and literal bytes.
- Consumes: `SourceIndexService`, `ProbeService`, `WorkJobRepository`, `WorkJobSpec`, `WorkScope`, `WorkTaskKind`, and `CatalogPublicationRepository`.

- [x] **Step 1: Extend the provider-neutral backend with distinct objects and range draining**

Define literal data once:

```rust
const CLOUD_DEFAULT_BYTES: &[u8] = b"cloud-byte-stream";
const CLOUD_ALTERNATE_BYTES: &[u8] = b"other-byte-stream";
const CLOUD_SUBTITLE_BYTES: &[u8] =
    b"1\n00:00:01,000 --> 00:00:02,000\nCloud\n\n\n";
```

Both video constants must remain 17 bytes. Expand `cloud_fixture` and `TestApp` with a distinct alternate provider object ID. Keep the existing default and subtitle IDs so unrelated cloud tests do not need semantic changes.

Add an atomic drain used only by tests:

```rust
fn take_ranges(&self) -> Vec<(String, u64, u64)> {
    self.ranges.lock().unwrap().drain(..).collect()
}
```

Retain `ranges()` because existing tests use its non-destructive behavior.

- [x] **Step 2: Insert reconciled inventory without a source publication**

Add a `seed_cloud_multi_source_inventory` helper modeled on `source_index_publishes_video_and_sidecar_from_sql_inventory`. Reuse `seed_library` and `seed_item`, then insert:

- one active `storage_accounts` row for `app.cloud_account`, provider `cloud-test`, and distinctive display/account/credential markers;
- one `storage_roots` row with `sync_revision=1` and `reconciled_sync_revision=1`;
- one `library_storage_roots` relation;
- one indexed Movie directory named `Remote Default`;
- `Remote Default.mkv`, `Remote Alternate.mkv`, and `Remote Default.eng.srt` as present child files at revision 1;
- matching `storage_root_objects` rows with the directory as parent;
- one `identity_matches` row from the directory to the Movie item in state `Matched`.

Use fixed distinctive provider metadata in the SQL facts:

```rust
const CLOUD_PROVIDER: &str = "cloud-test";
const CLOUD_DRIVE: &str = "drive-secret-marker";
const CLOUD_ACCOUNT_IDENTITY: &str =
    "https://upstream.invalid/secret?account=account-secret-marker";
const CLOUD_DISPLAY_NAME: &str = "Cloud Secret Display";
const CLOUD_CREDENTIAL_REF: &str =
    "credential-secret-marker:upstream-token-secret";
```

The fake upstream URL and token therefore live in real account facts traversed by the
playback lookup. Leak assertions must scan both the complete values and the distinctive
substrings (`account-secret-marker`, `credential-secret-marker`, and
`upstream-token-secret`).

Return typed record IDs from the helper. Do not insert `media_sources`, `media_locations`, streams, subtitles, or publications.

- [x] **Step 3: Execute Source Index through the work contract**

Add a helper whose two revision parameters remain explicit:

```rust
async fn index_cloud_sources(
    database: &DatabaseConnection,
    item: CatalogItemId,
    task_revision: i64,
    input_sync_revision: i64,
) -> i64 {
    let jobs = tjxy_db::WorkJobRepository::new(database);
    jobs.enqueue_or_join(
        &tjxy_db::WorkJobSpec::new(
            tjxy_db::WorkTaskKind::IndexMediaSources,
            tjxy_db::WorkScope::CatalogItem(item),
            task_revision,
            100,
        )
        .expect("valid source-index work spec")
        .with_input_sync_revision(input_sync_revision)
        .expect("valid reconciled revision"),
    )
    .await
    .expect("enqueue source-index work");
    let claimed = jobs
        .claim_next(
            &[tjxy_db::WorkTaskKind::IndexMediaSources],
            "cloud-multi-source-index",
            Duration::minutes(1),
        )
        .await
        .expect("claim source-index work")
        .expect("source-index work exists");
    tjxy_application::SourceIndexService::new(database.clone())
        .execute(&claimed)
        .await
        .expect("index reconciled cloud sources")
}
```

Use task revision 1 for the first publication and 2 for re-index, while both executions consume reconciled sync revision 1.

- [x] **Step 4: Resolve semantic identities and Probe both active sources**

Query active source/location/object relationships into a small `CloudPresentations` value:

```rust
struct CloudPresentations {
    default: Uuid,
    alternate: Uuid,
}
```

The query must join the effective active source publication, `media_sources`, `media_locations`, and `storage_objects`; map by exact provider object ID and reject duplicates or missing rows. Assert both presentation values parse as UUIDs and are distinct.

For each active source ID, enqueue `ProbeMedia`, claim the exact submitted job, and execute:

```rust
ProbeService::new(app.database.clone())
    .with_backend(app.cloud_account, Arc::clone(&app.cloud_backend))
    .with_inspector(Arc::new(CloudProbeInspector))
    .execute(&claimed)
    .await
    .expect("probe cloud source");
```

After both jobs complete, assert through `CatalogPublicationRepository::active_sources` that there are exactly two sources, both are `Probed`, both are MKV/H.264 1920x1080, only the default-object source owns the English SRT, and presentation IDs match the semantic query.

- [x] **Step 5: Run the server test target as a compilation and regression checkpoint**

```bash
cargo test -p tjxy-server --test browse_routes cloud_source_probe_uses_the_registered_backend_and_a_bounded_range --locked -- --exact
```

Expected: PASS. This checkpoint confirms the extended fixture did not alter existing cloud behavior. The new helpers become exercised by the end-to-end contract in Task 3 rather than by a redundant service-only test.

- [x] **Step 6: Format but do not commit the incomplete cross-task file**

```bash
cargo fmt --all -- --check
git diff --check
```

Task 2 and Task 3 intentionally share `browse_routes.rs`; keep them in one working change and commit only after the TCP contract is complete.

---

### Task 3: Prove The Cloud Multi-Source Contract Over Real TCP

**Files:**
- Create: `crates/server/tests/golden/playback/cloud-multi-source-playback-info.request.json`
- Create: `crates/server/tests/golden/playback/cloud-multi-source-playback-info.response.json`
- Modify: `crates/server/tests/browse_routes.rs`

**Interfaces:**
- Produces: one bounded real-TCP contract covering authenticated request parsing, ordered complete response, byte delivery, policy, leakage, and re-index stability.
- Consumes: Task 2's inventory/service helpers and semantic `CloudPresentations` mapping.

- [x] **Step 1: Add a bounded test-only Axum TCP owner**

Use a one-shot shutdown channel and retain the join handle:

```rust
struct TcpTestServer {
    base_url: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<Result<(), std::io::Error>>>,
}

impl TcpTestServer {
    async fn start(router: axum::Router) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind cloud playback test server");
        let address = listener.local_addr().expect("read test server address");
        let (shutdown, receiver) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move { let _ = receiver.await; })
                .await
        });
        Self {
            base_url: format!("http://{address}"),
            shutdown: Some(shutdown),
            task: Some(task),
        }
    }

    async fn stop(mut self) {
        self.shutdown
            .take()
            .expect("shutdown sender")
            .send(())
            .expect("test server receives shutdown");
        let task = self.task.take().expect("test server task");
        tokio::time::timeout(std::time::Duration::from_secs(2), task)
            .await
            .expect("test server stops before timeout")
            .expect("test server task joins")
            .expect("test server exits cleanly");
    }
}

impl Drop for TcpTestServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}
```

The `Drop` path makes assertion unwinding stop acceptance; the explicit `stop` path still
proves bounded task completion. Do not use `abort()` on the success path.

- [x] **Step 2: Add the literal cloud request and normalized complete response**

The request fixture pins direct-play compatibility for MKV:

```json
{
  "DeviceProfile": {
    "DirectPlayProfiles": [
      { "Type": "Video", "Container": "mkv" }
    ]
  }
}
```

The response fixture must contain exactly two complete MediaSources in policy order. Use these semantic placeholders only:

```text
{{item_id}}
{{default_source_id}}
{{alternate_source_id}}
{{play_session_id}}
```

Pin all serialized `MediaSourceInfo` and `MediaStream` fields, explicit nulls, default-source subtitle ownership/index, and local paths. Both sources must state `Protocol=Http`, `Path=null`, `IsRemote=false`, `SupportsTranscoding=false`, `SupportsDirectStream=false`, and `SupportsDirectPlay=true`.

- [x] **Step 3: Normalize from database semantics without sorting**

Add `normalize_cloud_playback(playback, item_id, presentations)` that:

1. validates `PlaySessionId` as a UUID;
2. requires exactly two response sources;
3. validates every response source ID as a UUID and maps it through `CloudPresentations`;
4. asserts each original direct-stream and subtitle URL exactly before replacing it;
5. replaces IDs and URLs with the correct semantic placeholders;
6. leaves both arrays in their returned order.

Reject an unknown or repeated source ID. This makes a swapped semantic mapping fail even when both sources otherwise have identical probe metadata.

- [x] **Step 4: Add reusable response leak and delivery assertions**

Define the marker set in the fixture, including provider, both provider object IDs, subtitle object ID, drive ID, account UUID/string identity, display name, credential reference, `https://upstream.invalid/secret`, and `upstream-token-secret`.

For `PlaybackInfo`, serialize the complete JSON value and scan it. Build the Reqwest client with redirects disabled so an intermediate redirect cannot hide its status or `Location`. For each delivery response, scan all raw header-name and header-value bytes before consuming the body; use lossy text conversion only in assertion diagnostics:

```rust
fn assert_headers_do_not_leak(headers: &reqwest::header::HeaderMap, markers: &[String]) {
    for (name, value) in headers {
        for marker in markers {
            let marker = marker.as_bytes();
            assert!(!marker.is_empty());
            for encoded in [name.as_str().as_bytes(), value.as_bytes()] {
                assert!(!encoded.windows(marker.len()).any(|part| part == marker));
            }
        }
    }
}
```

Consume only URLs returned by `PlaybackInfo` and verify:

- default full GET: 200, exact content type/cache/range/length/ETag contract, no `Content-Range`, and exact `CLOUD_DEFAULT_BYTES`;
- default HEAD: 200, the same pinned representation headers, no `Content-Range`, and an empty body;
- default Range GET `bytes=6-9`: 206, `Content-Range: bytes 6-9/17`, exact four bytes;
- default Range HEAD `bytes=6-9`: 206, the same pinned range headers, empty body;
- advertised default subtitle GET: 200 and exact `CLOUD_SUBTITLE_BYTES`;
- alternate full GET: 200 and exact `CLOUD_ALTERNATE_BYTES`.

Assert the drained backend range list exactly from the above operations using provider object IDs and `bytes.len()` bounds. HEAD requests must not create backend reads; GET and subtitle reads must appear once in request order.

- [x] **Step 5: Execute the complete pre-/post-re-index scenario**

Add one `#[tokio::test]` with a descriptive contract name. Its sequence is:

1. build `TestApp`, seed the cloud inventory, and execute Source Index revision 1;
2. semantically resolve both presentations and Probe both sources;
3. drain and assert the two bounded Probe reads, then start the TCP server;
4. authenticate Admin over TCP and retain the returned token/user ID;
5. `PUT /Admin/Items/{item}/MediaSources/{default}/PlaybackPolicy` with `{"AdminPriority":100,"IsDefault":true,"IsHidden":false}`;
6. GET item detail and verify the expected Movie identity;
7. POST the literal cloud request, compare the normalized full response golden, assert default/alternate order, scan for leaks, and run the delivery checks;
8. record the effective publication UUID/generation, both presentation UUIDs, source order, policy, subtitle index, and advertised URLs;
9. execute Source Index revision 2 against the same reconciled revision 1 facts;
10. assert a different active publication UUID and higher generation, but the same two semantic presentations and default policy;
11. drain any setup reads, repeat the exact golden/delivery contract, and compare the recorded stable fields;
12. gracefully stop the TCP server even when all assertions pass.

Keep the server owner in scope for the whole assertion body so its `Drop` sends shutdown
during unwinding. On the success path, call `stop().await` and do not ignore the bounded
shutdown result.

- [x] **Step 6: Run the focused cloud contract and surrounding target**

Run with loopback-bind permission:

```bash
cargo test -p tjxy-server --test browse_routes cloud_multi_source_playback_is_complete_local_and_stable_across_reindex --locked -- --exact
cargo test -p tjxy-server --test browse_routes --locked
```

Expected: both commands exit 0. Rename the exact test filter in this plan if implementation chooses a clearer final test name.

- [x] **Step 7: Prove the cloud golden and semantic mapping are sensitive**

Perform each mutation separately, rerun the exact focused test, confirm the intended failure, and restore before the next mutation:

- change one `SupportsDirectPlay` flag to `false`;
- delete the complete alternate source object from the expected `MediaSources` array;
- swap `default` and `alternate` when constructing `CloudPresentations`;
- change the expected subtitle delivery index and its URL index.

Expected: the first, second, and fourth mutations fail the complete golden comparison; the semantic swap fails the ID/URL/byte routing assertions. Restore all files, rerun the focused test to green, and verify `git diff` contains no mutation residue.

- [x] **Step 8: Verify and commit Tasks 2-3 together**

```bash
cargo fmt --all -- --check
cargo clippy -p tjxy-server --test browse_routes --locked -- -D warnings
git diff --check
git add crates/server/tests/browse_routes.rs \
  crates/server/tests/golden/playback/cloud-multi-source-playback-info.request.json \
  crates/server/tests/golden/playback/cloud-multi-source-playback-info.response.json
git diff --cached --check
git diff --cached --stat
git commit -m "test: prove cloud multi-source playback contract"
```

---

### Task 4: Update Release Evidence And Run The Full Gate

**Files:**
- Modify: `docs/api-parity.md`

**Interfaces:**
- Produces: an evidence statement aligned with actual deterministic coverage and explicit residual gaps.
- Consumes: passing Task 1 and Task 3 contracts.

- [x] **Step 1: Update the nearest compatibility evidence**

In `docs/api-parity.md`, record that the following are now covered:

- literal Filesystem and provider-neutral cloud `PlaybackInfo` request/normalized-response goldens;
- complete ordered two-source cloud output with explicit Admin default policy;
- full, HEAD, ranged GET/HEAD, subtitle, and alternate-source delivery through advertised local TJXY URLs;
- exact cloud bytes, header/body provider-marker exclusion, real Source Index and Probe, and stable presentations/URLs across replacement publication.

Keep production log-redaction, live provider server integration, source removal/tombstones, concurrent pointer switching, and multi-instance behavior explicitly incomplete. State that body/header checks do not substitute for production tracing capture.

- [x] **Step 2: Run focused server verification**

Run with loopback-bind permission:

```bash
cargo test -p tjxy-server --test browse_routes --locked
cargo test -p tjxy-server --test jellyfin_tcp_smoke --locked
```

Expected: both integration-test binaries pass in full.

- [x] **Step 3: Run workspace verification**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Expected: every command exits 0. If an environment-only test cannot run, record the exact command, error, and remaining risk rather than marking the gate complete.

- [x] **Step 4: Review the integrated change for quality and scope**

Inspect:

```bash
git diff --stat HEAD~2..HEAD
git diff --check
rg -n "cloud-test|secret|credential|upstream|token" \
  crates/server/tests/golden/playback docs/api-parity.md
git status --short
```

Confirm that:

- no production source, schema, API DTO, or runtime configuration changed;
- test helpers do not swallow service, HTTP, database, or shutdown errors;
- no provider marker appears in a response golden;
- range draining separates Probe and delivery observations;
- no broad normalizer can hide a malformed URL or swapped identity;
- the four controlled mutations were fully restored;
- documentation does not overstate the remaining release gates;
- only `.pi/` and `.playwright-cli/` remain as unrelated untracked content.

- [x] **Step 5: Commit documentation**

```bash
git add docs/api-parity.md
git diff --cached --check
git diff --cached
git commit -m "docs: record multi-source playback evidence"
```

- [x] **Step 6: Report completion**

Summarize the behavior proven, why semantic normalization was used, every verification command and result, controlled mutation results, commit IDs, and residual gaps. Do not mark the plan complete unless all required checks pass or each omitted check is explicitly documented.
