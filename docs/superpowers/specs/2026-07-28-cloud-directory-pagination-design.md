# Cloud Directory Pagination Design

## Status

Approved design for completing paginated Google Drive and OneDrive folder selection in
the React Admin storage-binding workflow. This slice fixes a confirmed production gap:
both provider backends already expose paginated child enumeration, but the Admin routes
always request the first page and discard the continuation.

## Context

`StorageBackend::list_children` already accepts an optional provider `PageToken` and
returns an `ObjectPage` with an optional next token. The Google Drive adapter maps this to
Drive API `pageToken` and `nextPageToken`. The OneDrive adapter follows the full Microsoft
Graph `@odata.nextLink` after validating its origin.

The server's Google and OneDrive directory handlers currently call
`list_children(parent, None)` and serialize only `Items`. Their Admin clients return a
naked array, so folders beyond the first provider page cannot be selected. This affects
large My Drive, Shared Drive, and OneDrive Personal directories even though the adapters
themselves implement pagination correctly.

Current provider documentation reinforces two different upstream representations:

- Google Drive continues `files.list` with the previous response's `nextPageToken` until
  no token is returned. Shared Drive enumeration also carries its drive scope parameters.
- Microsoft Graph returns a complete `@odata.nextLink`. Clients must request that complete
  URL unchanged, treat it as opaque, and stop when the property is absent.

Passing those values directly through the Admin API would expose provider-specific
continuation details, including a full Graph URL, and would make the browser responsible
for preserving a backend contract that `StorageBackend` intentionally abstracts.

## Decision

Add server-owned, opaque directory cursors to the existing OAuth-session workflows. The
browser receives only a random TJXY UUID. The corresponding OAuth session stores the raw
provider token together with the exact directory context in which it was issued.

Both directory routes accept an optional PascalCase `PageToken` query parameter and return
the same response shape:

```json
{
  "Items": [
    { "Id": "provider-folder-id", "Name": "Folder" }
  ],
  "NextPageToken": "tjxy-cursor-uuid-or-null"
}
```

`NextPageToken` is additive to the existing response. First-page callers omit
`PageToken`. A subsequent request repeats the same scope, Shared Drive ID, and parent ID
and supplies the TJXY cursor.

The existing paginated Google Shared Drives endpoint is not changed by this slice. Its
pagination already works, its continuation is not a Graph URL, and changing that separate
contract is unnecessary to fix directory truncation.

### Alternatives Considered

1. Server-owned opaque cursors are selected. They preserve the provider-neutral Admin
   boundary, keep Graph URLs out of the browser, and let the server validate context.
2. Passing provider tokens directly to the browser was rejected. It is smaller, but leaks
   the complete OneDrive next URL and couples Admin to provider-specific continuation
   formats.
3. Automatically fetching every provider page on the server was rejected. Large folders
   would create unbounded latency, memory use, API traffic, and rate-limit exposure before
   the administrator can interact with the page.

## Scope

This slice includes:

- paginated Google My Drive directory selection;
- paginated Google Shared Drive directory selection;
- paginated OneDrive Personal directory selection;
- server-owned cursor storage and context validation within OAuth sessions;
- bounded React Admin "Load more" interactions for both providers;
- server, API-adapter, component, and documentation coverage.

This slice excludes Shared Drive list pagination changes, background object inventory,
storage-account reauthorization, Business/SharePoint support, folder search, virtualized
lists, and persistence of browser cursors across OAuth-session expiry.

## Cursor Model

Each Google and OneDrive OAuth session owns a cursor registry. A registry entry contains:

- a random cursor UUID returned to Admin;
- the provider `PageToken`, retained only in server memory;
- the normalized directory context;
- a session-local monotonic insertion order for bounded eviction.

Google directory context contains the concrete scope (`MyDrive` or one specific Shared
Drive ID) and normalized parent provider-object ID. OneDrive directory context contains
the discovered Personal drive ID and normalized parent provider-object ID.

The OAuth state already selects one session and is bound to one authenticated login
session. Cursor lookup adds the directory-context check. Therefore:

- a cursor from another OAuth state is unknown;
- an OAuth state owned by another login session remains forbidden;
- changing the parent, Drive scope, or Shared Drive ID invalidates the cursor;
- session expiry or successful binding invalidates every cursor with the session.

Malformed, unknown, expired, or context-mismatched cursors return HTTP 400 without calling
the provider. An OAuth state owned by another authenticated session continues to return
HTTP 403. An authorization callback that has not completed continues to return HTTP 409.
These responses reveal no provider token or cursor-registry detail.

## Bounded And Replay-Safe Storage

Each OAuth session retains at most 256 directory cursor entries. Registration first
reuses an existing entry with the same provider token and context, so retrying the same
page returns the same TJXY cursor. If a new entry would exceed the bound, the oldest entry
by insertion order is evicted before insertion. An evicted cursor subsequently fails as an
unknown cursor.

Cursors are not single-use. A page request may be retried safely for the lifetime of the
OAuth session, and a failed network request does not consume its input cursor. The ten
minute OAuth-session TTL remains the outer lifetime bound; no new cleanup task or durable
table is introduced.

