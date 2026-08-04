# Similar-title recommendations implementation plan

## Scope

Implement the approved design in
`docs/superpowers/specs/2026-08-04-similar-title-recommendations-design.md`.
Preserve the current dirty worktree and integrate serially where files overlap
existing user changes.

## TDD seams

1. Repository contract: visible same-type candidate selection, integer scoring,
   watched exclusion, stable ordering, and empty results.
2. Application service contract: authenticated-user assertion and repository
   result propagation.
3. HTTP route contract: `/Items/{item_id}/Similar`, bounded `limit`, standard
   item DTO response, and error mapping.
4. Client API and page contract: request shape, Movie/Series-only loading,
   result/empty/error states, defensive filtering, and recommendation placement.

## Steps

- [x] Add reverse association indexes in migration `000054` and extend schema
      contract coverage.
- [x] Add a repository contract tracer test for ranked Movie recommendations,
      then implement the bounded shortlist and pure Rust scorer.
- [x] Add repository cases for weak-only matches, watched candidates, wrong
      types, stable ties, sparse results, visibility, and Series completion.
- [x] Add the `CatalogQueryService::similar_items` seam with authorization and
      service contract tests.
- [x] Add the authenticated Similar route, query parsing, DTO conversion, and
      route contract tests.
- [x] Add `getSimilarItems` to the client API with a request contract test.
- [x] Add the recommendation rail to `ItemPage` through page tests for loading,
      results, empty, unavailable, and unsupported item types.
- [x] Run focused tests after every slice, then format, lint, typecheck, full
      relevant Rust/React suites, production build, and desktop/mobile visual QA.
- [x] Review the final diff for N+1 queries, full-table scans, authorization or
      visibility leaks, unstable ordering, swallowed errors, and unrelated
      worktree changes.
