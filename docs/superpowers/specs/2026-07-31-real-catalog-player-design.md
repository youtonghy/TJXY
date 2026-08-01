# Real Catalog And Player Fixture Design

## Status

Approved on 2026-07-31. This design supersedes the demonstration-only playback
constraints in `2026-07-30-tmdb-demo-catalog-design.md` for the current development
database.

## Context

The current development catalog contains 100 Movies and 100 Series populated from real TMDB
metadata. Series structure is bounded to three Seasons and twelve Episodes per Season for
the development import. The two user-facing libraries are named `Movies` and `TV Shows`,
and every Movie and Episode has an active local fixture source. The ordinary-user player
exposes source selection, subtitle selection, playback telemetry, completion behavior, and
recoverable media errors.

TJXY already has the durable model required for this work:

- one CatalogItem can own multiple MediaSources;
- each MediaSource can expose independent locations, streams, resolution, codecs, and
  playback policy;
- external subtitles are source-owned and have stable delivery indices;
- playback sessions persist position ticks;
- user data persists favorite, played, and playback-position state.

A migration scopes provider IDs to their catalog item while retaining a lookup index for
provider-wide searches; TMDB Movie and Series IDs can otherwise collide.

## Goals

- Rename the user-facing libraries to `Movies` and `TV Shows` and remove all user-facing
  `Demo` and `metadata only` copy.
- Import at least 100 top-level Movies and at least 100 top-level Series from real TMDB
  records. Seasons and Episodes do not count toward the Series target.
- Preserve a bounded Season and Episode hierarchy for every selected Series.
- Attach one browser-playable short black-video source to every Movie and Episode.
- Attach multiple valid specifications, bilingual subtitles, and one intentionally damaged
  source to a bounded representative set of 12 playable titles.
- Add explicit source and subtitle selection to the HeroUI player.
- Persist playback start, progress, stop, and completion through TJXY's existing playstate
  and user-data routes.
- Display played, partial-progress, and favorite status on every shared poster tile.

## Non-Goals

- Production startup seeding, scheduled TMDB synchronization, or a production media
  generator.
- Transcoding, adaptive bitrate streaming, DRM, or remote playback.
- Browser-independent switching between embedded audio tracks in one container. Browser
  support for `audioTracks` is not sufficiently portable; different audio specifications
  are represented as selectable MediaSources.
- Fabricating metadata, imagery, playback progress, or favorites in the frontend.
- Committing large generated media files to the repository.

## Catalog Selection

Use a source-controlled manifest containing at least 100 unique Movie IDs and 100 unique
Series IDs. The manifest is derived once from TMDB's authenticated popular-list endpoints,
reviewed for valid detail responses, then frozen in the importer. Import runs remain
deterministic and idempotent even when TMDB's popularity ranking changes.

The importer continues to fetch localized details, classifications, credits, external IDs,
images, and the bounded Season/Episode structure. It retains bounded credits and artwork
rules from the previous rich-catalog design.

Library natural keys and deterministic item identifiers do not change. Re-running the
import renames the two existing libraries in place and updates the same CatalogItems rather
than creating parallel libraries or duplicate items.

The libraries use valid production schema policy values. `Demo`, `MetadataOnly`, and other
non-schema policy strings are removed from the publication.

## Development Media Fixture

The one-off media fixture command is explicit and development-only. It requires an absolute
fixture directory and the current development database URL. It never runs during normal
server startup.

The command publishes a small set of valid media templates from a checked-in H.264/AAC MP4
fixture:

- a short H.264/AAC MP4 default source;
- a short alternate MP4 source with an independent persisted specification label;
- English and Chinese WebVTT subtitle files;
- a zero-byte damaged source used only for error-state testing.

The fixture contains a black image and a silent audio track. It is a valid media file, not
a zero-byte placeholder. The command writes item-specific files while every storage object
retains its own provider identity.

Every Movie and Episode receives one default MP4 source. Twelve deterministic playable
items also receive the alternate source, two external subtitle tracks, and the damaged
source. Series and Season folder nodes remain non-playable.

The command publishes normal storage accounts, filesystem configuration, storage objects,
MediaSources, MediaLocations, MediaStreams, subtitles, and active Source publications. All
identifiers derive deterministically from the CatalogItem and fixture variant. Re-running
replaces the same fixture projection and does not duplicate sources.

