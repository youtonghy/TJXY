# Similar-title recommendations design

## Goal

Add a trustworthy "Recommended for you" section below the cast and crew on
Movie and Series detail pages. Recommendations come only from titles that are
currently visible in the authenticated user's TJXY catalog. They are derived
from stored catalog metadata and user state; the server never pads the result
with random or external titles.

The first version deliberately provides structured similarity rather than
semantic story embeddings. Theme and story proximity are approximated through
genre and creator/cast overlap. A future keyword or embedding system can extend
the scoring boundary without changing the route or page component.

## Product behavior

- A Movie recommends only Movies. A Series recommends only Series.
- Season, Episode, Audio, Folder, and CollectionFolder pages never request or
  display recommendations.
- The source title, invisible titles, and titles the current user has finished
  are excluded.
- The page requests at most four results. The server may return fewer, including
  zero, and the client must not fill missing slots.
- A candidate must share at least one genre or credited person with the source.
  Language, studio, country, year, rating, or favorite state cannot establish
  relevance by themselves.
- An eligible detail page always has a recommendation section. An empty result
  displays "No recommendations yet". A failed request displays
  "Recommendations are temporarily unavailable" without failing the rest of
  the detail page.

For Series, "finished" means either the Series itself is explicitly marked
played or all known visible Episodes are played. A Series with no expanded
Episodes is not inferred to be finished.

## API contract

Add the authenticated endpoint:

```text
GET /Items/{item_id}/Similar?limit=4
```

The endpoint returns the existing `BaseItemDtoQueryResult` shape so the client
can reuse `MediaItem`, `MediaImage`, and `MediaTile` without introducing a
parallel media DTO.

- `limit` defaults to 4 and is accepted in the range `1..=20`.
- A missing or invisible source returns `404`.
- An unsupported but visible source type returns `200` with an empty `Items`
  collection.
- No qualifying candidates returns `200` with an empty `Items` collection.
- Authentication and optional `userId` assertions follow the existing item
  routes and never permit selecting another user's recommendation state.

The route delegates through `CatalogQueryService` to
`CatalogQueryRepository::similar_items`. This preserves the existing
authorization, catalog visibility, catalog-generation, and user-revision
boundaries.

## Candidate retrieval

The repository uses two bounded phases:

1. Build a shortlist from candidates that share a genre or person with the
   source. Apply source visibility, candidate visibility, same-type, source-ID,
   and played-state filters in SQL. Before applying the 256-candidate bound,
   order by a lightweight proxy of shared genres times 30 plus shared people
   times 24, then shared genres and item ID. This keeps strong matches in the
   exact-scoring phase instead of allowing UUID order to choose the shortlist.
2. Batch-load the shortlist's genres, people, languages, studios, countries,
   year, rating, and current-user state. Compute the final integer score in a
   pure Rust function, filter by the relevance threshold, sort deterministically,
   and attach images to only the selected result records.

The implementation must not call `item_detail` once per candidate and must not
parse provider snapshot JSON during a request.

Reverse lookup indexes are required because the current association indexes are
catalog-item-first:

- `item_genres(genre_id, catalog_item_id)`
- `item_people(person_id, catalog_item_id)`
- `item_languages(language_id, catalog_item_id)`
- `item_studios(studio_id, catalog_item_id)`
- `item_countries(country_id, catalog_item_id)`

These indexes are introduced in one migration and asserted by the schema
contract tests. The query remains portable across SQLite, PostgreSQL, and MySQL.

## Scoring

Use integer points so ordering is stable across database backends:

| Signal | Points |
| --- | ---: |
| Shared genre | 30 each, capped at 60 |
| Shared credited person | 24 each, capped at 48 |
| Shared director, creator, writer, or screenplay credit | 8 additional each, within the people cap |
| Same original language | 10 |
| Shared spoken language | 5 each, capped at 10 |
| Shared studio | 8 each, capped at 16 |
| Same country | 6 |
| Production years within 3 years | 8 |
| Production years within 10 years | 4 |
| Candidate is favorite and not finished | 5 |

Only candidates with a score of at least 24 remain. Because the shortlist
already requires a shared genre or person, weak metadata and popularity cannot
promote an unrelated title.

Creator bonuses use distinct matching `(person, credit kind)` pairs and exact
case-insensitive credit-kind matching. Actor/cast credits never receive a
creator bonus from character-role text. Legacy rows without a specific credit
type may fall back to an exact role match.

Sort by:

1. total score descending;
2. number of shared genres descending;
3. community rating descending, with missing ratings last;
4. production year descending, with missing years last;
5. catalog item ID ascending.

This order is deterministic and makes exact recommendation expectations suitable
for contract tests.

## Client design

`catalogApi.ts` gains `getSimilarItems(itemId, limit = 4)`. `ItemPage` starts the
request only after a Movie or Series detail has loaded. Recommendation loading,
empty, and failure state are isolated from the main item request.

The recommendation section appears after cast and crew and reuses `MediaTile`.
It does not use the existing `MediaRow`, whose fixed responsive grid hides empty
states and does not scroll horizontally.

The rail uses semantic section/list markup and native horizontal scrolling with
scroll snap:

- four cards fit across the desktop content width;
- cards retain a stable width and scroll horizontally on narrower viewports;
- loading renders four stable poster skeletons;
- the list has an accessible label and each existing tile remains a normal
  detail-page link;
- the client defensively removes the source, played items, unsupported types,
  and wrong-type results even though the server enforces those rules.

No carousel dependency or custom drag engine is introduced for a four-item rail.

## Errors and caching

Recommendation errors are explicit and local. The server maps invalid limits to
`400`, missing sources to `404`, authorization failures to the existing `403`
behavior, and repository failures to the existing service-unavailable response.
The client distinguishes an empty successful response from an unavailable
response.

If service caching is added, its key must contain user ID, source ID, requested
limit, catalog generation, and user revision. This prevents recommendations from
surviving metadata updates or played/favorite changes. Caching is optional for
the first implementation because the shortlist is indexed and bounded.

## Verification

Repository contract tests cover:

- shared-genre and shared-person scoring;
- creator weighting and deterministic tie breaking;
- language-only candidates being rejected;
- different-type, current-title, Season, Episode, invisible, and played
  candidates being excluded;
- favorite-but-unplayed boosting;
- fewer than four and zero-result responses;
- Series completion based on known Episodes;
- separate users receiving results from their own state.

Service and route tests cover authentication, user assertions, invalid limits,
missing and invisible sources, unsupported types, the standard DTO response, and
image metadata.

Client tests cover loading, four results and links, empty and unavailable states,
Movie/Series-only requests, defensive filtering, and placement after cast and
crew. Desktop and mobile browser checks verify four-column framing, horizontal
scrolling, no page overflow, keyboard focus, and non-overlapping text.

## Non-goals

- External recommendations from TMDb or another catalog.
- Random fallback titles.
- LLM-generated recommendations.
- Request-time parsing of provider snapshots.
- Keyword extraction, vector indexing, or semantic embeddings.
- A recommendation settings screen or administrator-tunable weights.
