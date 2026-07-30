# TMDB Rich Demo Catalog Design

## Status

Reviewed design awaiting final implementation approval. This work expands TJXY's catalog
metadata and ordinary-user detail experience, then imports one realistic demonstration
catalog into the current development database.

## Context

TJXY already separates logical `CatalogItem` identity from playable `MediaSource` and
supports Movie, Series, Season, and Episode hierarchy nodes. The current query projection,
Jellyfin-compatible DTO, and HeroUI client expose only a title, year, overview, type, and
primary image. The existing TMDB provider searches Movie and Series titles but does not
retrieve rich details, credits, classifications, seasons, or episodes.

The administrator can store and test an encrypted TMDB API access token and language in
the development database. The requested demonstration catalog must use real TMDB metadata
and imagery while deliberately containing no video files and no playable sources.

## Goals

- Represent display-relevant Movie, Series, Season, and Episode metadata as authoritative
  catalog fields and associations.
- Expose complete, bounded item details and correctly ordered Season and Episode children
  through the existing authenticated catalog surface.
- Rebuild the HeroUI item experience to show rich movie and television information on
  desktop and mobile.
- Import 12 real movies and 6 short or limited series, including every Season and Episode
  returned for those selected series, into the current development database.
- Store real TMDB artwork in TJXY's content-addressed asset system.
- Keep every imported item browsable but unplayable by omitting `MediaSource` and
  `MediaLocation` records.

## Non-Goals

- A production startup seed, administrator demo-data controls, scheduled synchronization,
  or a general-purpose TMDB catalog browser.
- Automatic refresh of the imported titles after the one development import.
- Transcoding, placeholder video files, fake stream URLs, or fabricated playback success.
- Querying raw TMDB JSON directly from the frontend or public catalog API.
- Importing watch providers, recommendations, reviews, social data, or videos.
- Replacing the existing normal metadata-resolution workflow for scanned media.

## Decision

Extend the existing normalized catalog model and add a one-off, idempotent development
import command. The imported data follows the same repositories, query projections, API,
asset storage, and HeroUI routes as ordinary scanned media. A provider snapshot preserves
the source response for traceability, but query-facing behavior depends only on canonical
catalog fields and associations.

### Alternatives Considered

1. Normalized catalog fields plus an idempotent development importer are selected. This
   proves the real catalog and frontend paths and leaves useful product capability after
   the sample data is gone.
2. Storing the complete TMDB response in one JSON column was rejected. It couples queries
   and frontend DTOs to a remote provider, weakens validation, and makes ordering and
   compatibility behavior difficult to test.
3. Separate demo tables and demo-only APIs were rejected. They would duplicate catalog
   behavior and create an attractive preview that does not exercise TJXY's actual model.

## Catalog Model

`catalog_items` gains nullable canonical values appropriate to its item type:

- tagline;
- community rating and vote count;
- runtime in Jellyfin ticks;
- premiere or first-air date and optional end date;
- release status and official content rating;
- original language;
- index number for a Season or Episode.

Existing `production_year`, `original_title`, and `overview` remain canonical. A Movie has
no index number. A Season's index number is its TMDB season number. An Episode's index
number is its episode number and its parent Season supplies the season number. Season 0 is
retained for specials and sorts before Season 1.

Genres and studios continue to use their existing normalized associations. Production
countries and spoken languages use normalized value tables and item associations, matching
the established genre and studio pattern. Original language remains a scalar because it
has one semantic value.

Credits remain ordered associations between a CatalogItem and a reusable Person. The
person model gains stable TMDB identity. A Credit records its contribution type, display
role or job, and provider order. Cast and crew are not flattened into strings. Person
profile images use the same content-addressed `AssetBlob` ownership and publication
semantics as item images. Movie and Series records retain the first 24 provider-ordered
cast credits and 12 primary crew credits. Episodes retain the first 12 cast or guest-star
credits and their director and writer credits. Complete upstream credits remain in the
MetadataSnapshot without making the query projection unbounded.

A `MetadataSnapshot` associates a provider, provider entity kind, provider entity ID,
request language, fetch time, and validated provider response with a CatalogItem or
Person. Snapshots preserve unmapped source evidence but are not read by catalog queries.
No access token, request authorization header, or upstream error body is stored.

