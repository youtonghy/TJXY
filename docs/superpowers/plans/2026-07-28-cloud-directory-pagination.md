# Cloud Directory Pagination Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Google Drive My Drive/Shared Drive and OneDrive Personal folder selection paginate completely without exposing provider continuation tokens to React Admin.

**Architecture:** Add one private, generic, session-local cursor registry that maps TJXY UUIDs to opaque `PageToken` values and binds them to provider-specific directory contexts. The existing OAuth services resolve and register cursors while holding their mutex only for in-memory work; provider I/O remains outside the lock. React Admin consumes a common `{ items, nextPageToken }` boundary and appends deduplicated folder pages through the existing storage wizard layouts.

**Tech Stack:** Rust 2024, Axum 0.8, Tokio, UUID, existing `tjxy-storage` types, React 19, TypeScript 6, React Admin 5, MUI 9, Vitest/Testing Library.

## Global Constraints

- Do not change the public `StorageBackend` trait, database schema, OAuth-session TTL, or Shared Drive list pagination contract.
- Google Drive `files.list` must receive the previous provider `nextPageToken` unchanged.
- Microsoft Graph must receive the complete validated `@odata.nextLink` unchanged; do not extract or rebuild `$skiptoken`.
- Raw Google directory page tokens and Microsoft Graph URLs must never appear in Admin responses, errors, logs, or frontend state.
- Directory cursors are random UUIDs, bound to OAuth state, authenticated login session, provider drive/scope, and normalized parent ID.
- Each OAuth session retains at most 256 replayable directory cursors; reuse the same cursor for the same context/token and evict the oldest inserted entry.
- Malformed, unknown, expired, and context-mismatched cursors return 400 before provider I/O; wrong session owner remains 403; incomplete callback remains 409.
- Never hold an OAuth-session mutex across backend construction or provider network I/O.
- A provider page may contain zero folders and still have another page; Admin must keep "Load more" available in that state.
- Reuse existing `PageToken`, `validUuid`, error mapping, MUI controls, test fixtures, and the current Google `uniqueChoices` implementation instead of duplicating them.
- No new runtime dependency is allowed.
- Preserve unrelated `.pi/` and `.playwright-cli/` worktree content.

---

## Status

Design approved and implementation plan prepared. No production or test code from this
plan has been changed yet. Update this section with each red/green command, commit, review
finding, and residual risk during execution.

Plan self-review: every approved design requirement maps to Tasks 1-6; the placeholder
scan is clean; registry, server DTO, frontend DTO, and component method signatures agree.

---

## File Map

- Create `crates/server/src/storage_admin_cursor.rs`: bounded provider-neutral cursor registry and focused unit contracts.
- Modify `crates/server/src/lib.rs`: register the new private module only.
- Modify `crates/server/src/storage_admin.rs`: provider-specific cursor contexts, OAuth-session ownership, query/response DTOs, and two paginated handlers.
- Modify `crates/server/tests/storage_admin_routes.rs`: deterministic Google/Graph pagination, leakage, replay, context, and owner contracts.
- Modify `crates/storage-onedrive/src/lib.rs`: validate opaque Graph continuation URLs against the configured safe API origin.
- Modify `crates/storage-onedrive/tests/onedrive_contract.rs`: configured-origin continuation and cross-origin rejection contracts.
- Modify `admin/src/storage/googleDriveApi.ts`: common directory-page DTO parsing and query encoding.
- Modify `admin/src/storage/googleDriveApi.test.ts`: frontend HTTP-boundary contracts.
- Modify `admin/src/storage/GoogleDrivePage.tsx`: Google folder-page state and interaction.
- Modify `admin/src/storage/GoogleDrivePage.test.tsx`: Google append/dedupe/reset/failure contracts.
- Create `admin/src/storage/directoryChoices.ts`: shared first-seen-order folder deduplication moved from the Google component.
- Create `admin/src/storage/directoryChoices.test.ts`: focused deduplication contract.
- Modify `admin/src/storage/OneDrivePage.tsx`: OneDrive folder-page state and interaction.
- Modify `admin/src/storage/OneDrivePage.test.tsx`: OneDrive empty-page/final-page/failure contracts.
- Modify `docs/api-parity.md`: record verified interactive directory pagination without overstating live-provider coverage.
- Modify `README.md`: remove directory truncation from the Admin limitation statement while retaining storage status/reauthorization gaps.
- Modify this plan: record red/green commands, controlled checks, review results, and residual risk as tasks complete.

---

### Task 1: Bounded Session Cursor Registry

