# AI Media Assistant Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a production-shaped, user-scoped AI media assistant with encrypted OpenAI-compatible provider configuration, administrator-controlled model visibility, grounded catalog and viewing-context tools, persisted conversations, and a responsive HeroUI Pro chat interface.

**Architecture:** The browser talks only to authenticated TJXY endpoints. A new server-side AI service loads encrypted provider settings, maps an in-process MCP-style media tool registry to OpenAI Chat Completions function tools, runs a bounded tool loop, persists messages, and emits typed SSE events. Existing catalog authorization remains authoritative for all media access, and user identity always comes from the authenticated principal.

**Tech Stack:** Rust 1.88 / edition 2024, Axum 0.8, Tokio, reqwest 0.12, SeaORM 1.1, React 19, TypeScript 6, Vite 8, HeroUI React 3.2.2, HeroUI Pro React 1.0.0-beta.7, Vitest, Testing Library, Playwright.

## Global Constraints

- Preserve all pre-existing dirty-worktree changes and integrate against the current files.
- Use OpenAI-compatible `POST {base_url}/chat/completions` with Chat Completions messages and function `tools`.
- Never expose provider credentials, upstream base URL, unrestricted user IDs, raw metadata snapshots, hidden model IDs, or chain-of-thought to the browser.
- Encrypt API keys with the existing `CredentialCipher`; admin responses return configuration status only.
- Restrict the assistant to film, television, music, media-library discovery, and the authenticated user's viewing context.
- Use the authenticated principal as the only source of user identity for conversations and tools.
- Keep the MCP-style tool registry read-only and bounded; validate every model-generated argument before execution.
- Use HeroUI v3 compound APIs and HeroUI Pro AI components from their package subpaths.
- Add behavior tests before production code and observe the intended failure before implementation.
- Do not commit, stage, push, or overwrite unrelated user changes.

---

### Task 1: Persist AI Configuration, Models, Conversations, and Messages

**Files:**
- Create: `crates/db/src/migration/m20260802_000047_ai_assistant.rs`
- Create: `crates/db/src/ai.rs`
- Create: `crates/db/tests/ai_repository_contract.rs`
- Modify: `crates/db/src/migration/mod.rs`
- Modify: `crates/db/src/lib.rs`

**Interfaces:**
- Produces `AiProviderSettingsRepository::{get, put}`, `AiModelRepository::{list, replace}`, and `AiConversationRepository::{list, create, get, append_exchange, delete}`.
- Provider settings store `base_url`, encrypted `CredentialEnvelope`, prompt, enabled state, and optimistic `revision`.
- Public model records expose stable UUID, display name, upstream model ID only inside the server, visibility, default state, and sort order.

- [ ] **Step 1: Write failing migration and repository contract tests**

  Cover reversible migration, singleton provider settings, optimistic revision conflicts, exactly one visible default model, user-isolated conversations, ordered messages, and foreign-user deletion returning false.

- [ ] **Step 2: Run the new database test and confirm RED**

  Run: `cargo test -p tjxy-db --test ai_repository_contract`

  Expected: compilation fails because the AI repository and migration do not exist.

- [ ] **Step 3: Add the migration**

  Create cross-database tables `ai_provider_settings`, `ai_models`, `ai_conversations`, and `ai_messages`. Use UUID columns through existing migration helpers, restrictive foreign keys, unique provider/model keys, user and conversation indexes, bounded text validation in the repository, and a reversible `down` order.

- [ ] **Step 4: Implement repositories and exported records**

  Use `CredentialEnvelope` for secret fields, transactions for model replacement and assistant/user message pairs, and typed repository errors for validation, conflict, and storage failures.

- [ ] **Step 5: Run database tests and refactor while green**

  Run: `cargo test -p tjxy-db --test ai_repository_contract && cargo test -p tjxy-db --test schema_contract`

---

### Task 2: Add Administrator AI Settings and Public Model APIs

**Files:**
- Create: `crates/server/src/ai_settings.rs`
- Create: `crates/server/tests/ai_settings_routes.rs`
- Modify: `crates/server/src/lib.rs`
- Modify: `crates/server/src/startup.rs`

**Interfaces:**
- Produces administrator routes `GET/PUT/DELETE /Admin/Ai/Settings`, `POST /Admin/Ai/Settings/Test`, and authenticated `GET /Ai/Models`.
- `PUT /Admin/Ai/Settings` consumes PascalCase JSON with `Enabled`, `BaseUrl`, optional `ApiKey`, `SystemPrompt`, `Revision`, and `Models`.
- `GET /Ai/Models` returns only `{Id, DisplayName, IsDefault}` for visible enabled models.

- [ ] **Step 1: Write failing route tests**

  Assert administrator authorization, `Cache-Control: no-store`, no plaintext secret in GET responses, invalid URL rejection, revision conflict, hidden-model filtering, and exactly one public default.

- [ ] **Step 2: Run the route test and confirm RED**

  Run: `cargo test -p tjxy-server --test ai_settings_routes`