## Hierarchy And Membership

Each selected show is one Series `CatalogItem`. Every returned TMDB Season is a child of
that Series, and every returned Episode is a child of its Season:

```text
Series
  Season 0
    Episode 1
  Season 1
    Episode 1
    Episode 2
```

The Movie demonstration library contains the 12 Movie nodes. The Television demonstration
library contains each Series and all of its Season and Episode descendants through explicit
`LibraryMembership`. Descendants are not made visible by an implicit parent-path rule.

Imported items have complete metadata state and a non-error structure state, but no source
publication, `MediaSource`, or `MediaLocation`. The absence of a source is a supported
catalog state, not a fake storage failure.

## Dataset

The import uses a reviewed, source-controlled list of stable TMDB IDs rather than a
time-varying popularity or discovery query. The movie selection spans animation, drama,
science fiction, crime, action, mystery, and non-English cinema. The television selection
uses six completed short or limited series so importing every Season and Episode remains
bounded.

The committed Movie manifest contains:

- Arrival (`329865`);
- Parasite (`496243`);
- Spirited Away (`129`);
- The Godfather (`238`);
- Mad Max: Fury Road (`76341`);
- Spider-Man: Into the Spider-Verse (`324857`);
- In the Mood for Love (`843`);
- Crouching Tiger, Hidden Dragon (`146`);
- Knives Out (`546554`);
- Everything Everywhere All at Once (`545611`);
- Dune: Part Two (`693134`);
- The Dark Knight (`155`).

The committed Series manifest contains:

- Chernobyl (`87108`);
- The Queen's Gambit (`87739`);
- Mare of Easttown (`115004`);
- Unbelievable (`91275`);
- When They See Us (`81355`);
- Ripley (`94028`).

Contract tests assert the count and uniqueness but do not freeze localized titles, ratings,
or artwork paths that TMDB may legitimately update. The importer records the selected
language and fetch timestamp in each snapshot.

## TMDB Retrieval

The development command reads the same encrypted TMDB setting and credential-keyring
configuration used by the server. It fails before remote requests when no usable database
override exists; it does not silently fall back to a hard-coded or command-line secret.

For each Movie, retrieval includes details, release classifications, credits, external
IDs, and images. For each Series, retrieval includes details, content ratings, aggregate
credits, external IDs, and images. Every Season is then fetched for its details, credits,
images, and Episodes. Episode records include available still images and episode credits.
Requests use the configured language, with one bounded English fallback for an otherwise
empty localized title or overview.

TMDB wire DTOs remain private to the metadata boundary. They validate required IDs, names,
indices, dates, nonnegative durations, bounded rating values, association counts, image
paths, and response sizes before producing provider-neutral import records.

## Asset Flow

The importer downloads primary posters, backdrops, Season posters, Episode stills, and
bounded Person profile images through the existing HTTPS-only, bounded metadata image
fetcher. Images are validated and written through `AssetWriteService`, deduplicated by
content hash, and then associated at the correct image type and priority.

A primary poster is required for each selected Movie and Series. Missing optional
backdrops, Season posters, Episode stills, or Person images produce sanitized warnings and
omit only that asset. A failed or invalid required poster aborts publication.

Remote TMDB image URLs are never stored in browser-facing DTOs, and browser image requests
continue using authenticated TJXY image routes without session credentials in URLs.

## Import Transaction And Idempotency

Catalog, Person, association, membership, provider ID, provenance, and snapshot identifiers
are deterministically derived from provider kind and TMDB ID. Re-running the command updates
the same records and never creates duplicate libraries, CatalogItems, Seasons, Episodes, or
People.

Network retrieval, validation, and asset staging occur before the catalog transaction. The
transaction then:

1. upserts the two demonstration libraries;
2. upserts all CatalogItems and their parent relationships;
3. replaces the selected items' normalized metadata associations;
4. upserts explicit membership for every root and descendant;
5. publishes staged item and person assets;
6. records provider IDs, provenance, and snapshots;
7. increments `CatalogGeneration` once.

Any catalog write failure rolls back the transaction and leaves no partially published
hierarchy. Already content-addressed staged blobs may remain unreferenced after rollback;
that is safe and can be handled by the existing asset lifecycle rather than deleting files
during error recovery.