**Files:**
- Create: `crates/server/src/storage_admin_cursor.rs`
- Modify: `crates/server/src/lib.rs:22`
- Test: `crates/server/src/storage_admin_cursor.rs`

**Interfaces:**
- Consumes: `tjxy_storage::PageToken`, `uuid::Uuid`, and any `Context: Eq`.
- Produces: `DirectoryPageCursorRegistry<Context>::resolve(Option<Uuid>, &Context) -> Result<Option<PageToken>, DirectoryPageCursorError>` and `register(Context, Option<PageToken>) -> Option<Uuid>`.

- [ ] **Step 1: Register the module and write failing registry contracts**

Add `mod storage_admin_cursor;` next to `mod storage_admin;` in `lib.rs`. Create the module with tests that reference the not-yet-defined registry:

```rust
#[cfg(test)]
mod tests {
    use tjxy_storage::PageToken;
    use uuid::Uuid;

    use super::{DirectoryPageCursorError, DirectoryPageCursorRegistry};

    #[test]
    fn cursor_registry_is_replayable_context_bound_and_reuses_output() {
        let mut registry = DirectoryPageCursorRegistry::default();
        let context = ("MyDrive".to_owned(), "root".to_owned());
        let provider = PageToken::new("google-provider-page-2").unwrap();

        let cursor = registry.register(context.clone(), Some(provider.clone())).unwrap();
        assert_eq!(registry.register(context.clone(), Some(provider.clone())), Some(cursor));
        assert_eq!(registry.resolve(Some(cursor), &context).unwrap(), Some(provider));
        assert_eq!(
            registry.resolve(Some(cursor), &("MyDrive".to_owned(), "other".to_owned())),
            Err(DirectoryPageCursorError::UnknownOrMismatched),
        );
        assert_eq!(
            registry.resolve(Some(Uuid::new_v4()), &context),
            Err(DirectoryPageCursorError::UnknownOrMismatched),
        );
        assert_eq!(registry.resolve(None, &context).unwrap(), None);
        assert_eq!(registry.register(context, None), None);
    }

    #[test]
    fn cursor_registry_evicts_the_oldest_entry_at_its_fixed_bound() {
        let mut registry = DirectoryPageCursorRegistry::default();
        let first_context = 0_u16;
        let first = registry.register(
            first_context,
            Some(PageToken::new("provider-page-0").unwrap()),
        ).unwrap();
        for value in 1_u16..=256 {
            let _ = registry.register(
                value,
                Some(PageToken::new(format!("provider-page-{value}")).unwrap()),
            );
        }
        assert_eq!(registry.len(), 256);
        assert_eq!(
            registry.resolve(Some(first), &first_context),
            Err(DirectoryPageCursorError::UnknownOrMismatched),
        );
    }
}
```

- [ ] **Step 2: Run the focused unit test to prove red**

Run: `cargo test -p tjxy-server storage_admin_cursor --locked`

Expected: compilation fails because `DirectoryPageCursorRegistry` and `DirectoryPageCursorError` do not exist.

- [ ] **Step 3: Implement the private bounded registry**

Use a `HashMap` plus `VecDeque`; do not introduce an LRU dependency or a fallible counter:

```rust
use std::collections::{HashMap, VecDeque};

use tjxy_storage::PageToken;
use uuid::Uuid;

const MAX_DIRECTORY_PAGE_CURSORS: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectoryPageCursorError {
    UnknownOrMismatched,
}

struct DirectoryPageCursor<Context> {
    context: Context,
    provider_token: PageToken,
}

pub(crate) struct DirectoryPageCursorRegistry<Context> {
    entries: HashMap<Uuid, DirectoryPageCursor<Context>>,
    insertion_order: VecDeque<Uuid>,
}

impl<Context> Default for DirectoryPageCursorRegistry<Context> {
    fn default() -> Self {
        Self { entries: HashMap::new(), insertion_order: VecDeque::new() }
    }
}

impl<Context: Eq> DirectoryPageCursorRegistry<Context> {
    pub(crate) fn resolve(
        &self,
        cursor: Option<Uuid>,
        context: &Context,
    ) -> Result<Option<PageToken>, DirectoryPageCursorError> {
        let Some(cursor) = cursor else { return Ok(None) };
        let entry = self.entries.get(&cursor)
            .filter(|entry| &entry.context == context)
            .ok_or(DirectoryPageCursorError::UnknownOrMismatched)?;
        Ok(Some(entry.provider_token.clone()))
    }

    pub(crate) fn register(
        &mut self,
        context: Context,
        provider_token: Option<PageToken>,
    ) -> Option<Uuid> {
        let provider_token = provider_token?;
        if let Some((cursor, _)) = self.entries.iter().find(|(_, entry)| {
            entry.context == context && entry.provider_token == provider_token
        }) {
            return Some(*cursor);
        }
        if self.entries.len() == MAX_DIRECTORY_PAGE_CURSORS
            && let Some(oldest) = self.insertion_order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        let cursor = loop {
            let candidate = Uuid::new_v4();
            if !self.entries.contains_key(&candidate) { break candidate }
        };
        self.entries.insert(cursor, DirectoryPageCursor { context, provider_token });
        self.insertion_order.push_back(cursor);
        Some(cursor)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize { self.entries.len() }
}
```