- [ ] **Step 3: Implement configuration validation and encryption**

  Accept only absolute `http` or `https` URLs without credentials, fragments, or query strings; trim a trailing slash; cap prompt, model, and credential lengths; use the startup-injected `CredentialCipher`; never log request bodies.

- [ ] **Step 4: Implement connection testing**

  Send a bounded OpenAI-compatible chat completion request using either the submitted key or the stored encrypted key. Return `{Status:"Success"}` only after a valid response; translate timeout/upstream/protocol errors without returning secrets.

- [ ] **Step 5: Wire service construction and routes**

  Add `ai_settings` to `AppState`, initialize it beside metadata settings, and register routes in the central router.

- [ ] **Step 6: Run focused and server library tests**

  Run: `cargo test -p tjxy-server --test ai_settings_routes && cargo test -p tjxy-server --lib`

---

### Task 3: Implement Grounded MCP-Style Media Tools and Agent Chat

**Files:**
- Create: `crates/server/src/ai.rs`
- Create: `crates/server/tests/ai_routes.rs`
- Modify: `crates/server/src/lib.rs`
- Modify: `crates/server/src/startup.rs`

**Interfaces:**
- Produces `GET/POST/DELETE /Ai/Conversations`, `GET/DELETE /Ai/Conversations/{id}`, and `POST /Ai/Chat`.
- `POST /Ai/Chat` consumes `{ConversationId, ModelId, Message}` for an existing conversation or `{NewConversationId, ModelId, Message}` for a new one, and emits SSE events named `conversation`, `tool`, `delta`, `sources`, `done`, or `error`. The client-generated new id lets the UI reconcile a committed first exchange after a stopped or interrupted stream; unresolved IDs remain in tab-scoped session storage for reload, reconnect, and focus reconciliation.
- MCP-style tools are `search_catalog`, `get_media_detail`, `get_recent_watch_history`, `get_user_insights`, `get_favorites`, `get_resume_items`, and `recommend_candidates`.

- [ ] **Step 1: Write failing unit and route tests**

  Cover message/model validation, hidden model rejection, user conversation isolation, media-only prompt injection, tool argument rejection, bounded tool rounds, grounded source collection, persisted exchanges, and the SSE event contract.

- [ ] **Step 2: Run the AI route test and confirm RED**

  Run: `cargo test -p tjxy-server --test ai_routes`

- [ ] **Step 3: Implement the MCP-style tool registry**

  Define tool names, JSON schemas, a single `list_tools` source of truth, and `call_tool(principal, name, arguments)`. Reuse `CatalogQueryService` for visible catalog reads. Move or share user insight queries instead of making authenticated loopback HTTP requests. Cap every result list and strip raw snapshots.

- [ ] **Step 4: Implement the OpenAI-compatible provider client**

  Deserialize assistant text and function `tool_calls`; validate response shape, tool JSON, upstream status, size, and timeout. Keep the provider behind an injectable trait so route tests use a deterministic fake.

- [ ] **Step 5: Implement the bounded Agent loop**

  Prepend the administrator prompt plus a non-overridable server policy, include recent persisted messages, allow at most six tool rounds, append tool results, collect catalog source IDs, and make a final assistant response. Reject empty or overlong responses.

- [ ] **Step 6: Implement conversation routes and SSE output**

  Authenticate before opening the stream, emit non-sensitive tool labels, split final text into deterministic deltas, persist the exchange atomically, add keep-alives, and emit a typed terminal error event for failures after stream start.

- [ ] **Step 7: Run focused tests and server checks**

  Run: `cargo test -p tjxy-server --test ai_routes && cargo test -p tjxy-server --lib`

---

### Task 4: Build Strict Frontend API Clients and Administrator Settings UI

**Files:**
- Create: `admin/src/client/ai/aiTypes.ts`
- Create: `admin/src/client/ai/aiApi.ts`
- Create: `admin/src/client/ai/aiApi.test.ts`
- Create: `admin/src/settings/AiSettingsPage.tsx`
- Create: `admin/src/settings/aiSettingsApi.ts`
- Create: `admin/src/settings/aiSettingsApi.test.ts`
- Create: `admin/src/settings/AiSettingsPage.test.tsx`
- Modify: `admin/src/App.tsx`
- Modify: `admin/src/layout/adminNavigation.ts`
- Modify: `admin/src/layout/AdminShell.test.tsx`
- Modify: `admin/src/settings/locales/en-US.ts`
- Modify: `admin/src/settings/locales/zh-CN.ts`

**Interfaces:**
- `streamChat(request, handlers, signal)` parses SSE incrementally across arbitrary chunk boundaries and rejects malformed or unterminated events.
- `AiSettingsPage` edits provider enablement, URL, write-only key, system prompt, and a reorderable model table with upstream ID, display name, visibility, and default selection.

- [ ] **Step 1: Write failing API parser and admin page tests**

  Test split SSE frames, terminal errors, abort, strict response validation, write-only secret behavior, add/remove model, single default enforcement, connection testing, saving, and admin navigation.

