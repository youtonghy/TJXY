# HeroUI Admin Rebuild Design

## Status

Approved on 2026-07-29. The visual direction, frontend architecture, information
architecture, key page patterns, interaction states, migration order, and verification
scope were approved during design review.

## Context

The current `admin/` application is a React 19 and React Admin 5 single-page application
served under `/admin/`. React Admin's Material UI layer currently owns the login screen,
application shell, resource pages, forms, and part of the feedback system. Custom pages
also use Material UI directly. This has produced two overlapping layout styles: React
Admin resource pages and individually composed operational pages.

The result is visually inconsistent and structurally fragile. Page widths, headings,
loading states, error presentation, confirmation patterns, mobile tables, and cloud
storage workflows vary by feature. The login page compounds the problem with a small,
vertically floating card, redundant identity elements, mismatched control widths, and
large unstructured empty areas on tall viewports.

The data and authentication foundations are not the problem. The custom HTTP client,
session handling, authentication provider, Users data provider, typed command clients,
and the feature-specific API modules already encode the required backend contracts.
The rebuild must preserve those contracts while replacing the complete presentation
layer with HeroUI v3.

## Goals

- Rebuild the complete administrator interface with HeroUI v3 and Tailwind CSS v4.
- Remove Material UI, Emotion, and the Material UI presentation layer from React Admin.
- Preserve existing backend APIs, authentication behavior, route compatibility, and
  production static-serving boundaries.
- Establish one restrained operational design language for login, navigation, lists,
  forms, details, workflows, dialogs, and feedback states.
- Make desktop, tablet, and mobile layouts deliberate rather than relying on wide-table
  overflow as the default narrow-screen behavior.
- Keep the existing behavior coverage while adding visual, accessibility, focus, and
  sensitive-data verification.

## Non-Goals

- No backend API, database, authentication protocol, or static-server change.
- No public user-facing media application or dashboard.
- No cross-origin deployment, token-cookie migration, or new session revocation API.
- No speculative analytics, charts, global search, command palette, or bulk operations.
- No HeroUI Pro dependency. The rebuild uses the open-source `@heroui/react` package.
- No dark-first theme. A future dark theme may reuse the semantic tokens, but it is not
  required for this rebuild.

## Selected Approach

Use `ra-core` as the headless administration foundation and HeroUI v3 as the complete
view layer.

`ra-core` continues to own provider contexts, authentication orchestration, query cache,
resource routing, CRUD controllers, permissions, and notification state. HeroUI owns the
application shell, login, forms, tables, tabs, overlays, toasts, skeletons, empty states,
errors, and responsive behavior. This preserves the mature behavioral core without
keeping the Material UI skin.

The alternative of rebuilding routing, queries, authentication redirects, and CRUD state
from first principles was rejected because it duplicates working infrastructure and
expands regression risk without improving the interface. A mixed MUI/HeroUI migration
was rejected as an end state because it would retain two theme systems, two component
languages, and both dependency families.

## Visual Direction

The selected direction is **Operational Neutral**:

- light neutral page and sidebar surfaces;
- dark high-contrast text;
- a restrained teal accent for the primary action and active navigation;
- semantic green, amber, and red only for status and feedback;
- an 8px spacing grid and component radii of 8px or less;
- compact but readable tables and forms;
- subtle depth only for active navigation, overlays, and genuinely elevated tools;
- no gradients, decorative illustrations, oversized headings, nested cards, or
  dashboard-card composition.

HeroUI semantic variants and tokens are preferred over raw visual colors. Custom theme
values use `oklch` variables. Tailwind utilities control layout and responsive behavior;
HeroUI components retain their accessible default interaction semantics. Lucide React
provides product icons. Familiar icon-only actions use HeroUI Tooltip and explicit
accessible names.

## Application Shell And Navigation

Desktop uses a full-height 224-240px sidebar that is a sibling of the main content area.
The sidebar contains:

- TJXY Admin branding at the top;
- `Manage`: Users, Access, Libraries;
- `Operations`: Tasks;
- `Storage`: Google Drive, OneDrive;
- the current administrator identity and account menu at the bottom.

There is no persistent global app bar. Branding and identity are not repeated above the
content. Each page owns a compact header containing breadcrumbs where useful, one H1,
a concise description, and the primary page action. Content widths are selected by page
type: forms and readable detail content remain constrained, while tables may use a wider
bounded container.

On tablet and mobile, the persistent sidebar is replaced by a menu button and HeroUI
Drawer. The drawer exposes the same labels and groups. The main content never shifts
horizontally when the drawer opens.

## Routes And Information Architecture

Existing route compatibility is preserved:

