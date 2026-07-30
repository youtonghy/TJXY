# TJXY HeroUI Media Client Design

**Date:** 2026-07-30

## Context

TJXY currently ships one browser application under `/admin/*`. The server already exposes
authenticated Jellyfin-compatible catalog, search, image, direct-play, subtitle, user-data,
and playstate routes, but there is no ordinary-user browser experience. The administrator
application already establishes the approved HeroUI v3, Tailwind CSS v4, React 19, semantic
color, focus, reduced-motion, and responsive conventions.

## Goals

- Add a production-connected ordinary-user application under `/app/*`.
- Reuse the approved HeroUI theme and shared primitives without putting `ra-core` in the
  ordinary-user state or authorization boundary.
- Cover login, home, library browsing, search, item detail, introduction and metadata,
  direct playback, subtitles, resume, favorites, played state, and logout.
- Keep long-lived login credentials out of image, media, and subtitle URLs.
- Preserve `/admin/*` behavior and deployment as one frontend build artifact.

## Non-Goals

- Registration, account recovery, user switching, or Quick Connect.
- Transcoding, remuxing, HLS, or browser playback of incompatible containers.
- Per-library ACLs, which are not present in the current server model.
- Invented ratings or metadata that TJXY has not stored authoritatively.

## Architecture

One Vite build contains two sibling route trees. `/admin/*` continues to render `CoreAdmin`.
`/app/*` renders a standalone `ClientApp` with a dedicated auth context, session storage keys,
API adapters, query state, and error boundary. Both route trees import the same HeroUI v3 and
Tailwind v4 stylesheet and semantic theme tokens. The frontend build publishes assets under
`/assets/*`; the Rust static router serves that directory and returns the same `index.html` only
for accepted HTML requests under `/admin/*` and `/app/*`. `/` redirects to `/app/`.

The ordinary client uses `tjxy.web.token` and `tjxy.web.deviceId` and sends `Client="TJXY Web"`.
It accepts enabled ordinary or administrator accounts and rejects disabled users. It does not
reuse the admin `authProvider`, administrator route guard, or `ra-core` controllers.

## Routes And Experience

- `/app/login`: readiness status, username/password form, password reveal, inline safe errors,
  duplicate-submit prevention, and validated return-target restoration.
- `/app/`: full-width library and media rows for Continue watching, Next up, Recently added,
  and Libraries. Empty rows do not render decorative containers.
- `/app/libraries/:id`: breadcrumb-backed, stable paginated media grid with a fixed 2:3 poster
  ratio, item-type filter, and explicit empty/error states.
- `/app/search?q=`: URL-owned debounced search, cancelation of stale requests, pagination, and
  poster results without per-result detail requests.
- `/app/items/:id`: real poster/backdrop when available, title, original title, year, runtime,
  item type, genres, studios, people, overview, favorite/played actions, and playback source
  summary. Missing metadata is omitted rather than replaced with sample values.
- `/app/play/:id`: native video controls, direct-play source selection, authenticated subtitle
  loading, resume position, prepare/retry state, progress reporting, and an explicit unsupported
  state for browser-incompatible sources.

Desktop uses a quiet sticky top navigation with brand, Home, Libraries, Search, and account
menu. Mobile uses the same information architecture in a HeroUI drawer. Media imagery provides
the visual subject; the shell does not use decorative gradients, oversized marketing heroes, or
nested cards.

## Catalog Contract

`BaseItemDto` is extended with nullable or empty authoritative fields for `OriginalTitle`,
`RunTimeTicks`, `Genres`, `Studios`, and `People`. A person contains `Name`, `Role`, and `Type`.
The catalog query repository reads these fields in bounded set-based queries for the page being
returned. Search hints receive `PrimaryImageTag`, `ProductionYear`, `MediaType`, and user data so
the client can render complete search tiles without N+1 detail reads.

Image elements never receive a session credential. The client fetches image bytes with the
normal Authorization header, creates object URLs, cancels stale requests, and revokes each URL
when replaced or unmounted. Poster requests remain lazy because the server currently exposes
original images only.

## Playback Authorization

An authenticated login session may call `POST /Items/{itemId}/PlaybackTicket` with
`MediaSourceId` and `PlaySessionId`. The server verifies that the source is currently visible and
playable for that principal, creates a 256-bit opaque ticket, stores only its SHA-256 digest, and
returns the ticket id, expiry, and local stream URL. A ticket is bound to one auth session, user,
item, media source, and play session. Its expiry is the earlier of the login-session expiry and
six hours after issue. At most 32 active tickets may exist per login session.

`GET|HEAD /Videos/{itemId}/stream` and `/Audio/{itemId}/stream` accept `PlaybackTicket` only when
normal Authorization is absent. Validation joins the active login session and rejects expired,
revoked, wrong-item, wrong-source, disabled-user, or authorization-revision mismatches. Ticket
plaintext is never persisted or logged. `DELETE /PlaybackTickets/{ticketId}` revokes the current
session's ticket; logout also makes every associated ticket unusable through the active-session
join. Diagnostics redact both the query value and canonical 64-hex credentials.

The stream response maps supported containers to correct media types while retaining Range,
HEAD, ETag, If-Range, 206, and 416 behavior. The client selects only sources advertised as direct
play and supported by the browser (`mp4`, `webm`, `mp3`, `m4a`, and `ogg` with compatible
codecs). An unsupported source produces a visible explanation and never starts telemetry.

External subtitles are fetched with Authorization and converted from SRT to a VTT Blob in the
client. Login tokens therefore remain absent from `<video>` and `<track>` URLs.

## Playstate

The player posts Started after the media element begins playing, Progress at most every 15
seconds and on pause/seek, and Stopped when leaving the route or ending. It restores
`PlaybackPositionTicks` after metadata is available and explicitly marks an item played only on
the ended event. Ticket revocation runs after Stopped and does not suppress the Stopped request
if revocation fails.

## Failure Handling

- A 401 clears only the ordinary-user session and returns to the exact safe `/app/*` target.
- A 403 preserves the session and renders an access-denied surface with Sign out.
- Catalog and image requests use AbortController; an aborted request is not shown as an error.
- Retained catalog data remains visible during refresh and is paired with a local stale alert.
- Empty PlaybackInfo retries three bounded times before showing an actionable unavailable state.
- Secret-bearing errors are mapped to fixed user-facing messages and never interpolate request
  bodies, tokens, ticket values, or raw server payloads.

## Verification

Rust contract tests cover migration portability and rollback, ticket issue/auth/revocation,
cross-user and cross-item denial, expiry, capacity, MIME, Range, and credential redaction. Vitest
covers response validation, ordinary-user auth, URL restoration, image URL cleanup, search race
cancelation, SRT conversion, source selection, resume, and playstate timing. Playwright covers
desktop, tablet, and mobile login-to-play flows, keyboard navigation, Axe, overflow, console and
HTTP failures, and deterministic visuals. A real-server smoke uses the repository's valid one
second MP4/SRT fixture to prove authenticated Range, seek, subtitle, progress, and logout.
