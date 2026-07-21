# React Admin Users Vertical Slice Design

## Status

Approved scope for the first React Admin vertical slice. This design implements the
administrator login and local-user management workflow from `PLAN.md` section 16. It
establishes the frontend and static-serving boundaries that later admin pages will reuse.

## Context

TJXY currently exposes the complete administrator Users API but has no frontend build,
static-file service, or browser authentication adapter. The first slice must exercise a
real operational workflow without implying that the remaining section 16 pages exist.
The backend still creates the initial administrator from deployment configuration, so a
browser bootstrap wizard cannot yet be implemented safely.

Current React Admin requires every record to expose `id`, while TJXY deliberately uses
PascalCase Jellyfin-compatible fields such as `Id`. Its Users mutations are separate
commands rather than one generic REST update. A custom provider is therefore required.

## Decision

Build a strict TypeScript React Admin application in `admin/` and serve its production
build from the TJXY origin under `/admin/`. Use a custom `authProvider`, a custom Users
`dataProvider`, and explicit password and policy command forms. Do not add permissive
CORS. Vite development proxies TJXY API paths to the Rust server so development and
production use the same origin semantics.

### Alternatives considered

1. A same-origin React Admin SPA with custom providers is selected. It matches the PLAN,
   keeps credentials on one origin, and precisely adapts TJXY command APIs.
2. A separately hosted cross-origin SPA was rejected for this slice because it requires
   an Authorization-aware CORS policy and creates a second deployment boundary without
   adding user value.
3. Rust-rendered administration pages were rejected because they contradict the explicit
   React Admin architecture decision and would create a temporary UI stack to replace.

## Scope

The slice includes:

- administrator login through `POST /Users/AuthenticateByName`;
- administrator verification and identity through `GET /Users/Me`;
- local user list, detail, create, rename, password reset, policy update, and delete;
- explicit disabled and administrator status presentation;
- responsive desktop and mobile layouts;
- production static assets and scoped SPA deep-link fallback under `/admin/`;
- unit, integration, production-build, and browser workflow tests;
- README commands and deployment behavior.

The slice excludes the initial-administrator wizard, devices, API keys, server-side
logout/revocation, and every other section 16 administration domain. Those require new
backend contracts and remain visible as incomplete in `docs/api-parity.md`.

## Frontend Architecture

`admin/` is an independent npm project with a committed lockfile. Vite builds to
`admin/dist`, uses `base: "/admin/"`, and proxies the exact TJXY API prefixes needed in
development. The Node build remains an explicit release step rather than being hidden in
Cargo `build.rs`.

The application has four focused boundaries:

- `api/httpClient.ts` owns MediaBrowser headers, JSON parsing, empty responses, and typed
  HTTP errors.
- `auth/authProvider.ts` owns login, administrator verification, identity, permissions,
  and browser token lifecycle.
- `api/dataProvider.ts` owns only React Admin resource operations and the `Id` to `id`
  record adaptation.
- `users/` owns presentation and invokes explicit password and policy command clients.

The Users resource performs client-side sorting and pagination because `GET /Users`
returns the bounded local-user collection without pagination metadata. It does not invent
bulk mutations or send unsupported filters to the server.

## Authentication And Security

Login sends the canonical MediaBrowser client identity header and the existing TJXY login
payload. The returned access token is kept in `sessionStorage`, limiting persistence to
the browser tab. Every authenticated request sends a canonical MediaBrowser token header.
After login, `/Users/Me` must confirm that the account is an enabled administrator before
the admin shell is shown.

`logout` clears browser state. It must not claim to revoke the durable server session,
because no revocation endpoint exists. A `401` clears browser authentication and returns
to login. A `403` preserves authentication and shows an authorization error. Tokens,
passwords, MediaBrowser headers, and raw backend error bodies are never logged.

The app does not use a cross-origin token cookie, add CORS, embed bootstrap credentials,
or expose API paths through generated static configuration.

## User Workflows

The default authenticated screen is a compact Users table with name, administrator state,
disabled state, and direct show/edit actions. Create is a dedicated form. Show is
read-only. Edit separates four commands:

- rename;
- reset or replace password;
- update administrator and disabled policy flags;
- delete with confirmation.

These commands are deliberately not merged into one optimistic React Admin `update`.
Each command reports its own success or failure and refreshes the current record after a
successful response. This prevents a later command failure from making a partially
successful multi-request form look atomic. The last-enabled-administrator conflict is
shown as a conflict and leaves the current record intact.

The interface is a restrained operational surface: a dense table on desktop, a readable
stacked row layout on narrow screens, neutral backgrounds, dark text, teal success/status
accents, and amber/red warnings. It uses familiar icons with tooltips, stable control
dimensions, accessible labels, visible focus, and no decorative dashboard cards.

## HTTP And Data Mapping

The provider maps TJXY `Id` to React Admin `id` while retaining the original fields. It
supports only the operations the Users UI needs:

- `getList`: `GET /Users`, local sort/page, `{ data, total }`;
- `getOne`: `GET /Users/{id}`, `{ data }`;
- `create`: `POST /Users/New`, `{ data }`;
- `update`: rename through `POST /Users?userId=...`, then refetch;
- `delete`: `DELETE /Users/{id}`, returning the prior record for React Admin.

Password and policy endpoints are typed command functions outside the generic provider.
Unsupported provider operations reject explicitly rather than silently succeeding.
Malformed successful responses are treated as errors.

## Static Serving

The Rust server gains an opt-in router constructor that accepts an admin distribution
directory. The production binary reads `TJXY_ADMIN_DIST_DIR`, defaulting to
`admin/dist`. Existing API-only router construction remains available for focused tests
and embedding.

The static service is nested only at `/admin`. `/admin` redirects to `/admin/`; real files
are served with correct content types; missing GET/HEAD paths inside `/admin/` fall back to
that directory's `index.html`. The fallback is never global, so unknown API routes retain
their existing 404 response and non-GET admin requests are not rewritten to HTML.

If the distribution directory or `index.html` is missing, startup fails explicitly in the
production binary instead of presenting a ready server with a broken required admin UI.
Tests can continue to use the API-only router when static assets are outside their scope.

## Error Handling

The HTTP client distinguishes network failure, malformed response, authentication,
authorization, validation, conflict, and dependency-unavailable status. User-facing
messages remain actionable but do not echo secrets or internal database errors. Form
submissions remain editable after failure. Mutations are pessimistic because the server
owns invariants such as the final enabled administrator.

Static-file initialization and production directory validation return explicit startup
errors. No exception is swallowed to preserve readiness.

## Verification

Frontend checks:

```text
npm --prefix admin ci
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build
```

Rust checks cover `/admin/`, a deep link, static content type, missing distribution files,
unchanged API authentication, and unknown API 404 behavior. Existing Users route tests
remain the backend contract.

Playwright runs against the production frontend build and a temporary TJXY database. It
covers failed login, administrator login, list, create, rename, policy change, password
change, refresh persistence, deletion, non-administrator denial, and desktop/mobile layout
screenshots. Browser tests also assert that no uncaught console error occurs and that no
element overlaps or overflows at the supported viewports.

The full release check includes Cargo formatting, workspace Clippy with warnings denied,
the relevant SQLite and PostgreSQL suites, and the frontend production build.

## Follow-up Boundary

Later section 16 pages reuse the authentication, HTTP, static-serving, and layout modules.
They must not be represented by placeholder navigation. Each new page begins only after
its backend list/detail/command and error-state contracts exist and are independently
tested.