- [ ] **Step 4: Run registry tests and formatting**

Run: `cargo test -p tjxy-server storage_admin_cursor --locked`

Expected: both unit tests pass.

Run: `cargo fmt --all -- --check`

Expected: pass after applying `cargo fmt --all` if the check reports formatting differences.

- [ ] **Step 5: Commit the registry**

```bash
git add crates/server/src/lib.rs crates/server/src/storage_admin_cursor.rs
git commit -m "feat: add bounded storage admin cursors"
```

---

### Task 2: Session-Bound Google And OneDrive Directory Routes

**Files:**
- Modify: `crates/server/src/storage_admin.rs:291-805,865-1102`
- Modify: `crates/server/tests/storage_admin_routes.rs:251-805`
- Modify: `crates/storage-onedrive/src/lib.rs:780-885,1151-1176`
- Modify: `crates/storage-onedrive/tests/onedrive_contract.rs:318`

**Interfaces:**
- Consumes: `DirectoryPageCursorRegistry<GoogleDirectoryPageContext>` and `DirectoryPageCursorRegistry<OneDriveDirectoryPageContext>` from Task 1.
- Produces: optional UUID `PageToken` query fields and `{ Items, NextPageToken }` responses for both existing directory routes.

- [ ] **Step 1: Make deterministic fake providers expose three pages and record child requests**

Replace each fake's form-only state with a state that also records child queries. Google page 1 returns raw token `google-provider-page-2`, page 2 returns one duplicate folder plus `google-provider-page-3`, and page 3 is terminal. Microsoft page 1 returns a complete loopback `@odata.nextLink`, page 2 returns a second link, and page 3 is terminal.

Key handler shapes:

```rust
#[derive(Clone)]
struct FakeGoogleState {
    forms: FakeGoogleForms,
    child_queries: Arc<tokio::sync::Mutex<Vec<HashMap<String, String>>>>,
}

async fn fake_google_children(
    State(state): State<FakeGoogleState>,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
) -> Json<Value> {
    state.child_queries.lock().await.push(query.clone());
    match query.get("pageToken").map(String::as_str) {
        Some("google-provider-page-2") => Json(json!({
            "files":[
                {"id":"media-folder","name":"Media duplicate","mimeType":"application/vnd.google-apps.folder","trashed":false},
                {"id":"archive-folder","name":"Archive","mimeType":"application/vnd.google-apps.folder","trashed":false}
            ],
            "nextPageToken":"google-provider-page-3"
        })),
        Some("google-provider-page-3") => Json(json!({"files":[]})),
        _ => Json(json!({
            "files":[{"id":"media-folder","name":"Media","mimeType":"application/vnd.google-apps.folder","trashed":false}],
            "nextPageToken":"google-provider-page-2"
        })),
    }
}
```

The Microsoft state has the same `forms` and `child_queries` fields plus
`graph_api_base: String`. Bind the listener first, build
`graph_api_base = format!("http://{address}/graph/v1.0/")`, then construct the router and
state. Its handler returns
`format!("{}drives/personal-drive/items/root-item/children?$skiptoken=page-2", state.graph_api_base)`.
Never use `graph.microsoft.com` in this test because the adapter validates the configured
origin.

Expose the same `child_queries` `Arc` on `FakeGoogle` and `FakeMicrosoft` themselves so
route assertions can inspect counts after the server has been started.

- [ ] **Step 2: Write failing route assertions before server changes**

Extend both OAuth tests to assert:

```rust
let first_cursor = Uuid::parse_str(directories["NextPageToken"].as_str().unwrap()).unwrap();
assert!(!directories.to_string().contains("google-provider-page-2"));

let second = json_request(
    &app,
    "GET",
    &format!(
        "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=MyDrive&PageToken={first_cursor}"
    ),
    Some(&token),
    None,
).await;
assert_eq!(second.status(), StatusCode::OK);
let second: Value = serde_json::from_slice(
    &second.into_body().collect().await.unwrap().to_bytes(),
).unwrap();
assert_eq!(second["Items"], json!([
    {"Id":"media-folder","Name":"Media duplicate"},
    {"Id":"archive-folder","Name":"Archive"}
]));
let second_cursor = Uuid::parse_str(second["NextPageToken"].as_str().unwrap()).unwrap();
assert!(!second.to_string().contains("google-provider-page-3"));

let replay = json_request(
    &app,
    "GET",
    &format!(
        "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=MyDrive&PageToken={first_cursor}"
    ),
    Some(&token),
    None,
).await;
let replay: Value = serde_json::from_slice(
    &replay.into_body().collect().await.unwrap().to_bytes(),
).unwrap();
assert_eq!(replay["NextPageToken"], second_cursor.to_string());

let calls_before_invalid = fake.child_queries.lock().await.len();
let wrong_parent = json_request(
    &app,
    "GET",
    &format!(
        "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=MyDrive&ParentId=other&PageToken={first_cursor}"
    ),
    Some(&token),
    None,
).await;
assert_eq!(wrong_parent.status(), StatusCode::BAD_REQUEST);
assert_eq!(fake.child_queries.lock().await.len(), calls_before_invalid);

let (other_token, _) = login_with_user(&app).await;
let wrong_owner = json_request(
    &app,
    "GET",
    &format!(
        "/Admin/Storage/OAuth/GoogleDrive/{oauth_state}/Directories?Scope=MyDrive&PageToken={first_cursor}"
    ),
    Some(&other_token),
    None,
).await;
assert_eq!(wrong_owner.status(), StatusCode::FORBIDDEN);
```

Add replay, final-page, malformed UUID, wrong parent/scope, second OAuth state, and second login-session assertions. Compare fake query counts before and after every invalid request to prove failure occurs before provider I/O. Mirror the same contract for OneDrive and assert no response contains the loopback `@odata.nextLink`, `$skiptoken`, or fake Graph base URL.

- [ ] **Step 3: Run the two route tests to prove red**

Run:

```text
cargo test -p tjxy-server --test storage_admin_routes google_drive_oauth_uses_server_side_pkce_and_persists_only_encrypted_credentials --locked
cargo test -p tjxy-server --test storage_admin_routes onedrive_oauth_derives_personal_identity_and_never_accepts_browser_credentials --locked
```

Expected: fail because directory responses lack `NextPageToken` and UUID `PageToken` is ignored/rejected.

- [ ] **Step 4: Add provider-specific cursor contexts and session registries**

Import the registry and define exact contexts:

```rust
use crate::storage_admin_cursor::DirectoryPageCursorRegistry;

#[derive(Clone, Debug, Eq, PartialEq)]
struct GoogleDirectoryPageContext {
    scope: GoogleDriveScope,
    parent_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OneDriveDirectoryPageContext {
    drive_id: String,
    parent_id: String,
}
```

Add the corresponding `directory_cursors` field to each OAuth session and initialize it with `DirectoryPageCursorRegistry::default()` in `begin`. Add provider-specific `prepare_directory_page` and `register_directory_page` methods that:

- purge expired sessions;
- validate owner first;
- require authorized status before cursor lookup;
- clone credentials/drive data and resolve the provider token;
- register the returned provider token only after I/O under a fresh lock;
- map registry lookup failure to `StorageAdminError::InvalidRequest`.

Use these signatures so handlers cannot accidentally keep a lock guard:

```rust
async fn prepare_directory_page(
    &self,
    state: Uuid,
    owner_session_id: Uuid,
    context: &GoogleDirectoryPageContext,
    cursor: Option<Uuid>,
) -> Result<(GoogleOAuthCredentials, Option<PageToken>), StorageAdminError>;

async fn register_directory_page(
    &self,
    state: Uuid,
    owner_session_id: Uuid,
    context: GoogleDirectoryPageContext,
    provider_token: Option<PageToken>,
) -> Result<Option<Uuid>, StorageAdminError>;
```

The OneDrive prepare method reads the authorized `MicrosoftPersonalDrive` under the lock
and returns only the cloned credentials, concrete drive ID, normalized parent, context,
and provider page needed after the lock is released.

Use a concrete prepared request so parent normalization and cursor context cannot diverge:

```rust
struct PreparedOneDriveDirectoryPage {
    credentials: MicrosoftOAuthCredentials,
    drive_id: String,
    parent: StorageObjectId,
    context: OneDriveDirectoryPageContext,
    provider_page: Option<PageToken>,
}

async fn prepare_directory_page(
    &self,
    state: Uuid,
    owner_session_id: Uuid,
    parent_id: Option<String>,
    cursor: Option<Uuid>,
) -> Result<PreparedOneDriveDirectoryPage, StorageAdminError>;

async fn register_directory_page(
    &self,
    state: Uuid,
    owner_session_id: Uuid,
    context: OneDriveDirectoryPageContext,
    provider_token: Option<PageToken>,
) -> Result<Option<Uuid>, StorageAdminError>;
```

- [ ] **Step 5: Implement strict query and response contracts**

Change both directory query DTOs and the shared response:

```rust
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub(crate) struct OneDriveDirectoryQuery {
    parent_id: Option<String>,
    page_token: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub(crate) struct GoogleDriveDirectoryQuery {
    scope: GoogleScope,
    shared_drive_id: Option<String>,
    parent_id: Option<String>,
    page_token: Option<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct GoogleDriveDirectoryResponse {
    items: Vec<GoogleDriveDirectoryDto>,
    next_page_token: Option<Uuid>,
}
```

Each handler must normalize `StorageObjectId` and context, prepare credentials/token, call `backend.list_children(&parent, provider_page).await` without a session lock, register `page.next_page`, and serialize only directory objects plus the TJXY cursor. Destructure `ObjectPage` before consuming objects so the next token is never dropped.

- [ ] **Step 6: Run focused route and surrounding server tests**

Run: `cargo test -p tjxy-server --test storage_admin_routes --locked`

Expected: all storage Admin route tests pass.

Run: `cargo test -p tjxy-server --test startup --locked`

Expected: startup/OAuth construction remains green.

- [ ] **Step 7: Commit server pagination**

```bash
git add crates/server/src/storage_admin.rs crates/server/tests/storage_admin_routes.rs
git commit -m "feat: paginate cloud storage directories"
```

---

### Task 3: Frontend Directory Page Boundary

**Files:**
- Modify: `admin/src/storage/googleDriveApi.ts:14-155`
- Modify: `admin/src/storage/googleDriveApi.test.ts:90-180`

**Interfaces:**
- Consumes: server `{ Items, NextPageToken }` with UUID directory cursors.
- Produces: `StorageChoicePage`, `GoogleDirectoryRequest.pageToken`, `OneDriveDirectoryRequest`, and page-returning list functions.

- [ ] **Step 1: Write failing API adapter tests**

Change directory mocks and expectations to pages:

```ts
const next = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
requestMock.mockResolvedValue({
  Items: [{ Id: 'folder-1', Name: 'Shows' }],
  NextPageToken: next,
});

await expect(listGoogleDirectories('oauth-state', {
  scope: 'SharedDrive', sharedDriveId: 'drive-1', parentId: 'parent/1', pageToken: next,
})).resolves.toEqual({
  items: [{ id: 'folder-1', name: 'Shows' }], nextPageToken: next,
});
expect(requestMock).toHaveBeenCalledWith(
  `/Admin/Storage/OAuth/GoogleDrive/oauth-state/Directories?Scope=SharedDrive&SharedDriveId=drive-1&ParentId=parent%2F1&PageToken=${next}`,
);
```

Add the equivalent OneDrive object request and rejection cases for missing, non-UUID, control-character, and non-null/non-string `NextPageToken` values.

Use explicit strict-response assertions:

```ts
for (const nextPageToken of [undefined, 'provider-token', 'bad\ncursor', 42, {}]) {
  requestMock.mockResolvedValueOnce({ Items: [], NextPageToken: nextPageToken });
  await expect(listOneDriveDirectories('oauth-state')).rejects.toMatchObject({
    category: 'invalid-response',
  });
}
```

- [ ] **Step 2: Run the adapter test to prove red**

Run: `cd admin && npm test -- --run src/storage/googleDriveApi.test.ts`

Expected: fail because both directory functions return arrays and accept no cursor field/object.

- [ ] **Step 3: Implement one strict directory-page parser**

Import `validUuid` from `../api/responseValidation` and add:

```ts
export interface StorageChoicePage {
  items: GoogleDriveChoice[];
  nextPageToken: string | null;
}

export interface GoogleDirectoryRequest {
  scope: GoogleDriveScope;
  sharedDriveId?: string;
  parentId?: string;
  pageToken?: string;
}

export interface OneDriveDirectoryRequest {
  parentId?: string;
  pageToken?: string;
}

function toDirectoryPage(value: unknown, subject: string): StorageChoicePage {
  if (!isRecord(value) || !Array.isArray(value.Items)) throw invalidResponse(subject);
  const nextPageToken = value.NextPageToken;
  if (nextPageToken !== null && !validUuid(nextPageToken)) {
    throw invalidResponse(`${subject} pagination`);
  }
  return {
    items: value.Items.map(toChoice),
    nextPageToken,
  };
}
```