The damaged source is deliberately marked as a test source and is never the default.
Selecting it reaches the normal media endpoint and produces a bounded player error. The
player can then return to the recommended source.

## Playback API

`PlaybackInfo` remains the source of truth for the available playback choices. Each source
adds bounded display metadata already persisted by the source model:

- edition;
- container;
- bitrate;
- runtime ticks;
- default status;
- video and audio stream codec, resolution, language, channels, profile, and level.

External subtitle streams expose language, format, delivery index, default status, and
forced status. The API never exposes provider object identifiers, local filesystem paths,
storage-account identifiers, or credentials.

The client issues a playback ticket only for the selected source. Switching source revokes
the previous ticket, issues a new ticket, updates the media element, and restores the
bounded prior position after metadata is ready.

Subtitle delivery remains protected by the normal authenticated TJXY route. The client
fetches a selected WebVTT subtitle with its authenticated API helper, creates a temporary
Blob URL, attaches one `track`, and revokes the Blob URL on selection change or unmount.
No credential is placed in a subtitle URL.

## Player Experience

The player remains based on the native HTML video controls for reliable play, pause, seek,
volume, fullscreen, and picture-in-picture behavior. HeroUI controls around the player add:

- a MediaSource selector with human-readable resolution, container, codec, bitrate, and
  audio summary;
- a subtitle selector with Off, Chinese, and English choices;
- current source and subtitle status;
- a recoverable media-error Alert with a command to return to the recommended source;
- a real Exit command that records stop state, revokes the ticket, and navigates to detail.

The player reports Started when playback begins, Progress at a bounded interval while
playing and on pause/seek, and Stopped on Exit, route cleanup, or media end. Position is
converted from seconds to Jellyfin ticks without accepting negative, non-finite, or
out-of-runtime values.

On `ended`, the player sends the final stopped position and marks the item played through
the existing user-data command. It does not revoke the active ticket merely because the
media ended, so native replay remains functional. Ticket cleanup happens on source change,
Exit, and unmount.

## Poster Status

`MediaTile` owns one reusable poster-status component. It reads only the item's returned
`UserData` and runtime:

- `Played=true` renders a green circular check;
- otherwise, a positive playback position and positive runtime render a green progress
  ring with a ratio clamped between 1% and 99%;
- `IsFavorite=true` renders a pink filled heart;
- unwatched and non-favorite items render no overlay.

When both viewing and favorite states exist, the two badges sit in one compact top-right
group without covering the title artwork more than necessary. Tooltips and accessible
labels explain each icon. The visual does not show invented percentages when runtime is
unknown.

## Failure Handling

- A TMDB detail failure aborts publication before catalog mutation and identifies only the
  provider kind and numeric ID.
- Optional artwork failures remain sanitized warnings.
- Missing `ffmpeg`, invalid generated output, a non-absolute fixture directory, or a
  non-development database target fails before media publication.
- Source switching ignores stale asynchronous ticket responses and always revokes any
  superseded ticket.
- Subtitle fetch failures leave playback active, reset the selector to Off, and show a
  bounded message.
- Media decode and zero-byte failures show a recoverable Alert; they do not silently fall
  through to another source.
- Playstate telemetry failures do not stop local playback, but are surfaced as a
  non-blocking status and retried only by the next real media event.

## Verification

- Metadata tests pin manifest count, uniqueness, response bounds, and sanitized failures.
- Repository tests cover source fixture idempotency, active publications, source ordering,
  subtitle ownership, and no path or credential leakage.
- API goldens cover expanded source and subtitle fields.
- Vitest covers source changes, ticket cleanup, subtitle Blob lifecycle, playstate events,
  media error recovery, played completion, and poster badges.
- The development import is followed by SQL assertions that both libraries contain at
  least 100 top-level items and that every Movie and Episode has an active source.
- Browser validation covers desktop, tablet, and mobile layouts; valid playback; source and
  subtitle switching; damaged-source recovery; poster progress; completion; favorite
  status; resource errors; and console errors.

## Delivery Order

1. Freeze the expanded TMDB manifest and rename the existing catalog publication.
2. Publish development filesystem media fixtures through normal source tables.
3. Extend PlaybackInfo with bounded source and subtitle display metadata.
4. Rebuild the player around explicit source, subtitle, ticket, and playstate lifecycle.
5. Add poster viewing and favorite overlays.
6. Import only the current development database and run automated plus browser validation.