- [ ] **Step 2: Run focused tests and confirm RED**

  Run: `npm test -- --run src/client/ai/aiApi.test.ts src/settings/aiSettingsApi.test.ts src/settings/AiSettingsPage.test.tsx src/layout/AdminShell.test.tsx`

- [ ] **Step 3: Implement strict API clients**

  Reuse `clientFetch` for authenticated streaming and `apiRequest` plus `responseValidation` for administration. Never cache or return the API key from GET.

- [ ] **Step 4: Implement the settings page**

  Follow the nearby metadata/system settings patterns, use HeroUI `Card`, `TextField`, `Switch`, `Button`, `Alert`, and `Table`/list semantics, and provide explicit loading, empty, conflict, unavailable, saving, and test states.

- [ ] **Step 5: Wire route, navigation, and translations**

  Add `/admin/settings/ai` under System and update exact navigation tests without replacing current localization work.

- [ ] **Step 6: Run focused tests and typecheck**

  Run: `npm test -- --run src/client/ai/aiApi.test.ts src/settings/aiSettingsApi.test.ts src/settings/AiSettingsPage.test.tsx src/layout/AdminShell.test.tsx && npm run typecheck`

---

### Task 5: Build the HeroUI Pro AI Conversation Experience

**Files:**
- Create: `admin/src/client/ai/AiChatPage.tsx`
- Create: `admin/src/client/ai/AiChatPage.test.tsx`
- Modify: `admin/src/client/ClientApp.tsx`
- Modify: `admin/src/client/layout/ClientShell.tsx`
- Modify: `admin/src/client/layout/ClientShell.test.tsx`

**Interfaces:**
- `/app/ai` loads visible models and the authenticated user's conversations.
- The page combines HeroUI Pro `ChatListView`, `ChatConversation`, `ChatMessage`, `ChatLoader`, `ChatSource`, `ChatTool`, `Markdown`, `PromptInput`, and `PromptSuggestion` through verified local beta.7 APIs.

- [ ] **Step 1: Write failing page and navigation tests**

  Cover the empty state, recommendation prompts, visible model names only, message send, streaming deltas, tool status, source links, stop generation, error retry, conversation selection/deletion, responsive history trigger, and the new global navigation item.

- [ ] **Step 2: Run focused tests and confirm RED**

  Run: `npm test -- --run src/client/ai/AiChatPage.test.tsx src/client/layout/ClientShell.test.tsx`

- [ ] **Step 3: Implement the responsive workspace**

  Use a dense two-column desktop layout with a stable-width conversation rail and an unframed chat surface; use a Drawer for history on smaller viewports. Keep the composer fixed within the page layout without overlapping messages or the global navbar.

- [ ] **Step 4: Implement chat state and interactions**

  Optimistically append the user message, stream assistant events, expose stop/retry/new-chat/delete actions, update source links to `/app/items/:id`, and preserve the selected allowed model per conversation.

- [ ] **Step 5: Wire route and global navigation**

  Add `/app/ai` after Rankings in desktop and mobile navigation, using a Lucide sparkle/message icon and localized `AI 助手` label.

- [ ] **Step 6: Run focused tests, all frontend tests, and static checks**

  Run: `npm test -- --run src/client/ai/AiChatPage.test.tsx src/client/layout/ClientShell.test.tsx && npm test -- --run && npm run typecheck && npm run lint && npm run build`

---

### Task 6: Documentation, End-to-End Verification, and Quality Review

**Files:**
- Modify: `README.md`
- Modify: `docs/api-parity.md`
- Add or modify the existing Playwright spec that covers authenticated client navigation and admin settings.

**Interfaces:**
- Documents `TJXY_CREDENTIAL_KEYRING`, OpenAI-compatible endpoint requirements, administrator setup, supported AI routes, data/privacy boundaries, and current structured-retrieval limitation.

- [ ] **Step 1: Update nearby documentation**

  Describe configuration without real secrets, clarify that the first release uses live structured catalog tools rather than embeddings, and list the administrator and user-facing API surfaces.

- [ ] **Step 2: Run full Rust checks**

  Run: `cargo fmt --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace`

- [ ] **Step 3: Run full frontend checks**

  Run: `npm test -- --run && npm run typecheck && npm run lint && npm run build`

- [ ] **Step 4: Start the application and perform browser QA**

  Verify `/app/ai` and `/admin/settings/ai` at desktop and mobile sizes, light and dark themes, nonblank content, no horizontal overflow, no overlap, keyboard operation, loading/error/empty states, and a real configured-provider conversation when credentials are available.

- [ ] **Step 5: Perform final code-quality and security review**

  Inspect the final diff for plaintext secret exposure, SSRF weaknesses, cross-user access, unbounded model/tool loops, swallowed errors, accidental raw metadata exposure, blocking work in async handlers, and unrelated file churn. Resolve all load-bearing findings and rerun affected checks.
