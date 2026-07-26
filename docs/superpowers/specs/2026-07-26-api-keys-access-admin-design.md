# API Keys And Access Admin Design

## Status

Approved design for the API Keys compatibility contract and the Devices/API Keys React
Admin workflow required by `PLAN.md` sections 4.2 and 16. This design closes the remaining
authentication-management gap without treating API keys as login sessions or broadening
the credential model beyond the v1 requirements.

## Context

TJXY already persists login sessions, authenticates canonical MediaBrowser tokens, and
implements the Jellyfin-compatible Devices endpoints. API key lifecycle endpoints and the
Devices/API Keys Admin page are absent. `docs/api-parity.md` therefore still marks this
area incomplete and it blocks treating real-client smoke testing as final release
acceptance.

The pinned Jellyfin OpenAPI contract has an important consequence: `POST /Auth/Keys`
returns `204` without a body, while `GET /Auth/Keys` returns each complete `AccessToken`.
A digest-only or one-time-reveal design would leave compatible clients unable to obtain a
new key after creation. TJXY must therefore retain recoverable key material while keeping
plaintext out of SQL.

Existing code already provides the required primitives: session tokens use 256 bits of
random UUID material and SHA-256 lookup digests, `CredentialCipher` provides versioned
AES-256-GCM envelopes with identity/provider-bound associated data, and
`TJXY_CREDENTIAL_KEYRING` supplies active and historical encryption keys.

## Decisions

Implement API keys as a separate credential kind with a separate durable table and an
explicit API-key authentication origin. Do not insert synthetic rows into
`auth_sessions`. API keys authenticate as the enabled administrator who created them,
subject to that user's current authorization revision, but they do not acquire a login
session identity.

Use the existing `CredentialCipher` directly. Do not reuse `storage_credentials`, whose
refresh and provider lifecycle semantics do not apply, and do not introduce a generic
credential ledger before another credential kind demonstrates that abstraction.

The React Admin application consumes the canonical `/Auth/Keys` and `/Devices` APIs from
one `/admin/access` page. No private Admin API or expansion of the Users-only React Admin
`dataProvider` is needed.

### Alternatives Considered

1. A separate `api_keys` table and principal origin is selected. It keeps key lifecycle,
   encryption, and session-only behavior explicit while preserving the existing session
   model.
2. Storing API keys as `auth_sessions` was rejected. It requires fabricated device and
   client data, pollutes Sessions and Devices, and can accidentally grant playback or
   OAuth session semantics.
3. A unified credential parent table was rejected for v1. It would migrate every active
   session and introduce a broad abstraction for one new credential kind.
4. Digest-only storage or one-time reveal was rejected because it cannot satisfy the
   canonical GET-after-POST Jellyfin workflow.

## Scope

This slice includes:

- canonical administrator-only `GET`, `POST`, and `DELETE /Auth/Keys` behavior;
- encrypted, recoverable API key persistence and digest-only authentication lookup;
- API key creator authorization-revision binding and immediate revocation;
- fail-closed startup validation for existing encrypted keys;
- an explicit distinction between session and API-key principals;
- a responsive Access page for Devices and API Keys;
- SQLite, PostgreSQL, MySQL, HTTP, startup, frontend, and production-browser contracts;
- compatibility and operator documentation updates.

This slice excludes key scopes, expiry, rotation workflows, soft deletion, audit-history
retention, bulk operations, and a generic access-grant framework. Those features are not
required by the current plan or pinned Jellyfin contract.

## Durable Model

Add an `api_keys` table with these fields:

| Column | Meaning |
|---|---|
| `id BIGINT` | Auto-incrementing Jellyfin-compatible `AuthenticationInfo.Id`. |
| `envelope_id UUID` | Unique identity used as `CredentialCipher` associated data. |
| `creator_user_id UUID` | Administrator represented by this key. |
| `creator_auth_revision BIGINT` | User authorization revision captured at creation. |
| `token_digest BINARY(32)/BLOB` | Unique SHA-256 lookup digest; binary on every backend. |
| `encrypted_payload BLOB` | AES-GCM nonce and ciphertext; never plaintext. |
| `key_version INTEGER` | Credential keyring version used for the envelope. |
| `app_name VARCHAR(256)` | Required Jellyfin `app` value. |
| `created_at TIMESTAMP WITH TIME ZONE` | Creation time. |
| `last_used_at TIMESTAMP WITH TIME ZONE NULL` | Throttled authentication activity time. |