- `/admin/login`
- `/admin/users`
- `/admin/users/create`
- `/admin/users/:id/show`
- `/admin/users/:id`
- `/admin/access`
- `/admin/tasks`
- `/admin/libraries`
- `/admin/storage/google-drive`
- `/admin/storage/onedrive`

The rebuild adds `/admin/libraries/:id` as the durable library detail and policy-editing
route. Complex library editing leaves the oversized modal and becomes a deep-linkable
page. Browser refresh and Back preserve the user's location.

The Access tab is reflected in a `tab` search parameter. `/admin/access` defaults to
Devices; `/admin/access?tab=api-keys` restores API Keys across reload, sharing, and
browser navigation. Mounting only the selected tab remains intentional so API key reveal
state is destroyed when the tab changes.

Google Drive and OneDrive retain their stable routes but share one workflow structure.
Provider-specific capabilities, such as Shared Drives, remain inside their respective
step instead of creating separate page conventions.

## Page Patterns

### Login

The login page uses a full-height neutral canvas with a small anchored TJXY Admin brand,
a centered form surface no wider than 380px, and a small server-availability indicator.
The form heading is `Administrator sign in`; it does not repeat the product name or use
an avatar/shield badge. Username and password fields have visible labels, the password
reveal control is an icon button with an accessible name, and the submit button spans the
form width. Authentication errors are displayed inline and remain associated with the
form. On mobile, the brand participates in normal flow above the form.

The availability indicator performs one unauthenticated request to the existing
`/health/ready` endpoint when the page mounts. It reports `Server available` only after a
successful response and otherwise reports `Server unavailable`; it does not poll, delay
form submission, or expose response details.

### Index

Users, Libraries, Devices, API Keys, scheduled tasks, and durable jobs share an index
grammar:

1. page title and one primary action;
2. optional derived summaries when the current response already contains the data;
3. search or filters only where they can operate correctly on the complete loaded set;
4. one semantic data collection;
5. pagination or result summary.

Users summary counts are computed from the complete `/Users` response inside the existing
client-side list provider and returned as list metadata before the requested page is
sliced. Pages that do not receive a complete response omit aggregate summaries rather
than deriving misleading totals from the visible page.

Desktop uses HeroUI Table when column comparison matters. At 390px, Users, Libraries,
Devices, and API Keys become vertically labeled records rather than horizontally scrolled
desktop tables. Scheduled tasks become action rows. Recent durable jobs and background
candidates may retain an internally scrolling table when cross-row comparison is more
important than item actions, but document-level overflow is forbidden.

### Detail And Edit

User and Library editing use durable pages. Entity identity and read-only status remain
visible beside or above the editable content. Commands stay independent:

- User: rename, access policy, password replacement or clear, delete.
- Library: identity, enabled/profile policy, advanced overrides, background candidates,
  delete.

Each command has its own pending, error, and success state because the backend operations
are not atomic. Sections are separated by spacing and semantic headings rather than
nested cards. The danger zone is always last. Destructive actions open a confirmation
modal and identify the affected entity.

### Workflow

Cloud storage setup uses three explicit phases:

1. Authorize: choose an enabled target library and open provider authorization.
2. Choose folder: browse provider locations and select a folder.
3. Review: name the storage binding, confirm the target, and submit.

The workflow always offers `Restart authorization` after authorization begins. Google
Drive exposes My Drive and Shared Drive selection within the folder phase and retains
Shared Drive pagination. Both providers use the same naming, loading, empty, error, and
success conventions. Successful binding remains visible with restart-required and
initial-job details when returned by the backend.

### Atomic Modal

HeroUI Modal is limited to focused, short actions: create, device rename, revoke, delete,
and other confirmations. User and library identity editing remains on durable pages.
Modal content is sized to the task. Pending submissions prevent closing, repeat
submission, and conflicting actions. Focus is trapped while open and returns to the
trigger after close.

## Shared Components

The rebuild introduces focused UI boundaries rather than a broad internal component
framework:

- `AdminShell`: desktop sidebar, mobile drawer, identity, outlet, and skip link.
- `PageHeader`: breadcrumbs, H1, description, and primary action slot.
- `AsyncContent`: initial loading, refresh, no-data error, stale-data error, and empty
  state selection.
- `StatusChip`: text plus semantic tone for user, library, task, and job states.
- `ResponsiveCollection`: semantic table on wide screens and labeled records where the
  mobile workflow benefits from reflow.
- `ConfirmDialog`: consistent irreversible or high-impact confirmation.
- `AdminNotifications`: bridges `ra-core` notification state to HeroUI Toast.
- `PageError`, `AccessDenied`, `NotFound`, and `LoadingPage`: complete `ra-core` UI slots.