The bound covers substantially more interactive navigation than the binding wizard needs,
while preventing a caller from growing server memory for the lifetime of a session.

## Server Flow

For either provider, the directory handler follows this sequence:

1. Authenticate an administrator and require a login-session origin.
2. Resolve and validate provider scope and the effective parent ID.
3. Lock the OAuth session, verify owner and authorized status, clone the credentials, and
   resolve an optional TJXY cursor against the exact directory context.
4. Release the session lock before constructing the backend or making a network request.
5. Call `list_children` with the recovered provider token.
6. If the provider returned another token, lock the session again, revalidate that the
   same authorized OAuth session still exists, and register or reuse a TJXY cursor.
7. Serialize directory objects plus the TJXY `NextPageToken`.

No OAuth-session mutex is held across provider I/O. If the session expires or is consumed
by binding while the provider request is in flight, registration fails explicitly and no
raw continuation is returned. Provider errors retain the existing sanitized HTTP mapping.

The provider `PageToken` remains opaque at the server boundary. Google tokens are passed
back to Google unchanged. The complete validated Microsoft Graph `@odata.nextLink` remains
inside the OneDrive backend and is passed back unchanged; it is never parsed or rebuilt by
the server handler.

## React Admin Flow

The storage API adapter exposes one page shape with `items` and `nextPageToken`. Both
directory list functions accept an optional TJXY page token and validate the PascalCase
response before returning it. The response cursor reuses the Admin application's existing
UUID validator rather than accepting an arbitrary bounded string.

Each storage page keeps `nextDirectoryPage` alongside its current folder list:

- authorization verification or opening a folder replaces both items and cursor;
- switching Google scope or Shared Drive also replaces both;
- "Load more" repeats the current folder context with the cursor, appends results, removes
  duplicate IDs while preserving first-seen order, and stores the returned cursor;
- a failed page leaves existing items and the current cursor unchanged;
- the button disappears only when `nextPageToken` is null;
- all navigation and binding controls remain disabled during the request.

Provider pages contain files and folders while the server returns only folders. A page may
therefore contain zero visible folders and still have a continuation. Admin must still show
the "Load more" command in that state; it reports an empty folder only when both the item
list is empty and the continuation is null.

The Google page retains its existing Shared Drive "Load more" control separately from the
new directory pagination state. The OneDrive busy-state union gains the corresponding
directory-page operation. Existing MUI list, button, icon, spacing, and responsive patterns
remain unchanged.

## Security And Failure Handling

Directory responses and errors must not contain raw Google continuation tokens, Microsoft
Graph hosts, query strings, access tokens, refresh tokens, credentials, or provider error
bodies. Cursor UUIDs are capabilities only within the already authenticated and
session-bound OAuth state; possession alone does not bypass owner or context checks.

Every error remains explicit. Invalid cursors fail before provider I/O, provider failures
retain the current 400/503 mapping, and cursor registration failure does not silently drop
the provider continuation or return a falsely terminal page.

## Test Strategy

Server route contracts cover both providers:

- first-page responses return provider folders and a UUID-shaped TJXY cursor;
- response bodies do not contain the raw Google token or Graph next URL;
- the next request causes the mock provider to receive the exact original continuation;
- final pages return `NextPageToken: null`;
- malformed, unknown, cross-parent, cross-scope, and cross-OAuth-state cursors fail before
  provider I/O;
- an OAuth state owned by another login session remains forbidden;
- replaying a cursor is supported and repeated next-token registration is stable;
- the 256-entry registry remains bounded and evicts the oldest entry deterministically.

Frontend API tests cover query encoding, PascalCase response validation, malformed cursor
responses, and the shared page shape. Component tests cover append ordering, duplicate-ID
removal, empty filtered pages with a continuation, cursor disappearance on the final page,
failure-state preservation, and cursor reset on navigation or Google scope changes.

Focused verification will run:

```text
cargo test -p tjxy-server --test storage_admin_routes --locked
cd admin && npm test -- --run
cd admin && npm run typecheck
cd admin && npm run lint
cd admin && npm run build
cargo fmt --all -- --check
git diff --check
```

After focused verification, run the relevant workspace Rust tests and clippy. Browser E2E
is required if component tests or screenshot inspection expose a layout uncertainty;
otherwise the interaction follows the already-covered storage binding layout and does not
add a new viewport structure.

## Documentation And Residual Risk

Update `docs/api-parity.md` and the nearby README Admin status when the implementation is
verified. The documentation may claim complete interactive folder pagination for My Drive,
Shared Drive, and OneDrive Personal. It must not claim account reauthorization, persistent
folder cursors, Business/SharePoint support, or live-provider acceptance.

Residual risks remain provider-side directory mutation between page requests, OAuth-session
expiry during slow navigation, and live Google/Microsoft behavior outside deterministic
adapter and server contracts. Provider cursors define their own snapshot semantics; TJXY
does not attempt to merge or stabilize a changing remote directory across pages.