`envelope_id` is unique and the cipher provider/AAD label is a versioned constant specific
to API access tokens. The creator foreign key is restrictive; the existing user-deletion
transaction explicitly deletes that user's API keys before deleting the user. API key
DELETE physically removes the row. Unknown keys are an idempotent success and do not form
an existence oracle.

At most 256 API keys may exist. The canonical list has no pagination controls and decrypts
every returned token, so this is both an operational and memory bound. App names may be
duplicated but must be non-empty, contain no control characters, and contain at most 256
Unicode scalar values.

## Authentication Model

`AuthenticatedPrincipal` gains an explicit origin:

```text
AuthenticationOrigin
  Session { session_id, device_id }
  ApiKey { api_key_id }
```

Both origins carry the resolved current user. A session remains valid only when its
stored `auth_revision` matches the user. An API key additionally requires the creator to
exist, remain enabled, remain an administrator, and retain the captured
`creator_auth_revision`. Every password, name, policy, disablement, or other user mutation
that advances the revision physically deletes that user's API keys in the same transaction.
The revision comparison remains a race fence, so a key selected concurrently with the
mutation cannot authenticate after commit. No cache or delayed revocation job is involved.

`authenticate_token` first performs the existing indexed session lookup. On a miss it
performs an indexed API-key digest lookup; authentication never decrypts the token. API-key
activity updates `last_used_at` no more frequently than once every three minutes, matching
the existing session activity write bound.

Ordinary resource and administrator endpoints can use either origin. Operations whose
meaning requires one concrete login session reject an API-key origin with
`AuthError::SessionRequired`, mapped to HTTP 403. This includes capability persistence,
logout, playback events and ping, and OAuth state creation/consumption. Reading a session
DeviceProfile for an API-key request returns no profile rather than fabricating one.
Sessions and Devices queries continue to read only `auth_sessions` and never expose API
keys.

## Service And Transaction Boundaries

`AuthService` owns the public application interface:

```text
list_api_keys(actor) -> sensitive API key records
create_api_key(actor, app_name) -> ()
delete_api_key(actor, raw_token) -> ()
authenticate_token(raw_token) -> authenticated principal
```

The service holds an optional shared `CredentialCipher`. Secret token wrapper types expose
plaintext only through a deliberately named adapter method and render `[REDACTED]` from
`Debug`. They do not implement ordinary `Display`.

Create validates input and generates the same 256-bit random token format used by login
sessions. It computes the SHA-256 digest and seals the raw value with a fresh envelope
UUID. One database transaction then revalidates the actor's enabled-administrator state
and authorization revision, verifies the capacity bound, and inserts the metadata,
digest, and envelope. A digest uniqueness failure fails closed; it never stores or returns
the token through an error.

List revalidates administrator status, reads at most 256 rows in deterministic newest-first
order, validates each envelope, and decrypts all records. Any unknown key version, malformed
envelope, failed authentication tag, or database error fails the whole operation. Partial
key lists are forbidden.

Delete validates the raw-token bound, computes its digest, revalidates administrator
status in the transaction, and physically deletes the matching row. It does not decrypt
the target and returns success when no row matches.

Existing user name, password, policy, and delete transactions remove every API key owned
by the affected user before advancing the authorization revision or deleting the user.
Because the operation is atomic, a committed user mutation never leaves an inactive API
key row for the canonical list to misrepresent as active.

## Startup And Keyring Behavior

An installation with no API keys may start without `TJXY_CREDENTIAL_KEYRING`; API key
create and list operations return 503 until a valid keyring is configured. Session
authentication remains available.

If any API key exists, startup must have a credential keyring and must authenticate every
bounded envelope before readiness. A missing keyring, missing historical version,
malformed envelope, swapped-row AAD, or failed authentication tag causes an explicit
startup error. Startup never deletes, disables, or silently omits an unreadable key.

Operators rotate the key-encryption key using the existing active-plus-historical keyring
mechanism. A dedicated API-key rewrap workflow is outside this slice; historical keys must
remain configured while their version is referenced.

## Canonical HTTP Contract

The server adds these administrator-only routes:

- `GET /Auth/Keys` returns `AuthenticationInfoQueryResult` with `Items`,
  `TotalRecordCount`, and `StartIndex=0`.
- `POST /Auth/Keys?app={name}` creates a key and returns `204` with no body.
- `DELETE /Auth/Keys/{raw-token}` deletes by the raw access token and returns `204`.