Replace the existing `GoogleDrivePage` response interface with `StorageChoicePage` and use
it for Shared Drive and directory page return types. Encode optional `PageToken` after the
existing scope/parent fields. Make
`listOneDriveDirectories(state, request: OneDriveDirectoryRequest = {})` use the same
directory parser. Keep `listSharedDrives` on its existing provider-token parser because
that contract is explicitly out of scope.

- [ ] **Step 4: Run adapter tests, typecheck, and lint**

Run:

```text
cd admin && npm test -- --run src/storage/googleDriveApi.test.ts
cd admin && npm run typecheck
cd admin && npm run lint
```

Expected: adapter tests and lint pass. Because Tasks 4-5 deliberately migrate the two
component consumers after this boundary change, the full typecheck is expected to report
only the old array-return assumptions in `GoogleDrivePage`/`OneDrivePage` and their tests;
it must pass after Task 5.

- [ ] **Step 5: Commit the frontend HTTP boundary**

```bash
git add admin/src/storage/googleDriveApi.ts admin/src/storage/googleDriveApi.test.ts
git commit -m "feat(admin): expose cloud directory pages"
```

---

### Task 4: Google Drive Folder Pagination Interaction

**Files:**
- Modify: `admin/src/storage/GoogleDrivePage.tsx:45-405`
- Modify: `admin/src/storage/GoogleDrivePage.test.tsx`
- Create: `admin/src/storage/directoryChoices.ts`
- Create: `admin/src/storage/directoryChoices.test.ts`

**Interfaces:**
- Consumes: `listGoogleDirectories(...): Promise<StorageChoicePage>` from Task 3.
- Produces: append/dedupe/load-more behavior with cursor reset on folder, scope, and Shared Drive navigation.

- [ ] **Step 1: Update fixture pages and write failing interaction tests**

Use UUID cursors and add a pagination contract:

```tsx
directoriesMock
  .mockResolvedValueOnce({
    items: [{ id: 'folder-1', name: 'Shows' }],
    nextPageToken: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11',
  })
  .mockResolvedValueOnce({
    items: [
      { id: 'folder-1', name: 'Shows duplicate' },
      { id: 'folder-2', name: 'Archive' },
    ],
    nextPageToken: null,
  });

await screen.findByRole('combobox', { name: 'Target library' });
await user.click(screen.getByRole('button', { name: 'Authorize Google Drive' }));
await user.click(screen.getByRole('button', { name: 'Check authorization' }));
await user.click(screen.getByRole('button', { name: 'Load more folders' }));
expect(directoriesMock).toHaveBeenLastCalledWith('oauth-state', {
  scope: 'MyDrive', pageToken: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11',
});
expect(screen.getByRole('button', { name: 'Open Shows' })).toBeVisible();
expect(screen.getByRole('button', { name: 'Open Archive' })).toBeVisible();
expect(screen.queryByRole('button', { name: 'Load more folders' })).not.toBeInTheDocument();
```

Add a failure/retry test proving current folders and cursor remain after rejection, and a navigation test proving opening a folder replaces the root cursor before another load-more call. Keep the existing Shared Drive load-more test state separate.

For failure preservation, use a first page with one folder and a UUID cursor, reject the
next call, then click the still-visible button again after supplying a terminal mock. Assert
both load-more calls use the same root cursor and `Open Shows` remains visible throughout.

Add a focused contract for the moved helper before moving it:

```ts
import { uniqueChoices } from './directoryChoices';

it('keeps first-seen folder order while removing duplicate identifiers', () => {
  expect(uniqueChoices([
    { id: 'folder-1', name: 'Shows' },
    { id: 'folder-1', name: 'Renamed duplicate' },
    { id: 'folder-2', name: 'Archive' },
  ])).toEqual([
    { id: 'folder-1', name: 'Shows' },
    { id: 'folder-2', name: 'Archive' },
  ]);
});
```

- [ ] **Step 2: Run the Google component test to prove red**

Run: `cd admin && npm test -- --run src/storage/GoogleDrivePage.test.tsx`

Expected: fail because the page does not store or render directory continuation state.

- [ ] **Step 3: Implement Google directory pagination**

Make busy states unambiguous:

