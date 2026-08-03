# AI Usage Analytics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add administrator-visible AI usage, token, user-ranking, model-ranking, trend, and failure analytics to the existing AI settings page.

**Architecture:** Persist one bounded execution record for every authenticated `/Ai/Chat` attempt, storing identifiers, timestamps, elapsed milliseconds, safe outcome category, and provider-reported token usage without prompt or response bodies. Aggregate those records in SQL through a focused repository, expose a no-store administrator endpoint, and render the result with existing HeroUI v3 cards and compound tables.

**Tech Stack:** Rust, Axum, SeaORM/SeaQuery, PostgreSQL/SQLite-compatible migrations, React, TypeScript, HeroUI v3, Vitest.

## Global Constraints

- Token totals use upstream `usage.prompt_tokens`, `usage.completion_tokens`, and `usage.total_tokens`; missing usage remains unknown and is never estimated.
- “Today” is computed using the configured TJXY system time zone and returned with an explicit UTC window.
- Store no API keys, provider error bodies, prompt text, assistant text, tool arguments, or chain-of-thought in analytics records.
- Failure categories are bounded safe values: `upstream_rejected`, `upstream_invalid`, `upstream_timeout`, `tool_failed`, `persistence_failed`, and `internal_error`.
- Follow HeroUI v3 compound component APIs and the repository’s existing admin page patterns.
- Do not commit changes unless the user explicitly requests a commit.

---

### Task 1: AI execution records and aggregate repository

**Files:**
- Create: `crates/db/src/migration/m20260803_000050_ai_usage_analytics.rs`
- Modify: `crates/db/src/migration/mod.rs`
- Modify: `crates/db/src/ai.rs`
- Test: `crates/db/tests/ai_repository_contract.rs`
- Test: `crates/db/tests/schema_contract.rs`

**Interfaces:**
- Produces: `AiExecutionInput`, `AiExecutionOutcome`, `AiUsageAnalytics`, and `AiUsageRepository::{record, analytics}`.
- `analytics(window_start, window_end, trend_start, trend_end, trend_offset)` returns summary totals, daily trend rows, user ranking rows, model ranking rows, and recent failures.

- [ ] Write a failing repository contract that records successful and failed runs, verifies unknown usage stays unknown, and checks literal summary/ranking/trend totals.
- [ ] Run `cargo test -p tjxy-db --test ai_repository_contract ai_usage -- --nocapture` and confirm failure because the analytics interfaces do not exist.
- [ ] Add `ai_execution_records` with UUID primary key, user/model foreign keys, started/completed timestamps, elapsed milliseconds, outcome, nullable token columns, and supporting time/user/model indexes.
- [ ] Implement bounded input validation and SQL aggregation without loading full conversations or messages.
- [ ] Run the focused repository and schema tests until green.

### Task 2: Capture provider usage and execution outcomes

**Files:**
- Modify: `crates/server/src/ai.rs`
- Test: `crates/server/tests/ai_routes.rs`

**Interfaces:**
- Consumes: `AiUsageRepository::record` and provider completion `usage`.
- Produces: one execution record for every accepted chat request, including pre-persistence failures and persistence failures.

- [ ] Add failing route tests for a successful response with exact token usage, an upstream rejection, and a persistence-safe failure classification.
- [ ] Run `cargo test -p tjxy-server --test ai_routes ai_usage -- --nocapture` and confirm failure.
- [ ] Extend provider response parsing with optional strict non-negative token usage.
- [ ] Carry accumulated usage across tool rounds and write one execution record after success or failure without changing the browser SSE contract.
- [ ] Classify reqwest timeout separately and collapse all unrecognized internal failures to `internal_error`.
- [ ] Run the focused route tests until green.

### Task 3: Administrator analytics API

**Files:**
- Modify: `crates/server/src/ai_settings.rs`
- Modify: `crates/server/src/lib.rs`
- Test: `crates/server/tests/ai_settings_routes.rs`

**Interfaces:**
- Produces: `GET /Admin/Ai/Analytics` returning `Window`, `Summary`, `Daily`, `Users`, `Models`, and `RecentFailures` with PascalCase fields.

- [ ] Add a failing administrator route test covering authorization, `Cache-Control: no-store`, exact response shape, and absence of message/provider-secret fields.
- [ ] Run `cargo test -p tjxy-server --test ai_settings_routes ai_analytics -- --nocapture` and confirm failure.
- [ ] Implement the route using the existing authenticated-administrator guard and system time-zone service.
- [ ] Return bounded rows: 14 daily points, top 10 users, top 10 models, and 20 recent failures.
- [ ] Run the focused settings route tests until green.

### Task 4: Strict frontend analytics client

**Files:**
- Modify: `admin/src/settings/aiSettingsApi.ts`
- Test: `admin/src/settings/aiSettingsApi.test.ts`

**Interfaces:**
- Produces: `AiAnalytics` and `getAiAnalytics(signal?)`.

- [ ] Add failing tests for the exact successful payload and malformed/extra-key rejection.
- [ ] Run `npm test -- --run src/settings/aiSettingsApi.test.ts` from `admin/` and confirm failure.
- [ ] Add strict response mapping with safe integer, ISO date, nullable token, bounded-array, and enumerated outcome validation.
- [ ] Run the focused API tests until green.

### Task 5: HeroUI analytics dashboard

**Files:**
- Create: `admin/src/settings/AiAnalyticsPanel.tsx`
- Create: `admin/src/settings/AiAnalyticsPanel.test.tsx`
- Modify: `admin/src/settings/AiSettingsPage.tsx`
- Modify: `admin/src/settings/AiSettingsPage.test.tsx`

**Interfaces:**
- Consumes: `AiAnalytics` and `getAiAnalytics`.
- Produces: summary cards, 14-day accessible bar trend, user ranking, model ranking, and recent failures table.

- [ ] Add failing component tests for summary values, unknown-token rendering, user/model ordering, failure categories, empty state, retry, and refresh.
- [ ] Run `npm test -- --run src/settings/AiAnalyticsPanel.test.tsx src/settings/AiSettingsPage.test.tsx` and confirm failure.
- [ ] Implement the panel with HeroUI v3 `Card`, `Table.ScrollContainer`, `Table.Content`, `Table.Header`, `Table.Body`, `Table.Row`, `Table.Cell`, `Chip`, `ProgressBar`, `Skeleton`, and `Alert` patterns already used by the app.
- [ ] Keep the provider form independently usable when analytics loading fails.
- [ ] Add analytics refresh to the page reload action without coupling draft form state to statistics state.
- [ ] Run focused component tests and `npm run typecheck` until green.

### Task 6: Verification, documentation, and quality review

**Files:**
- Modify: `docs/api-parity.md`

- [ ] Run `cargo test -p tjxy-db --test ai_repository_contract --test schema_contract`.
- [ ] Run `cargo test -p tjxy-server --test ai_routes --test ai_settings_routes`.
- [ ] Run `npm test -- --run src/settings/aiSettingsApi.test.ts src/settings/AiAnalyticsPanel.test.tsx src/settings/AiSettingsPage.test.tsx` from `admin/`.
- [ ] Run `npm run typecheck`, `npm run lint`, and `npm run build` from `admin/`.
- [ ] Apply and roll back the new migration against the repository’s migration test path.
- [ ] Update `docs/api-parity.md` with the administrator endpoint, usage semantics, unknown-token behavior, and data-minimization guarantees.
- [ ] Review the final diff for secret leakage, unbounded queries, time-zone boundary errors, unsafe provider error storage, and overlapping user changes.