Each item exposes `Id`, complete `AccessToken`, `AppName`, creator `UserId` and `UserName`,
`IsActive=true`, `DateCreated`, optional `DateLastActivity`, and the nullable
device/version/revocation fields required by the pinned DTO. A golden test fixes exact
PascalCase field and null serialization.

GET responses include `Cache-Control: no-store`. API key lifecycle responses and errors
must not be cached. Authentication accepts API keys through the same canonical
MediaBrowser `Token` parameter and allowed `ApiKey` query aliases as session tokens.

Status mapping is explicit:

- 400 for malformed, duplicate, missing, empty, or overlong API-key parameters;
- 401 for missing or invalid caller credentials;
- 403 for non-administrators and session-required operations called by an API key;
- 409 when the bounded key capacity is reached;
- 503 for unavailable persistence, keyring, or authenticated decryption;
- 204 for successful create/delete, including deletion of an unknown well-formed key.

No handler, error, metric label, tracing field, Redis value, or response body outside the
canonical sensitive list may contain a raw key. The application does not install request
logging that records the DELETE URI. Tests assert that errors do not echo path tokens.

## React Admin Access Page

Add a custom authenticated `/admin/access` route and one Access navigation item. The page
uses two tabs rather than nested cards or placeholder destinations:

- Devices lists active devices, displays effective names and activity, edits the custom
  name through `/Devices/Options`, and revokes all selected-device sessions through the
  canonical DELETE endpoint after confirmation.
- API Keys lists app name, creation time, last use, and the access token from `/Auth/Keys`;
  creates through a focused app-name dialog; and deletes after confirmation.

Dedicated `deviceApi.ts` and `apiKeyApi.ts` adapters validate successful response shapes
and use the existing shared HTTP client. The Users-only `dataProvider` remains unchanged.
Vite adds exact `/Devices` and `/Auth` development proxy prefixes.

Keys are visually masked by default. Familiar reveal, hide, copy, edit, and delete icons
have tooltips and accessible labels. Reveal state is component memory only and resets on
refetch or unmount. Copy uses the browser clipboard API and never writes the key to local
or session storage. Creation refetches the canonical list because POST has no response
body. Errors identify the failed operation without including the key.

The page uses a compact desktop table and a stacked narrow-screen layout with stable
action dimensions. Confirmations name the app or device, not the secret. Controls must
fit without overlap at the production desktop viewport and at 390 CSS pixels.

## Verification

The work follows red-green-refactor at each public boundary.

Rust coverage includes:

- API DTO golden serialization and null behavior;
- migration up/down and required columns, indexes, foreign keys, and binary digest type;
- repository contracts for insert/list/delete, plaintext absence, digest lookup, capacity,
  deterministic order, activity throttling, swapped AAD, corrupted ciphertext, and user
  deletion;
- application contracts for administrator authorization, creator revision/disable/demote
  invalidation, API-key authentication, and every session-required operation;
- HTTP contracts for exact 200/204/400/401/403/409/503 behavior, `no-store`, canonical and
  legacy token transport, idempotent delete, and secret-free errors;
- startup contracts for absent, active, historical, missing, and corrupted keyring cases;
- a real TCP lifecycle proving create, GET recovery, authenticated use, restart recovery,
  delete, and immediate rejection.

Frontend coverage includes API adapter validation, both tab workflows, dialog errors,
mask/reveal/copy state, absence of Web Storage secrets, responsive layout, and navigation.
Production Playwright covers administrator login, device rename/revoke, API key creation,
recovery after refresh, API-key-authenticated request, deletion, and rejection after
deletion at desktop and mobile viewports.

The final automated gate runs formatting, workspace tests, warnings-denied Clippy, Admin
typecheck/lint/unit/build/e2e, PostgreSQL 17 contracts, MySQL 8.4 contracts, and the Redis
contract. After these checks and a fresh `PLAN.md` completion audit, real third-party
client/provider smoke testing can begin, but it is not represented as complete by this
slice alone.

## Documentation And Follow-up

Update `docs/api-parity.md` when the backend and Access page are verified. Update README
configuration text to explain that deployments need `TJXY_CREDENTIAL_KEYRING` before
creating API keys and must retain historical versions while encrypted records reference
them. Do not add placeholder navigation for the remaining section 16 pages.

The next incomplete `PLAN.md` areas remain storage status/reauthorization, metadata
operations, migration/conflict workflows, and the remaining v2.6 catalog/identity release
gates. They remain separate slices and must not be reported complete after this design is
implemented.