```ts
type BusyOperation =
  | 'start' | 'verify' | 'browse' | 'shared-more' | 'directory-more' | 'bind' | null;
```

Move the existing `uniqueChoices` function unchanged into `directoryChoices.ts` and import
it into the Google component. Add `nextDirectoryPage`, replace items/token after verify
and every `loadFolder`, and append via that shared helper in `loadMoreDirectories`. Build
the request from the current `scope`, `sharedDriveId`, and `currentFolder`; omit `parentId`
for My Drive root and include the TJXY cursor.

Render an outlined MUI button below the folder box:

```tsx
{nextDirectoryPage !== null && (
  <Button
    variant="outlined"
    aria-label="Load more folders"
    startIcon={busy === 'directory-more'
      ? <CircularProgress size={18} />
      : <RefreshOutlined />}
    disabled={busy !== null}
    onClick={() => void loadMoreDirectories()}
  >
    Load more
  </Button>
)}
```

Show `No folders on this page.` when items are empty but a cursor exists, and `No child folders.` only when both are empty. Preserve list dimensions and keep the separate Shared Drive button tied to `shared-more`.

- [ ] **Step 4: Run Google component and frontend gates**

Run:

```text
cd admin && npm test -- --run src/storage/directoryChoices.test.ts src/storage/GoogleDrivePage.test.tsx
cd admin && npm run typecheck
cd admin && npm run lint
```

Expected: focused Google tests and lint pass. Full typecheck may still report only the
unmigrated OneDrive consumer from Task 5; it must pass after Task 5.

- [ ] **Step 5: Commit Google interaction**

```bash
git add admin/src/storage/directoryChoices.ts admin/src/storage/directoryChoices.test.ts admin/src/storage/GoogleDrivePage.tsx admin/src/storage/GoogleDrivePage.test.tsx
git commit -m "feat(admin): paginate Google Drive folders"
```

---

### Task 5: OneDrive Folder Pagination Interaction

**Files:**
- Modify: `admin/src/storage/OneDrivePage.tsx:31-270`
- Modify: `admin/src/storage/OneDrivePage.test.tsx`

**Interfaces:**
- Consumes: `listOneDriveDirectories(state, OneDriveDirectoryRequest): Promise<StorageChoicePage>` from Task 3.
- Produces: OneDrive empty-page continuation, append/dedupe, final-page, retry, and navigation-reset behavior.

- [ ] **Step 1: Update existing mocks and write failing empty-page/failure contracts**

First prove that a folder page containing only provider files remains navigable:

```tsx
directoriesMock
  .mockResolvedValueOnce({
    items: [],
    nextPageToken: '028f17ac-4e99-7ec5-b4fd-8f15ca9f4f12',
  })
  .mockResolvedValueOnce({
    items: [{ id: 'folder-1', name: 'Shows' }],
    nextPageToken: null,
  });

await screen.findByRole('combobox', { name: 'Target library' });
await user.click(screen.getByRole('button', { name: 'Authorize OneDrive' }));
await user.click(screen.getByRole('button', { name: 'Check authorization' }));
expect(screen.getByText('No folders on this page.')).toBeVisible();
await user.click(screen.getByRole('button', { name: 'Load more folders' }));
expect(directoriesMock).toHaveBeenLastCalledWith('oauth-state', {
  pageToken: '028f17ac-4e99-7ec5-b4fd-8f15ca9f4f12',
});
expect(await screen.findByRole('button', { name: 'Shows' })).toBeVisible();
```

Add a rejection followed by retry and assert the same cursor is sent twice, existing items remain visible, and the error notification is explicit. Update the existing browse test to the object request `{ parentId: 'folder-1' }` and page response shape.

The retry assertion must pin the cursor rather than only the call count:

```tsx
expect(directoriesMock).toHaveBeenNthCalledWith(2, 'oauth-state', {
  pageToken: '038f17ac-4e99-7ec5-b4fd-8f15ca9f4f13',
});
expect(directoriesMock).toHaveBeenNthCalledWith(3, 'oauth-state', {
  pageToken: '038f17ac-4e99-7ec5-b4fd-8f15ca9f4f13',
});
expect(screen.getByRole('button', { name: 'Shows' })).toBeVisible();
expect(notify).toHaveBeenCalledWith(expect.any(String), { type: 'error' });
```

- [ ] **Step 2: Run the OneDrive component test to prove red**

Run: `cd admin && npm test -- --run src/storage/OneDrivePage.test.tsx`

Expected: fail because the component still consumes arrays and has no load-more control.