Feature-specific fields, forms, and rows stay with their owning feature. A shared helper
is added only when at least two features require the same behavior and no equivalent
already exists.

## Data And State Flow

The existing `api/httpClient.ts`, response validation, session module, authentication
provider, Users data provider, user commands, and feature API modules remain the backend
boundary. Imports that only require provider types move from `react-admin` to `ra-core`.

`ra-core` controllers and hooks mediate resource reads and writes. User resource pages use
`ListBase`, `CreateBase`, `ShowBase`, and `EditBase` or the equivalent controller hooks
with HeroUI children. Mutations currently issued directly through the exported data
provider move through `ra-core` mutation hooks so cache invalidation and authentication
error handling remain consistent.

Custom feature pages continue to use their typed API modules, but all API failures pass
through the shared authentication-error path. A `401` clears the session and redirects to
login with the original target. A `403` preserves the session and renders an access-denied
state. No error is silently swallowed.

## Loading, Empty, Error, And Success States

- Initial loading uses HeroUI Skeleton shaped like the eventual content.
- Refresh and polling retain valid existing data and display a local progress indicator.
- Empty state is rendered only after a successful response proves the collection is empty.
- A failed initial load renders an inline error with Retry instead of an empty state.
- A failed refresh with retained data renders a non-blocking stale-data banner.
- HeroUI Toast reports the result of a user-triggered command.
- Errors requiring a decision, conflicts, and multi-step workflow results remain inline.
- Form input and user-entered drafts remain intact after mutation failure.
- Controls use `isPending` and stable dimensions so progress does not move the layout.

Library policy version conflicts never overwrite newer server state. The page keeps the
local draft, reports that the library changed elsewhere, and offers an explicit reload of
the latest server version.

## Destructive Actions

Irreversible or high-impact actions use `ConfirmDialog`: user deletion, library deletion,
API key deletion, device-session revocation, and cancelling an active scheduled task.
The confirmation names the target and uses a danger action. Background-candidate unpin
remains a direct low-risk action because it can be restored; it uses subordinate danger
styling and reports completion without a modal.

## Security And Sensitive Data

The existing session token remains in `sessionStorage` and is cleared on logout and
authentication failure. The rebuild does not move tokens to local storage or URLs.

API key plaintext exists only in the active component's in-memory state. It must never be
written to local or session storage, URL parameters, notifications, user-facing errors,
console output, Playwright traces, or screenshots. Tab changes, reloads, and component
unmounts return the value to its mask. Copy success does not repeat the key value.

Passwords are never retained after a successful command and are not included in errors,
logs, traces, or screenshots. Backend error bodies continue to be mapped to safe,
actionable messages.

## Accessibility

- Every page has one H1 and logical section headings.
- A skip link moves keyboard focus to main content.
- Navigation exposes labels in desktop and drawer modes and marks the current page.
- Tables, rows, headers, and mobile field labels retain semantic roles.
- Icon-only controls have a specific `aria-label` and HeroUI Tooltip.
- Status is expressed with text as well as color.
- Form errors are programmatically associated with their fields or form summary.
- Toast and inline feedback use appropriate live-region semantics without duplicate
  announcements.
- Modal and Drawer trap focus, close by documented keyboard behavior when not pending,
  and restore focus to their trigger.
- Layout and text remain usable at 200% zoom and with reduced-motion preferences.

## Responsive Rules

The supported verification viewports are 1440x900, 768x1024, and 390x844. Content must
also remain stable between those breakpoints.

- Desktop: persistent sidebar and bounded content.
- Tablet: drawer navigation; wide collections remain tables if they fit the content area.
- Mobile: drawer navigation, stacked page actions, full-width form controls, and reflowed
  action records.
- Breadcrumbs may horizontally scroll within their own line but cannot expand the page.
- Dialogs with long forms become drawers or full-height sheets only when the task cannot
  fit a constrained modal; ordinary confirmations remain compact.
- No document-level horizontal overflow or intersecting action controls is permitted.

## Migration Sequence

1. Add `ra-core` as a direct dependency plus HeroUI v3, HeroUI styles, Tailwind CSS v4,
   Tailwind PostCSS, PostCSS, tailwind-variants, Lucide React, `@axe-core/playwright`,
   semantic theme tokens, and updated test utilities.
2. Replace the Material UI Admin wrapper with `ra-core`; deliver AdminShell, Login,
   notifications, loading, error, access-denied, and not-found UI together so no route
   can fall through to a blank screen.
3. Migrate the complete Users vertical slice and verify resource routing, CRUD, command
   mutation, cache, authentication, deep links, and responsive behavior.