## API Contract

List and search projections remain bounded. They return the fields required by media tiles:
ID, parent, name, item type, year, community rating, index number, primary image tag, and
UserData. Search results receive the same image and basic metadata and do not issue one
detail request per result.

`GET /Items/{id}` returns the authoritative rich detail projection:

- title, original title, overview, tagline, type, hierarchy indices, and dates;
- rating, vote count, runtime, status, and official rating;
- genres, studios, countries, spoken languages, and original language;
- bounded ordered cast and primary crew credits;
- available image tags, source availability, and UserData.

`GET /Items?ParentId={id}` remains the child-navigation contract. Series children return in
Season index order; Season children return in Episode index order. Pagination still applies
and deterministic ID tie-breaking handles duplicate or absent indices.

Raw MetadataSnapshots are not serialized by ordinary-user or Jellyfin-compatible routes.

## HeroUI Experience

Movie and Series details use real backdrop imagery as a restrained content header with a
readable overlay, followed by an unframed responsive information layout. A poster anchors
the identity while compact facts expose rating, runtime, date, content rating, country,
language, genres, and studios. Overview, crew, and cast are separate scannable sections.
Repeated cast entries may use small HeroUI surfaces; page sections are not nested cards.

Series details add a Season selector and ordered Episode list. Each Episode row contains a
stable still-image area, episode code, title, air date, runtime, overview, and an action to
open its detail. Season and Episode detail routes reuse the same metadata language and
preserve parent breadcrumbs.

Movie and Episode details expose a Play command. The player calls the normal PlaybackInfo
route. When no sources exist, it renders an explicit non-danger state stating that no
playable file is attached, with a route back to details. It never issues a playback ticket
or creates a fake media element.

Desktop and mobile retain the approved HeroUI v3 semantic theme, focus behavior, responsive
navigation, and reduced-motion behavior. Media artwork is the visual subject; the pages do
not introduce marketing heroes, decorative gradients, or nested dashboard cards.

## Failure Handling

- Missing or undecryptable TMDB configuration fails before catalog mutation.
- HTTP 429 honors a bounded `Retry-After`; transient 5xx and network failures use bounded
  backoff. Permanent 4xx responses do not retry.
- Required item detail, hierarchy identity, or primary root artwork failure aborts the
  import. Optional asset failure is a warning.
- Wire validation errors identify provider entity kind and ID without logging response
  bodies or credentials.
- Database errors roll back the complete catalog publication.
- A missing media source produces an expected unavailable response and frontend state,
  distinct from unsupported-container and authorization failures.
- All warnings and errors use fixed sanitized text at HTTP and UI boundaries.

## Verification

Database tests cover forward and backward migration portability, constraints, normalized
associations, parent hierarchy, and deterministic ordering. Metadata tests use local wire
fixtures for Movie, Series, Season, Episode, credits, classifications, images, localization
fallback, response bounds, and credential redaction.

Importer tests prove deterministic IDs, manifest uniqueness, full descendant membership,
idempotent re-run, association replacement, rollback, optional-asset warnings, required
poster failure, and one `CatalogGeneration` increment. API tests cover authorization,
rich detail shape, bounded credits, list/search projections, image tags, source
availability, and Season/Episode order.

Vitest covers rich detail rendering, omitted optional metadata, Season switching, Episode
rows, search imagery, and no-source playback behavior. Playwright covers authenticated
desktop and mobile journeys through home, both libraries, search, Movie detail, Series
detail, Season/Episode navigation, and playback-unavailable state, including overflow,
accessibility, failed requests, console errors, and deterministic screenshots.

After automated verification, the one-off command runs against the current development
database with the configured TMDB token. The browser walkthrough verifies the actual
localized titles, metadata, images, hierarchy, and unavailable playback state.

## Delivery Order

Because migration, domain shape, repository projection, API, importer, and frontend all
share contracts, implementation remains serial at those boundaries:

1. schema and domain values;
2. rich metadata transport and provider-neutral records;
3. repository publication and detail queries;
4. API DTO and route projections;
5. HeroUI detail, hierarchy, and unavailable playback states;
6. one-off import and real-database execution;
7. automated and browser verification.