- [ ] **Step 3: Implement OneDrive page state and UI**

Add `'more'` to `BusyOperation`, `nextDirectoryPage` state, and the same append/dedupe
semantics as Google. Import `uniqueChoices` from `directoryChoices.ts`; do not create a
second implementation.

Use object requests:

```ts
const page = await listOneDriveDirectories(oauth.state, {
  ...(folder.id === 'root' ? {} : { parentId: folder.id }),
});
setPath(nextPath);
setDirectories(page.items);
setNextDirectoryPage(page.nextPageToken);
```

Add the same `aria-label="Load more folders"` button under the existing list. During load more, retain existing rows and disable navigation/binding. Do not show `No subfolders` until the cursor is null.

- [ ] **Step 4: Run all frontend tests and production checks**

Run:

```text
cd admin && npm test -- --run
cd admin && npm run typecheck
cd admin && npm run lint
cd admin && npm run build
```

Expected: all pass.

- [ ] **Step 5: Commit OneDrive interaction**

```bash
git add admin/src/storage/OneDrivePage.tsx admin/src/storage/OneDrivePage.test.tsx
git commit -m "feat(admin): paginate OneDrive folders"
```

---

### Task 6: Compatibility Evidence, Full Verification, And Review

**Files:**
- Modify: `docs/api-parity.md:53,66-67`
- Modify: `README.md:60-70`
- Modify: `docs/superpowers/plans/2026-07-28-cloud-directory-pagination.md`

**Interfaces:**
- Consumes: verified server and Admin pagination behavior from Tasks 1-5.
- Produces: accurate compatibility evidence and a reviewed, release-ready slice.

- [ ] **Step 1: Update nearby compatibility documentation**

Record that My Drive, Shared Drive, and OneDrive Personal folder pickers now paginate with server-owned OAuth-session cursors. Keep these limitations explicit:

- Shared Drive list pagination remains its existing contract;
- account status and online reauthorization/rotation are not implemented;
- Business/SharePoint remains rejected;
- deterministic fake-provider tests are not live Google/Microsoft acceptance.

Do not change a `⚠️` row to `✅` when the row still contains unrelated incomplete storage behavior.

- [ ] **Step 2: Run focused Rust and frontend verification**

Run:

```text
cargo test -p tjxy-server storage_admin_cursor --locked
cargo test -p tjxy-server --test storage_admin_routes --locked
cargo test -p tjxy-server --test startup --locked
cd admin && npm test -- --run
cd admin && npm run typecheck
cd admin && npm run lint
cd admin && npm run build
```

Expected: all pass. Record exact counts and any ignored tests in this plan's status section.

- [ ] **Step 3: Run workspace quality gates**

Run:

```text
cargo test --workspace --locked -- --test-threads=1
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Expected: all pass. Use serialized workspace tests because the preceding completed slice documented non-reproducible shared-database/background-worker races under default parallel execution.

- [ ] **Step 4: Inspect the UI if layout behavior changed unexpectedly**

If component output, build warnings, or a manual code review indicates wrapping/overlap risk, start the Admin development server and use the repository Playwright workflow to inspect desktop and mobile screenshots. Otherwise record that no new viewport structure was introduced: the new button uses the already-covered storage-wizard stack and stable folder container.

- [ ] **Step 5: Perform independent two-axis review**

Use `superpowers:requesting-code-review` after all gates pass. Require reviewers to check:

- spec compliance: every cursor/context/error/UI/test requirement;
- code quality: lock lifetime, bounded memory, provider-token leakage, replay behavior, stale state, error preservation, and unnecessary abstraction.

Fix every Critical or Important finding with a focused regression test and rerun affected gates. Record Minor findings and residual risk explicitly.

- [ ] **Step 6: Commit documentation and evidence**

```bash
git add docs/api-parity.md README.md docs/superpowers/plans/2026-07-28-cloud-directory-pagination.md
git commit -m "docs: record cloud directory pagination"
```

- [ ] **Step 7: Re-audit the next PLAN slice**

Confirm this slice closes only cloud directory truncation. Then select the next evidence-backed item from the completed audit in this order unless new evidence changes priority:

1. media `If-None-Match`/304 conditional request contract;
2. real TCP downstream disconnect cancellation contract;
3. pinned Jellyfin OpenAPI subset/checksum provenance gate;
4. cloud account status/reauthorization and credential operations;
5. shared identity/import-conflict workflow before migration UI or Emby DB import.

Do not mark the parent `PLAN.md` goal complete until every §22 release gate and explicitly planned v1 Admin/import capability has direct current-state evidence.