4. Migrate Access and its Devices/API Keys panels.
5. Migrate Tasks while preserving polling, manual commands, and durable-job refresh.
6. Migrate Libraries, add the library detail route, and migrate background candidates.
7. Extract the shared storage workflow and migrate Google Drive and OneDrive.
8. Remove `react-admin`, Material UI, MUI icons, Emotion, the old MUI theme, obsolete test
   wrappers, and MUI-specific bundler groups.
9. Update nearby README and admin documentation for dependencies, commands, route
   behavior, and the new UI architecture.
10. Run the full verification matrix and perform final code-quality, security, bundle,
    performance, memory-lifecycle, and responsive-layout review.

Each migration step must leave a testable interface. Temporary MUI/HeroUI coexistence is
allowed only within the branch during the migration and must not remain in the final
dependency graph or production bundle.

## Verification

Existing checks remain required:

```text
npm --prefix admin ci
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build
npm --prefix admin run e2e
```

The current 104 unit tests and production lifecycle tests are migrated rather than
discarded. New coverage includes:

- stable visual screenshots for login, shell, Users, Access, Tasks, Libraries, library
  edit, storage workflows, and representative dialogs at all three viewports; visual
  baselines use deterministic network fixtures or seeded records with normalized dynamic
  timestamps;
- loading, empty, initial-error, stale-error, success, authorization, conflict, and
  popup-blocked states;
- keyboard traversal, skip link, mobile drawer, modal initial focus, focus trap, Escape,
  pending-close prevention, and focus restoration;
- automated accessibility checks with `@axe-core/playwright` on representative pages;
- 200% zoom and reduced-motion smoke checks;
- long names, identifiers, breadcrumbs, labels, and error text;
- document overflow and visible action-control intersection checks;
- API key masking after tab change, reload, and unmount; absence from browser storage,
  URLs, console diagnostics, traces, and all captured screenshots; a dedicated reveal
  test may inspect the live DOM but must disable tracing and must not capture an image;
- route restoration after login and deep-link reload for existing and new routes;
- a production bundle check confirming MUI, Emotion, and `react-admin` UI artifacts are
  absent.

Chromium remains the required production lifecycle browser. A focused WebKit smoke suite
covers login, navigation, one resource workflow, one modal, and logout to catch React
Aria/browser integration differences without multiplying the full backend lifecycle cost.

## Risks And Mitigations

### Headless Admin UI Gaps

`ra-core` supplies state but not a complete visual shell. Switching away from the Material
UI Admin wrapper without notifications, loading, error, 404, and access-denied components
would create silent failures or blank screens. These components migrate atomically with
the shell.

### Cache And Mutation Regressions

Some existing commands call API modules or the exported provider directly. Resource
mutations move through `ra-core` hooks, while custom feature mutations explicitly refetch
their authoritative state. Tests cover refresh persistence and stale response rejection.

### Route Regressions

The application currently combines an outer BrowserRouter with an admin basename. The
rebuild preserves one router owner and verifies direct navigation, post-login return,
Back/Forward behavior, and production deep-link fallback before removing React Admin UI.

### Large Visual Migration

Seventeen production files and roughly three thousand presentation lines directly use
Material UI. Migration proceeds by testable feature slice, but final integration and
dependency removal are serialized in one worktree so shared routes, theme, and shell
cannot diverge.

### Mobile Behavior

Replacing desktop table overflow with mobile records can accidentally hide comparison
data or actions. Every transformed collection defines its required visible fields and
actions explicitly, and tests assert the same workflow remains possible at 390px.

### Parallel Cloud Pagination Work

The active `codex/cloud-directory-pagination` worktree changes both provider APIs and the
Google Drive and OneDrive pages. The Storage migration must begin from a branch state that
already contains that completed work. Before the Storage slice, update this branch from
the latest integrated `main` and resolve the shared workflow against the paginated
directory contracts; do not recreate or discard those pagination helpers.

## Acceptance Criteria

- Every existing admin workflow remains functional through the same backend contracts.
- Existing routes remain compatible, and the added library route and Access tab query
  behave correctly on refresh and browser navigation.
- The final application uses HeroUI v3 and Tailwind CSS v4 without Material UI, Emotion,
  or the React Admin Material UI package in dependencies or production output.
- Login, shell, page hierarchy, feedback states, confirmations, and responsive behavior
  follow this design consistently.
- No supported viewport has document overflow, overlapping controls, clipped text, or an
  inaccessible action.
- Authentication, authorization, API key, password, and error-redaction guarantees are
  preserved.
- Type checking, linting, unit tests, production build, lifecycle E2E, visual checks, and
  accessibility checks pass.
