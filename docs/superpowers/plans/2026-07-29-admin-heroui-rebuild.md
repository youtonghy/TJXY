# HeroUI Admin Rebuild Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the complete TJXY administrator presentation layer with a stable, responsive HeroUI v3 interface while preserving every existing backend, authentication, route, and sensitive-data contract.

**Architecture:** Keep `ra-core` as the headless admin controller and routing layer. Keep the existing typed HTTP, authentication, Users data-provider, command, and feature API modules as the backend boundary. Build the shell, resource views, forms, tables, dialogs, notifications, and workflow states with HeroUI v3, Tailwind CSS v4, and Lucide icons. Migrate one vertical slice at a time, then remove React Admin's Material UI package, MUI, and Emotion only after all production imports have moved.

**Tech Stack:** React 19.2.7, TypeScript 6.0.3, Vite 8.1.5, `ra-core` 5.15.0, HeroUI React 3.2.2, HeroUI Styles 3.2.2, Tailwind CSS 4.3.3, React Router 7.18.1, React Hook Form 7.82.0, Lucide React 1.27.0, Vitest 4.1.10, Testing Library, Playwright 1.61.1, and `@axe-core/playwright` 4.12.1.

## Global Constraints

- Execute in `/Users/youtonghy/github/Project/TJXY/.worktrees/admin-heroui-rebuild` on branch `codex/admin-heroui-rebuild`.
- Before Task 1 implementation, resolve the Cloud Pagination Gate below. The current audit on 2026-07-29 shows commit `509cbb3` is not an ancestor of `main`, so the complete plan is not yet executable end to end.
- Treat `docs/superpowers/specs/2026-07-29-admin-heroui-rebuild-design.md` as the approved source of truth. A behavioral change not described there requires another design review.
- Keep `/admin/` as the SPA basename and preserve all existing URLs. Add only `/admin/libraries/:id` and the `?tab=api-keys` Access state approved in the design.
- Do not change Rust handlers, database schema, authentication protocol, production static serving, OAuth contracts, or feature API response shapes for this UI rebuild.
- Keep exactly one router owner: the existing outer `BrowserRouter`; let `CoreAdmin` consume that router through its basename rather than adding another browser router.
- Keep `api/httpClient.ts`, `auth/session.ts`, `auth/authProvider.ts`, `api/dataProvider.ts`, user commands, and feature API modules as the network and security boundary. Presentation components never call `fetch` directly except the dedicated plain-text readiness probe.
- A `401` clears the session and restores the original route after login. A `403` preserves the session and renders Access Denied. Do not merge these states.
- In every custom feature API catch path, await `useLogoutIfAccessDenied(error)` before rendering or notifying the error. Continue with page-local handling only when it resolves `false`; this prevents duplicate feedback after a 401 logout or 403 redirect.
- Never place access tokens, passwords, API key plaintext, OAuth values, or internal backend errors in URLs, storage beyond the existing session token contract, logs, notifications, screenshots, traces, or test failure messages.
- Keep API key plaintext only in the active `ApiKeysPanel` instance. Tab changes, authoritative reloads, and unmounts must remove it from the DOM.
- Use HeroUI semantic props and theme variables for visual state. Use Tailwind utilities for layout. Do not recreate HeroUI controls with raw div/button implementations.
- Use Lucide icons, visible labels for fields, a tooltip plus specific `aria-label` for icon-only controls, and text in every status indicator.
- Keep radii at 8px or less, letter spacing at zero, no gradients, no nested cards, and no decorative dashboard composition.
- Use pessimistic mutations for scoped commands. Preserve valid data during refresh/polling, preserve drafts after mutation failure, and distinguish initial error from successful empty state.
- Before adding a helper, search with `rg` for an existing equivalent. Shared components in this plan exist because at least two feature slices need the contract.
- Follow test-driven implementation: add or update the focused failing test first, run the stated red command, implement the smallest complete behavior, then run the green command.
- Keep tasks serial when they touch `App.tsx`, shared UI, dependencies, routes, or configuration. Read-only audits and code review may use subagents; the primary agent applies and verifies all edits.
- Commit after each task only when its focused tests, typecheck, and lint are green. Never combine unrelated user work into these commits.

## Cloud Pagination Gate

The completed local branch `codex/cloud-directory-pagination` ends at `509cbb3` and changes backend cursor contracts plus both Storage pages. The approved design requires this work to be integrated before the HeroUI Storage migration. Resolve the gate before Task 1 so the implementation does not accumulate nine UI commits before discovering a known branch dependency.

```bash
git merge-base --is-ancestor 509cbb3 main
```

Expected exit code: `0`. It currently exits `1`. Do not merge or cherry-pick the pagination branch into `main` without the repository owner's explicit direction. After the owner integrates it, merge the updated `main` into `codex/admin-heroui-rebuild`, run the pagination verification commands listed in Task 10, and then begin Task 1. If the pagination work is integrated through rewritten commits, the owner must identify the equivalent integrated commit and the executor must verify the complete backend and frontend pagination file set before accepting the gate.

## Target File Map

```text
admin/
  postcss.config.mjs                     # Tailwind v4 PostCSS plugin
  src/
    styles.css                           # Tailwind, HeroUI, reset, global layout
    theme.css                            # TJXY semantic HeroUI tokens
    App.test.tsx                         # CoreAdmin route/auth slot integration
    api/
      readiness.ts                       # one-shot plain-text /health/ready probe
      readiness.test.ts
    auth/
      LoginPage.tsx                      # custom HeroUI login
      LoginPage.test.tsx
      loginDestination.ts                # validated post-login route restoration
      loginDestination.test.ts
    layout/
      AdminLayout.tsx                    # CoreAdmin layout adapter
      AdminRouteGuard.tsx                # distinguishes anonymous 401 from preserved 403
      AdminRouteGuard.test.tsx
      AdminShell.tsx                     # desktop shell/mobile drawer
      AdminShell.test.tsx
      adminNavigation.ts                 # grouped route metadata
    ui/
      AdminNotifications.tsx             # ra-core notifications to HeroUI Toast
      AdminNotifications.test.tsx
      AsyncContent.tsx
      AsyncContent.test.tsx
      ConfirmDialog.tsx
      ConfirmDialog.test.tsx
      PageHeader.tsx
      PageHeader.test.tsx
      ResponsiveCollection.tsx
      ResponsiveCollection.test.tsx
      StatusChip.tsx
      SystemPages.tsx                    # loading/error/access-denied/not-found slots
      HeroUiSmoke.test.tsx
    test/
      renderWithAdmin.tsx                # MemoryRouter + CoreAdminContext test harness
    users/                               # HeroUI resource pages, existing commands retained
    access/                              # HeroUI tabs, devices, API keys
    tasks/                               # HeroUI scheduled tasks and durable jobs
    libraries/
      LibrariesPage.tsx                  # collection and create action
      LibraryCreateDialog.tsx
      LibraryEditPage.tsx                # durable /libraries/:id route
      LibraryPolicyForm.tsx
      HybridCandidatesPanel.tsx
    storage/
      StorageWorkflow.tsx                # shared phase frame only
      StorageWorkflow.test.tsx
      FolderBrowser.tsx                  # shared folder interaction surface
  e2e/
    adminFixtures.ts                     # deterministic frontend route fixtures
    visual.spec.ts
    accessibility.spec.ts
    webkit-smoke.spec.ts
```

Existing API modules and most existing test files are modified in place. `theme.ts`, `access/ResponsiveTableCell.tsx`, and `libraries/HybridCandidatesDialog.tsx` are deleted only after their replacements are green.

---

### Task 1: Record the Approved Baseline and Install the HeroUI Foundation

**Files:**

- Modify: `admin/package.json`
- Modify: `admin/package-lock.json`
- Create: `admin/postcss.config.mjs`
- Create: `admin/src/styles.css`
- Create: `admin/src/theme.css`
- Create: `admin/src/ui/HeroUiSmoke.test.tsx`
- Modify: `admin/src/main.tsx`
- Track: `docs/superpowers/specs/2026-07-29-admin-heroui-rebuild-design.md`
- Track: `docs/superpowers/plans/2026-07-29-admin-heroui-rebuild.md`

**Interfaces:**

- Pin the versions named in the plan header. Add production dependencies `@heroui/react`, `@heroui/styles`, `ra-core`, `react-hook-form`, `tailwind-variants`, and `lucide-react`.
- Add development dependencies `tailwindcss`, `@tailwindcss/postcss`, `postcss`, and `@axe-core/playwright`.
- Keep the existing MUI/Emotion/`react-admin` dependencies during migration; Task 11 removes them.
- Import global CSS once from `main.tsx`, before rendering `App`.

- [ ] Verify the isolated branch and baseline before changing dependencies:

```bash
git status --short --branch
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build
```

Expected baseline: branch `codex/admin-heroui-rebuild`; 20 Vitest files and 104 tests pass; the build still reports MUI and React Admin chunks.

- [ ] Add the smoke test before installing HeroUI:

```tsx
import { Button, Input, Label, TextField } from '@heroui/react';
import { render, screen } from '@testing-library/react';

it('renders accessible HeroUI controls', () => {
  render(
    <>
      <TextField>
        <Label>Administrator name</Label>
        <Input />
      </TextField>
      <Button>Save changes</Button>
    </>,
  );

  expect(screen.getByRole('textbox', { name: 'Administrator name' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Save changes' })).toBeEnabled();
});
```

- [ ] Run the red test and confirm it fails because `@heroui/react` is unresolved:

```bash
npm --prefix admin test -- --run src/ui/HeroUiSmoke.test.tsx
```

- [ ] Install exact dependencies with npm so `package-lock.json` is the only lockfile changed:

```bash
npm --prefix admin install --save-exact @heroui/react@3.2.2 @heroui/styles@3.2.2 ra-core@5.15.0 react-hook-form@7.82.0 tailwind-variants@3.2.2 lucide-react@1.27.0
npm --prefix admin install --save-dev --save-exact tailwindcss@4.3.3 @tailwindcss/postcss@4.3.3 postcss@8.5.20 @axe-core/playwright@4.12.1
```

- [ ] Add `postcss.config.mjs`:

```js
export default {
  plugins: {
    '@tailwindcss/postcss': {},
  },
};
```

- [ ] Add `styles.css` with imports in this exact order, then global box sizing, body defaults, focus/skip-link behavior, `overflow-wrap`, reduced motion, and no document-level horizontal overflow:

```css
@import "tailwindcss";
@import "@heroui/styles";
@import "./theme.css";

@layer base {
  *, *::before, *::after { box-sizing: border-box; }
  html, body, #root { min-height: 100%; }
  body { margin: 0; background: var(--color-background); color: var(--color-foreground); }
  :where(p, td, th, dd, a) { overflow-wrap: anywhere; }
}
```

- [ ] Add `theme.css` with the Operational Neutral token contract. Keep the HeroUI semantic variable names and set `--radius-lg` no higher than `0.5rem`:

```css
:root, [data-theme='light'] {
  --font-sans: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  --radius-sm: 0.375rem;
  --radius-md: 0.5rem;
  --radius-lg: 0.5rem;
  --color-background: oklch(97.8% 0.004 180);
  --color-foreground: oklch(24% 0.018 215);
  --color-accent: oklch(54% 0.105 180);
  --color-success: oklch(60% 0.14 151);
  --color-warning: oklch(70% 0.14 78);
  --color-danger: oklch(58% 0.2 25);
}
```

- [ ] Import `./styles.css` from `main.tsx`, run the focused test and build, and inspect the emitted CSS for HeroUI and Tailwind output:

```bash
npm --prefix admin test -- --run src/ui/HeroUiSmoke.test.tsx
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin run build
npm --prefix admin ls ra-core
rg -n -- "--color-accent|data-slot" admin/dist/assets/*.css
```

`npm ls ra-core` must show one `ra-core@5.15.0` instance so old React Admin pages and the new headless shell share the same contexts during migration.

- [ ] Commit the approved documents and foundation:

```bash
git add admin/package.json admin/package-lock.json admin/postcss.config.mjs admin/src/main.tsx admin/src/styles.css admin/src/theme.css admin/src/ui/HeroUiSmoke.test.tsx docs/superpowers/specs/2026-07-29-admin-heroui-rebuild-design.md docs/superpowers/plans/2026-07-29-admin-heroui-rebuild.md
git commit -m "feat(admin): establish HeroUI foundation"
```

---

### Task 2: Build the Shared Operational UI Contracts

**Files:**

- Create: `admin/src/ui/PageHeader.tsx`
- Create: `admin/src/ui/PageHeader.test.tsx`
- Create: `admin/src/ui/AsyncContent.tsx`
- Create: `admin/src/ui/AsyncContent.test.tsx`
- Create: `admin/src/ui/StatusChip.tsx`
- Create: `admin/src/ui/ResponsiveCollection.tsx`
- Create: `admin/src/ui/ResponsiveCollection.test.tsx`
- Create: `admin/src/ui/ConfirmDialog.tsx`
- Create: `admin/src/ui/ConfirmDialog.test.tsx`
- Create: `admin/src/ui/SystemPages.tsx`
- Create: `admin/src/ui/SystemPages.test.tsx`

**Interfaces:**

```ts
export interface BreadcrumbItem {
  label: string;
  to?: string;
}

export interface PageHeaderProps {
  title: string;
  description?: string;
  breadcrumbs?: readonly BreadcrumbItem[];
  actions?: React.ReactNode;
}

export interface AsyncContentProps {
  isPending: boolean;
  error: unknown | null;
  hasData: boolean;
  isEmpty: boolean;
  onRetry: () => void;
  loading: React.ReactNode;
  empty: React.ReactNode;
  children: React.ReactNode;
}

export type StatusTone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger';

export interface ResponsiveCollectionProps {
  ariaLabel: string;
  desktop: React.ReactNode;
  mobile: React.ReactNode;
}

export interface ConfirmDialogProps {
  trigger: React.ReactNode;
  title: string;
  description: React.ReactNode;
  confirmLabel: string;
  isPending: boolean;
  onConfirm: () => void | Promise<void>;
}
```

- [ ] Write focused tests first. They must assert:

  - `PageHeader` renders one H1, updates `document.title` to `<title> | TJXY Admin`, keeps action placement stable, and uses links only for breadcrumbs with `to`.
  - `AsyncContent` chooses skeleton for initial loading, `PageError` for initial failure, empty only for a successful empty result, and a stale-data alert plus children when a refresh fails after data exists.
  - `StatusChip` always contains visible status text and maps each tone to a HeroUI semantic color.
  - `ResponsiveCollection` gives both representations the same accessible label and applies the required breakpoint classes; computed breakpoint visibility is verified later in Playwright because JSDOM does not evaluate compiled Tailwind media rules.
  - `ConfirmDialog` names the target, focuses Cancel as the least-destructive action, disables close/Escape/confirm while pending, keeps the dialog open on rejected confirmation, and restores focus on success.
  - loading/error/access-denied/not-found pages each have one H1 and a useful navigation, retry, or Sign out action. Access Denied must offer Sign out without clearing the session merely by rendering; its explicit logout disables return-target capture so the next login falls back to Users instead of returning to Access Denied.

- [ ] Run the red tests:

```bash
npm --prefix admin test -- --run src/ui/PageHeader.test.tsx src/ui/AsyncContent.test.tsx src/ui/ResponsiveCollection.test.tsx src/ui/ConfirmDialog.test.tsx src/ui/SystemPages.test.tsx
```

- [ ] Implement `PageHeader` as an unframed header band. Use a constrained breadcrumb line, an H1 no larger than `text-2xl`, a compact description, and a wrapping action slot. Do not put the page header in a Card.

- [ ] Implement the `AsyncContent` state order exactly:

```tsx
if (isPending && !hasData) return loading;
if (error !== null && !hasData) return <PageError error={error} onRetry={onRetry} />;

return (
  <>
    {error !== null && <StaleDataAlert onRetry={onRetry} />}
    {isEmpty ? empty : children}
  </>
);
```

- [ ] Implement `StatusChip` with HeroUI `Chip`, `variant="soft"`, `size="sm"`, and an explicit tone-to-color map. Do not infer status text from color.

- [ ] Implement `ResponsiveCollection` with stable CSS media boundaries rather than JavaScript viewport listeners. Desktop is active from `640px`; mobile records are active below it. Both representations remain semantic in source, but the inactive branch must be hidden from the accessibility tree with CSS `display: none`. Unit tests assert markup/classes; Task 12 asserts computed visibility and the accessibility tree in a real browser.

- [ ] Implement `ConfirmDialog` with HeroUI's compound Modal API. Await `onConfirm`; close only after resolution; leave the dialog and trigger state intact after rejection. Use a fixed-height pending label area so the button does not resize.

- [ ] Run focused and shared validation:

```bash
npm --prefix admin test -- --run src/ui
npm --prefix admin run typecheck
npm --prefix admin run lint
```

- [ ] Commit the shared UI contracts:

```bash
git add admin/src/ui
git commit -m "feat(admin): add operational UI primitives"
```

---

### Task 3: Prepare the Headless Shell, Guard, and Notification Components

**Files:**

- Create: `admin/src/layout/AdminRouteGuard.tsx`
- Create: `admin/src/layout/AdminRouteGuard.test.tsx`
- Create: `admin/src/layout/AdminShell.tsx`
- Create: `admin/src/layout/AdminShell.test.tsx`
- Create: `admin/src/layout/adminNavigation.ts`
- Create: `admin/src/ui/AdminNotifications.tsx`
- Create: `admin/src/ui/AdminNotifications.test.tsx`
- Modify: `admin/src/auth/authProvider.ts` (move types to `ra-core`; add approved 403 redirect semantics)
- Modify: `admin/src/auth/authProvider.test.ts`
- Modify: `admin/src/api/dataProvider.ts` (types only, from `react-admin` to `ra-core`)
- Create: `admin/src/test/renderWithAdmin.tsx`
- Modify: `admin/src/test/setup.ts`

**Interfaces:**

```ts
export interface AdminNavigationGroup {
  label: 'Manage' | 'Operations' | 'Storage';
  items: readonly {
    label: string;
    to: string;
    icon: React.ComponentType<{ size?: number; 'aria-hidden'?: boolean }>;
  }[];
}
```

The exact navigation order is Users, Access, Libraries; Tasks; Google Drive, OneDrive. Desktop sidebar width is `15rem`. At widths below `1024px`, use a HeroUI Drawer opened by a 40x40 menu button. Main content has `id="main-content"`, `tabIndex={-1}`, and a skip link.

`CoreAdmin requireAuth` is intentionally not used: its catch-all treats every `checkAuth` rejection as logout, which violates the approved requirement to preserve the session on 403. `AdminRouteGuard` wraps the shell inside the layout and branches on the typed auth error.

- [ ] Add shell tests before implementation. Cover desktop grouped navigation, current-route indication, mobile drawer open/close/focus return, identity display, account menu, logout, skip-link focus, and no duplicated product header.

- [ ] Add guard tests before changing `App.tsx`. Cover pending auth, authenticated children, a stored-token 401 that renders `LogoutOnMount` and preserves the current path as the login return target, and a stored-token 403 that renders Access Denied without calling `authProvider.logout` or removing session storage. Verify its explicit Sign out action does clear the session.

- [ ] Add notification tests before implementation. Queue two `ra-core` notifications and assert both are drained exactly once in Strict Mode, translated, mapped from `info|success|warning|error` to HeroUI `info|success|warning|danger`, and timed with `autoHideDuration: null` as `timeout: 0` or otherwise `autoHideDuration ?? 4000`. Add a source contract test that this application never requests an undoable mutation; undo support may not be silently discarded.

- [ ] Extend `authProvider.test.ts` first. Preserve the current 401 session clear and add a 403 contract that keeps the session and rejects with `logoutUser: false` plus the `/admin/access-denied` redirect expected by `useLogoutIfAccessDenied`.

- [ ] Run the red tests:

```bash
npm --prefix admin test -- --run src/layout/AdminRouteGuard.test.tsx src/layout/AdminShell.test.tsx src/ui/AdminNotifications.test.tsx src/auth/authProvider.test.ts
```

- [ ] Implement `AdminRouteGuard` with `useAuthState<ApiError>({}, false)`. Render `LoadingPage` while pending, `AccessDeniedPage` for status 403, `LogoutOnMount` for any other rejected auth check, and children only when authenticated. Keep it unused by the production `AdminLayout` until Task 4 performs the atomic shell/login switch.

- [ ] Implement account identity with `useGetIdentity` and logout with `useLogout`. Identity failure is an auth error state, not an empty avatar. Use the first visible character only for the Avatar fallback.

- [ ] Implement `AdminNotifications` with `useNotificationContext`, `takeNotification`, and `useTranslate`. Consume one notification at a time in an effect; translate string messages with `messageArgs`, preserve a safe ReactNode message, map types explicitly, and apply the exact timeout contract from the test. The app's mutations remain pessimistic, so no undo notification may enter this bridge.

- [ ] Add `renderWithAdmin.tsx` to provide `MemoryRouter`, `CoreAdminContext`, a strict fake data provider, and a configurable auth provider without mocking `ra-core` internals. Migrate tests to it as their feature slice moves.

- [ ] Update type-only imports in `authProvider.ts` and `dataProvider.ts`. The only runtime provider change in this task is the tested 403 behavior: preserve the session and reject with `{ logoutUser: false, redirectTo: '/admin/access-denied', message: false }`.

- [ ] Add a test cleanup in `src/test/setup.ts` for HeroUI overlays/toasts and restore mocked media queries so modal/drawer tests do not leak state.

- [ ] Run auth, shell, notification, and unit validation without changing the production wrapper:

```bash
npm --prefix admin test -- --run src/layout/AdminRouteGuard.test.tsx src/layout/AdminShell.test.tsx src/ui/AdminNotifications.test.tsx src/auth/authProvider.test.ts src/api/dataProvider.test.ts
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin run build
```

- [ ] Commit the unused, tested shell infrastructure. Production still uses the existing wrapper until Task 4:

```bash
git add admin/src/layout/AdminRouteGuard.tsx admin/src/layout/AdminRouteGuard.test.tsx admin/src/layout/AdminShell.tsx admin/src/layout/AdminShell.test.tsx admin/src/layout/adminNavigation.ts admin/src/ui/AdminNotifications.tsx admin/src/ui/AdminNotifications.test.tsx admin/src/auth/authProvider.ts admin/src/auth/authProvider.test.ts admin/src/api/dataProvider.ts admin/src/test/renderWithAdmin.tsx admin/src/test/setup.ts
git commit -m "feat(admin): prepare HeroUI admin shell"
```

---

### Task 4: Rebuild Login and Atomically Switch to the Headless Shell

**Files:**

- Create: `admin/src/api/readiness.ts`
- Create: `admin/src/api/readiness.test.ts`
- Rewrite: `admin/src/auth/LoginPage.tsx`
- Create: `admin/src/auth/LoginPage.test.tsx`
- Create: `admin/src/auth/loginDestination.ts`
- Create: `admin/src/auth/loginDestination.test.ts`
- Modify: `admin/src/App.tsx`
- Create: `admin/src/App.test.tsx`
- Rewrite: `admin/src/layout/AdminLayout.tsx`

**Interfaces:**

```ts
export async function checkServerReadiness(signal: AbortSignal): Promise<boolean>;

export function loginDestination(state: unknown, origin: string): string;
```

`/health/ready` returns plain text (`ready` or `not ready`), so this dedicated probe must inspect only `response.ok`; do not weaken `apiRequest`'s JSON response validation.

- [ ] Test the readiness helper first: same-origin `/health/ready`, no Authorization header, `true` on 2xx, `false` on 503/network failure, and request abortion through the supplied signal.

- [ ] Test login first under React Strict Mode with a real `ra-core` auth context or a narrowly mocked `useLogin`. Cover visible labels, password mask/reveal accessible name, pending duplicate prevention, an inline error programmatically associated with the form, exactly one readiness request, unavailable readiness that does not disable submit, abort on unmount, desktop anchored brand, and mobile brand in normal flow. Test destination resolution separately: direct `/admin/login` falls back to `/admin/users`; an auth-loss deep link restores its `/admin/...` pathname and search; external, protocol-relative, login-loop, control-character, and non-admin targets fall back safely.

- [ ] Add `App.test.tsx` before the wrapper switch. Assert anonymous deep links reach Login and return to their original target, every custom route is inside the shell after authentication, resource deep links retain their URL, `/authentication-error`, `/access-denied`, and catch-all render the named state pages, and top-level render errors invoke the ErrorBoundary reset action.

- [ ] Run the red tests:

```bash
npm --prefix admin test -- --run src/api/readiness.test.ts src/auth/loginDestination.test.ts src/auth/LoginPage.test.tsx src/App.test.tsx
```

- [ ] Implement `checkServerReadiness` with a same-origin Request and no body parsing:

```ts
export async function checkServerReadiness(signal: AbortSignal): Promise<boolean> {
  try {
    const response = await fetch(new Request(
      new URL('/health/ready', window.location.origin),
      { signal },
    ));
    return response.ok;
  } catch {
    return false;
  }
}
```

- [ ] Build the login composition: anchored `TJXY Admin` brand, centered `max-w-[380px]` form surface, H1 `Administrator sign in`, visible Username and Password labels, Eye/EyeOff icon button with Tooltip, inline Alert for submission failure, full-width Sign in Button, and a text-plus-dot readiness status. Use `useLogin`; do not call `authProvider.login` directly. Compute a validated same-origin internal destination from `location.state.nextPathname` plus `nextSearch`, fall back to `/admin/users`, and pass it as the second `login(credentials, destination)` argument to avoid `ra-core@5.15.0`'s missing-state `undefinedundefined` path.

- [ ] Implement `loginDestination` by accepting only a record with a string pathname and optional search that is empty or begins `?`, resolving it against `origin`, requiring the same origin and a pathname beginning `/admin/`, rejecting `/admin/login`, `/admin/authentication-error`, and `/admin/access-denied`, and returning only pathname plus search. Any parse failure, control character, or rejected target returns `/admin/users`; fragments are discarded.

- [ ] Start readiness from a zero-delay timer inside `useEffect`, cancel the timer and abort the controller in cleanup, and check `signal.aborted` before setting state. Strict Mode's first setup/cleanup cancels its timer before fetch begins, so the surviving setup makes exactly one request per mounted visit. Initial status is `Checking server`; form submission is always independent of this state.

- [ ] Clear the password after successful submission and never echo password values into an error. Preserve the entered username on failure.

- [ ] Replace `<Admin>` with `<CoreAdmin>` and import `CoreAdmin`, `CustomRoutes`, and `Resource` from `ra-core`. Keep `BrowserRouter`, `/admin/*`, basename `/admin`, and every existing route. Omit `requireAuth`; `AdminRouteGuard` protects every route rendered through `AdminLayout`. Do not use `CustomRoutes noLayout` for any admin feature route.

```tsx
<CoreAdmin
  basename="/admin"
  authProvider={authProvider}
  dataProvider={dataProvider}
  layout={AdminLayout}
  loginPage={LoginPage}
  loading={LoadingPage}
  error={ApplicationError}
  accessDenied={AccessDeniedPage}
  authenticationError={AuthenticationErrorPage}
  catchAll={NotFoundPage}
  disableTelemetry
  title="TJXY Admin"
>
  <Resource name="users" list={UserList} create={UserCreate} edit={UserEdit} show={UserShow} />
  <CustomRoutes>
    <Route path="/access" element={<AccessPage />} />
    <Route path="/tasks" element={<TasksPage />} />
    <Route path="/libraries" element={<LibrariesPage />} />
    <Route path="/storage/google-drive" element={<GoogleDrivePage />} />
    <Route path="/storage/onedrive" element={<OneDrivePage />} />
  </CustomRoutes>
</CoreAdmin>
```

- [ ] Rewrite `AdminLayout` to put `AdminRouteGuard` around `<AdminShell>{children}</AdminShell>` and `<AdminNotifications />`. Add exactly one `<Toast.Provider placement="bottom end" />` in the guarded tree.

- [ ] Wire the `CoreAdmin` slots with their actual meanings: `loading` covers resource configuration pending; `error({ error, errorInfo, resetErrorBoundary })` covers top-level render failures; `catchAll` is a guarded in-shell 404; `authenticationError` is `/authentication-error`; `accessDenied` is `/access-denied`. The route guard owns authentication pending/failure. Resource-controller request errors remain page-local and flow through `AsyncContent`.

- [ ] Run focused and auth regressions:

```bash
npm --prefix admin test -- --run src/api/readiness.test.ts src/auth/loginDestination.test.ts src/auth/LoginPage.test.tsx src/App.test.tsx src/layout src/ui/AdminNotifications.test.tsx src/auth/authProvider.test.ts src/api/httpClient.test.ts
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin run build
```

- [ ] Commit the Login, CoreAdmin wrapper, guarded shell, notifications, and system slots as one production switch:

```bash
git add admin/src/App.tsx admin/src/App.test.tsx admin/src/layout/AdminLayout.tsx admin/src/api/readiness.ts admin/src/api/readiness.test.ts admin/src/auth/loginDestination.ts admin/src/auth/loginDestination.test.ts admin/src/auth/LoginPage.tsx admin/src/auth/LoginPage.test.tsx
git commit -m "feat(admin): switch to HeroUI admin shell"
```

---

### Task 5: Migrate the Users Index and List Metadata

**Files:**

- Modify: `admin/src/api/dataProvider.ts`
- Modify: `admin/src/api/dataProvider.test.ts`
- Rewrite: `admin/src/users/UserList.tsx`
- Rewrite: `admin/src/users/UserList.test.tsx`
- Rewrite: `admin/src/users/UserStatus.tsx`

**Interfaces:**

```ts
export type UserAccessFilter = 'all' | 'administrator' | 'standard' | 'disabled';

export interface UserListFilter {
  q?: string;
  access?: UserAccessFilter;
}

export interface UserListMeta {
  totalUsers: number;
  administrators: number;
  disabled: number;
}
```

Filtering happens after the complete `/Users` response is validated and before pagination. `meta` always summarizes the complete unfiltered validated response. Access groups are exclusive: disabled; enabled administrator; enabled standard user.

- [ ] Extend data-provider tests first. Assert case-insensitive name/ID search, each access filter, search-plus-access composition, unknown filter rejection, summaries computed before filtering/slicing, stable Name/ID sort, and 25-row pagination.

- [ ] Rewrite list component tests first using `ListBase`/`ListContextProvider` from `ra-core`, not module mocks for `react-admin`. Cover initial skeleton, initial error Retry, successful empty, retained-data stale alert, summaries, search/filter reset to page one, named desktop table, mobile labeled records, View/Edit links, and long ID wrapping.

- [ ] Run the red tests:

```bash
npm --prefix admin test -- --run src/api/dataProvider.test.ts src/users/UserList.test.tsx
```

- [ ] Replace `requireEmptyFilter` with strict filter parsing. Normalize search with `trim().toLocaleLowerCase()`. Reject arrays, unknown keys, non-string values, and unsupported access values with the existing safe validation `ApiError`.

- [ ] Return `{ data, total, meta }`, where `total` is the filtered count and `meta` is computed before the filtered array is paginated.

- [ ] Build `UserList` with `ListBase` defaults `{ sort: { field: 'Name', order: 'ASC' }, perPage: 25 }`, `PageHeader`, a local search TextField, an access Select, compact text summaries, `ResponsiveCollection`, HeroUI Table, HeroUI Pagination, and named icon actions.

- [ ] Keep all required mobile fields: Name, ID, Administrator/Standard, Enabled/Disabled, View, Edit. Do not hide a workflow on mobile.

- [ ] Run focused tests plus resource route smoke:

```bash
npm --prefix admin test -- --run src/api/dataProvider.test.ts src/users/UserList.test.tsx
npm --prefix admin run typecheck
npm --prefix admin run lint
```

- [ ] Commit the Users index:

```bash
git add admin/src/api/dataProvider.ts admin/src/api/dataProvider.test.ts admin/src/users/UserList.tsx admin/src/users/UserList.test.tsx admin/src/users/UserStatus.tsx
git commit -m "feat(admin): migrate users index to HeroUI"
```

---

### Task 6: Migrate User Create, Detail, Edit, and Destructive Commands

**Files:**

- Rewrite: `admin/src/users/UserCreate.tsx`
- Create: `admin/src/users/UserCreate.test.tsx`
- Rewrite: `admin/src/users/UserShow.tsx`
- Create: `admin/src/users/UserShow.test.tsx`
- Rewrite: `admin/src/users/UserEdit.tsx`
- Rewrite: `admin/src/users/UserEdit.test.tsx`
- Modify: `admin/src/users/userCommands.ts` only if a presentation import remains
- Modify: `admin/src/users/userCommands.test.ts` only for preserved safe-error contracts

**Interfaces:**

- `UserCreate` uses `CreateBase` and `ra-core` `Form`; successful create redirects to `/users/:id/show`.
- `UserShow` uses `ShowBase`; it renders Name, ID, access role, enabled state, and password configuration.
- `UserEdit` uses `EditBase` for authoritative record state, `useUpdate` for rename, `useDelete` for deletion, and the existing typed `userCommands` functions for password and policy commands.
- Each edit section owns its pending and inline error state. One section's request does not disable unrelated sections.

- [ ] Add create/show tests first for validation, pending state, redirect, skeleton/error/empty record states, one H1, breadcrumbs, and visible read-only fields.

- [ ] Expand edit tests first. Preserve every existing command contract and add initial skeleton, initial load error with Retry, successful reload, failed refresh that retains the current record and draft under a stale-data banner, confirmation focus/target text, mutation-failure draft preservation, password clearing on success, command-local pending, cache refresh after custom commands, browser-deep-link render, and 409 last-administrator deletion behavior.

- [ ] Run the red tests:

```bash
npm --prefix admin test -- --run src/users/UserCreate.test.tsx src/users/UserShow.test.tsx src/users/UserEdit.test.tsx
```

- [ ] Implement visible labels with `Controller` from React Hook Form and HeroUI compound `TextField`. Import and compose `<TextField><Label /><Input /><FieldError /></TextField>`; form-level safe errors render once in an Alert.

- [ ] Compose User Edit as unframed sections in this order: Identity, Access policy, Password, Danger zone. Use semantic H2 headings and spacing/dividers, not Cards inside Cards.

- [ ] Move rename and delete through `ra-core` pessimistic mutation hooks so cache/auth handling is consistent. After a successful custom password or policy command, call `useRefresh` and notify with a safe message that contains no submitted value.

- [ ] Use `ConfirmDialog` for delete. The dialog names the user and explains the effect. On 409, keep the page and current record, close neither draft nor unrelated state, and show `The last enabled administrator cannot be deleted.`

- [ ] In password and policy command catches, await `useLogoutIfAccessDenied`. Render the section's safe error only when it returns `false`; 401 and 403 follow the shared auth routes.

- [ ] Run the complete Users slice:

```bash
npm --prefix admin test -- --run src/users src/api/dataProvider.test.ts src/auth/authProvider.test.ts
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin run build
```

- [ ] Commit User CRUD:

```bash
git add admin/src/users
git commit -m "feat(admin): migrate user management to HeroUI"
```

---

### Task 7: Migrate Access, Devices, and API Keys

**Files:**

- Rewrite: `admin/src/access/AccessPage.tsx`
- Rewrite: `admin/src/access/AccessPage.test.tsx`
- Rewrite: `admin/src/access/DevicesPanel.tsx`
- Rewrite: `admin/src/access/DevicesPanel.test.tsx`
- Rewrite: `admin/src/access/ApiKeysPanel.tsx`
- Rewrite: `admin/src/access/ApiKeysPanel.test.tsx`
- Delete: `admin/src/access/ResponsiveTableCell.tsx`
- Modify: `admin/src/access/useAuthoritativeLoad.ts`
- Create: `admin/src/access/useAuthoritativeLoad.test.tsx`
- Preserve: `admin/src/access/deviceApi.ts`
- Preserve: `admin/src/access/apiKeyApi.ts`

**Interfaces:**

```ts
type AccessTab = 'devices' | 'api-keys';

function parseAccessTab(searchParams: URLSearchParams): AccessTab;
```

Missing or invalid `tab` defaults to `devices`. Selecting API Keys writes `?tab=api-keys`; selecting Devices removes the parameter. Only the active panel is mounted.

`useAuthoritativeLoad` changes its result callback from `(result) => void` to `(result) => void | Promise<void>`. It awaits that callback, keeps abort/sequence checks on both sides of the await, and clears the matching request's loading state in `finally`. This lets panels complete 401/403 handling before deciding whether to publish local error state.

- [ ] Rewrite Access tests first for controlled HeroUI Tabs, URL state, reload restoration, Back/Forward, invalid-tab fallback, correct tab/tabpanel association, and unmount of the inactive panel.

- [ ] Rewrite Devices tests first while retaining all existing authoritative-load, abort, stale-response, rename, clear-name, revoke, safe-error, and pending contracts. Add responsive record assertions and ConfirmDialog focus/close behavior.

- [ ] Rewrite API Keys tests first while retaining mask/show/hide/copy/create/delete/reload contracts. Add checks that plaintext is absent from notifications, URL, `localStorage`, `sessionStorage`, and the remounted DOM; pending create/delete cannot close or submit twice.

- [ ] Add hook tests first for an async `applyResult`, rejected result handlers, stale requests, abort cleanup, and loading reset. Add panel loader tests where 401/403 is handled by `useLogoutIfAccessDenied`: neither error may produce a stale banner/local error/toast, and loading must settle rather than remain stuck.

- [ ] Run the red tests:

```bash
npm --prefix admin test -- --run src/access/useAuthoritativeLoad.test.tsx src/access/AccessPage.test.tsx src/access/DevicesPanel.test.tsx src/access/ApiKeysPanel.test.tsx
```

- [ ] Implement Access with `PageHeader`, HeroUI Tabs, and `useSearchParams`. Give each tab stable `id` and `aria-controls`; render only the selected panel.

- [ ] Implement Devices with `AsyncContent`, `ResponsiveCollection`, HeroUI Table, short rename Modal, and revoke `ConfirmDialog`. During authoritative reload retain existing rows, show local progress, disable stale row commands, and ignore/abort obsolete requests through the existing hook.

- [ ] Implement API Keys with the same collection grammar. Keep the mask literal `****************`. Copy notification text is `API key copied.` and must never include the value. Clear the reveal map before authoritative reload and on unmount.

- [ ] Await every Devices/API Keys load result and command failure through `useLogoutIfAccessDenied` first. A handled auth error must not also become a stale banner, modal error, or toast. Update `useAuthoritativeLoad` to await the panel's result handler and use a sequence-aware `finally` so handled and rejected callbacks cannot leave loading active.

- [ ] Replace `ResponsiveTableCell` with explicit desktop cells and mobile definition-list rows inside `ResponsiveCollection`, then delete the old MUI helper.

- [ ] Run Access and API boundary validation:

```bash
npm --prefix admin test -- --run src/access
npm --prefix admin run typecheck
npm --prefix admin run lint
```

- [ ] Commit Access:

```bash
git add admin/src/access
git commit -m "feat(admin): migrate access management to HeroUI"
```

---

### Task 8: Migrate Scheduled Tasks, Commands, and Durable Jobs

**Files:**

- Rewrite: `admin/src/tasks/TasksPage.tsx`
- Rewrite: `admin/src/tasks/TasksPage.test.tsx`
- Preserve: `admin/src/tasks/taskApi.ts`
- Preserve: `admin/src/tasks/taskApi.test.ts`

**Interfaces:**

- Poll every 5 seconds while mounted.
- Keep valid tasks/jobs visible during poll and manual reload.
- Confirm cancellation only for an active scheduled task.
- Root commands remain validate, discover, and full scan. UUID item commands remain resolve, expand, index, and probe.
- Successful commands refresh authoritative state and report the durable job count returned by the current API.
- Status tones are exhaustive: scheduled `Idle=neutral`, scheduled/job `Running=accent`, job `Pending=neutral`, `Retrying=warning`, `Completed=success`, `Cancelled=neutral`, and `Failed=danger`.

- [ ] Rewrite tests first for initial skeleton, successful empty, initial error Retry, stale poll failure with retained data, timer cleanup, manual reload, task start, confirmed cancel, cancel failure, every manual command, per-command pending isolation, durable job result text, and visible text plus the exact tone for all Idle/Running/Pending/Retrying/Completed/Cancelled/Failed states.

- [ ] Run the red tests:

```bash
npm --prefix admin test -- --run src/tasks/TasksPage.test.tsx src/tasks/taskApi.test.ts
```

- [ ] Build one PageHeader with Reload, an action-row scheduled-task collection, two unframed manual-command sections, and an internally scrollable recent-jobs table. Translate raw enum values into readable labels while retaining raw identifiers in secondary text.

- [ ] Use `StatusChip` with the exhaustive tone map above and visible text for every state. Button pending state must preserve width.

- [ ] Put `ConfirmDialog` around Cancel. Start remains a direct reversible command. Leave the task row visible while either command is pending.

- [ ] Keep polling in one effect with explicit interval cleanup and AbortController cleanup. An obsolete response must not replace a newer manual reload result.

- [ ] Route initial load, poll, reload, and command failures through `useLogoutIfAccessDenied` before applying page-local error state. Ignore abort errors caused by cleanup, but do not swallow network or server failures.

- [ ] Run the task slice and shared async tests:

```bash
npm --prefix admin test -- --run src/tasks src/ui/AsyncContent.test.tsx src/ui/ConfirmDialog.test.tsx
npm --prefix admin run typecheck
npm --prefix admin run lint
```

- [ ] Commit Tasks:

```bash
git add admin/src/tasks/TasksPage.tsx admin/src/tasks/TasksPage.test.tsx
git commit -m "feat(admin): migrate task operations to HeroUI"
```

---

### Task 9: Split Libraries into an Index and Durable Edit Route

**Files:**

- Modify: `admin/src/App.tsx`
- Rewrite: `admin/src/libraries/LibrariesPage.tsx`
- Rewrite: `admin/src/libraries/LibrariesPage.test.tsx`
- Create: `admin/src/libraries/LibraryCreateDialog.tsx`
- Create: `admin/src/libraries/LibraryCreateDialog.test.tsx`
- Create: `admin/src/libraries/LibraryEditPage.tsx`
- Create: `admin/src/libraries/LibraryEditPage.test.tsx`
- Create: `admin/src/libraries/LibraryPolicyForm.tsx`
- Create: `admin/src/libraries/HybridCandidatesPanel.tsx`
- Create: `admin/src/libraries/HybridCandidatesPanel.test.tsx`
- Delete: `admin/src/libraries/HybridCandidatesDialog.tsx`
- Delete: `admin/src/libraries/HybridCandidatesDialog.test.tsx`
- Preserve: `admin/src/libraries/libraryApi.ts`
- Preserve: `admin/src/libraries/hybridCandidateApi.ts`

**Interfaces:**

- Register `<Route path="/libraries/:id" element={<LibraryEditPage />} />` after `/libraries` inside the same `CustomRoutes` tree. It inherits `AdminRouteGuard` and the shell through `AdminLayout`.
- Load a single library from the existing validated `listLibraries` response and match by route ID. Do not invent an endpoint.
- Create defaults remain `kind=mixed`, `enabled=true`, `profile=Lazy`.
- Profile save always sends the loaded `profileVersion`. Non-advanced profiles omit override fields.
- A 409 keeps every local policy field, shows an inline conflict Alert, and exposes `Reload latest`; it never automatically replaces the draft.

- [ ] Rewrite list tests first for skeleton/error/empty/stale states, create defaults, readable policy/status labels, desktop table, mobile records, edit deep links, and reload. Deletion moves to the durable edit page's Danger zone.

- [ ] Add edit tests first for direct deep-link initial skeleton, initial load error with Retry, successful reload, refresh failure with retained library and draft, Back breadcrumb, not found, rename, enabled/profile policy, advanced overrides, non-advanced omission, version propagation, 409 draft preservation, explicit reload-latest, command-local pending, and confirmed delete redirect.

- [ ] Replace candidate dialog tests with panel tests. Preserve enabled/background/UUID gating, 50-row paging, pin/unpin, old-pin removal, stale-response rejection, and no accidental page overflow.

- [ ] Run the red tests:

```bash
npm --prefix admin test -- --run src/libraries/LibrariesPage.test.tsx src/libraries/LibraryCreateDialog.test.tsx src/libraries/LibraryEditPage.test.tsx src/libraries/HybridCandidatesPanel.test.tsx
```

- [ ] Keep `LibrariesPage` focused on collection, create, and navigation. Use a short HeroUI create Modal and responsive labeled records below 640px; do not duplicate the destructive action in the list.

- [ ] Build `LibraryEditPage` in this order: Identity, Scanning policy, Advanced overrides when enabled by the selected profile, Background candidates when eligible, Danger zone. Preserve the draft in component/form state until a successful save or explicit reload. The Danger zone uses `ConfirmDialog` and redirects to `/admin/libraries` only after confirmed deletion succeeds.

- [ ] Render candidates inline as `HybridCandidatesPanel`; do not put the full workflow into another modal. Candidate unpin is a subordinate direct danger action and reports completion through safe notification text.

- [ ] Route list, edit, delete, candidate load, pin, and unpin failures through `useLogoutIfAccessDenied` before applying their local error/conflict handling. A 409 CAS conflict remains local and preserves the policy draft.

- [ ] Add the new route without changing existing library or storage URLs. Test direct refresh and browser Back through the router integration.

- [ ] Run the complete library slice and build:

```bash
npm --prefix admin test -- --run src/libraries
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin run build
```

- [ ] Commit Libraries:

```bash
git add admin/src/App.tsx admin/src/libraries
git commit -m "feat(admin): add durable library management"
```

---

### Task 10: Integrate Cloud Pagination and Migrate the Shared Storage Workflow

**Files:**

- Review integrated prerequisite changes touching: `crates/server/src/storage_admin.rs`, `crates/server/src/storage_admin_cursor.rs`, `crates/server/tests/storage_admin_routes.rs`, `admin/src/storage/googleDriveApi.ts`, `admin/src/storage/directoryChoices.ts`, `admin/src/storage/GoogleDrivePage.tsx`, and `admin/src/storage/OneDrivePage.tsx`
- Create: `admin/src/storage/StorageWorkflow.tsx`
- Create: `admin/src/storage/StorageWorkflow.test.tsx`
- Create: `admin/src/storage/FolderBrowser.tsx`
- Create: `admin/src/storage/FolderBrowser.test.tsx`
- Rewrite: `admin/src/storage/GoogleDrivePage.tsx`
- Rewrite: `admin/src/storage/GoogleDrivePage.test.tsx`
- Rewrite: `admin/src/storage/OneDrivePage.tsx`
- Rewrite: `admin/src/storage/OneDrivePage.test.tsx`
- Preserve: `admin/src/storage/googleDriveApi.ts`
- Preserve: `admin/src/storage/directoryChoices.ts`

**Prerequisite Recheck:**

The Cloud Pagination Gate must already have been resolved before Task 1. Do not recreate, cherry-pick only frontend fragments, or overwrite its helpers.

- [ ] Before editing Storage, verify again that the accepted pagination commit is present in this branch. When integration preserved commit identity, run:

```bash
git merge-base --is-ancestor 509cbb3 HEAD
```

Expected exit code: `0`. For an owner-approved rewritten integration, verify the recorded equivalent commit and rerun every pagination contract below instead of relying on ancestry alone.

- [ ] With a clean HeroUI worktree, rerun the pagination contracts:

```bash
git status --short
cargo test -p tjxy-server storage_admin_cursor --locked
cargo test -p tjxy-server --test storage_admin_routes --locked
npm --prefix admin test -- --run src/storage/googleDriveApi.test.ts src/storage/directoryChoices.test.ts src/storage/GoogleDrivePage.test.tsx src/storage/OneDrivePage.test.tsx
```

**Interfaces:**

```ts
export type StoragePhase = 'authorize' | 'choose-folder' | 'review' | 'complete';

export interface StorageWorkflowProps {
  title: string;
  providerName: 'Google Drive' | 'OneDrive';
  phase: StoragePhase;
  isBusy: boolean;
  onRestart: () => void;
  children: React.ReactNode;
}

export interface FolderChoice {
  id: string;
  name: string;
}

export interface FolderBrowserProps {
  ariaLabel: string;
  path: readonly FolderChoice[];
  directories: readonly FolderChoice[];
  isLoading: boolean;
  error: unknown | null;
  hasMore: boolean;
  isLoadingMore: boolean;
  isDisabled: boolean;
  onNavigate: (pathIndex: number) => void;
  onOpen: (folder: FolderChoice) => void;
  onLoadMore: () => void;
  onRetry: () => void;
}
```

`StorageWorkflow` owns only the visual phase frame and restart command. Provider pages retain OAuth, drive selection, pagination tokens, path, binding, and API calls so provider differences remain explicit.

- [ ] Add shared workflow tests first for phase labels, current-step semantics, Restart authorization visibility after OAuth begins, pending restart prevention, responsive step layout, and child preservation.

- [ ] Rewrite Google tests first while preserving the OAuth popup name and opener/referrer isolation, an inline popup-blocked error with Retry/Restart, retry-before-callback, My Drive/Shared Drive switch, paginated Shared Drive list, paginated folder append/dedupe, breadcrumb resets, disabled target libraries, bind result, initial job, restart-required status, and stale request rejection. Reserve a same-origin blank popup during the user gesture, detach `opener`, and navigate it under a `no-referrer` policy; a direct `noopener` feature returns `null` on success and cannot distinguish a blocked popup.

- [ ] Rewrite OneDrive tests first with the parallel contract: popup-blocked error, OAuth retry, paginated folder browse, breadcrumb resets, bind result, explicit loading indicator, restart, and stale request rejection.

- [ ] Run the red UI tests after replacing MUI assertions but before implementing HeroUI views:

```bash
npm --prefix admin test -- --run src/storage/StorageWorkflow.test.tsx src/storage/FolderBrowser.test.tsx src/storage/GoogleDrivePage.test.tsx src/storage/OneDrivePage.test.tsx
```

- [ ] Implement the three phases: Authorize, Choose folder, Review. Keep Complete as a result state after submission. Use a segmented step indicator, not clickable tabs; users advance through validated commands.

- [ ] Implement `FolderBrowser` as an unframed tool with breadcrumbs, folder list, loading/empty/error states, and an internal Load more action. Long breadcrumbs scroll within their own line. The current folder is explicit even when it has no children.

- [ ] Implement `onRestart` in each provider page to abort in-flight requests and clear OAuth session state, authorization state, provider/drive choice, path, directory choices, page token, review fields derived from the old target, and binding result. It must not clear unrelated authentication/session storage.

- [ ] Keep enabled-library selection in Authorize and lock it only while an OAuth attempt is active. Restart returns the user to an editable target selector.

- [ ] Route library loads, OAuth verification, provider browsing, pagination, and binding failures through `useLogoutIfAccessDenied` before provider-local handling. Cleanup aborts are silent; all other unhandled failures remain visible and retryable.

- [ ] Run frontend and backend storage validation:

```bash
npm --prefix admin test -- --run src/storage
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin run build
cargo test -p tjxy-server storage_admin_cursor --locked
cargo test -p tjxy-server --test storage_admin_routes --locked
```

- [ ] Commit only the HeroUI storage changes after the prerequisite merge commit is already recorded:

```bash
git add admin/src/storage
git commit -m "feat(admin): unify cloud storage workflows"
```

---

### Task 11: Remove the Material UI Presentation Stack

**Files:**

- Modify: `admin/package.json`
- Modify: `admin/package-lock.json`
- Modify: `admin/vite.config.ts`
- Delete: `admin/src/theme.ts`
- Create: `admin/src/test/dependencyBoundary.test.ts`

**Interfaces:**

The final runtime dependency graph contains `ra-core` but not `react-admin`, `ra-ui-materialui`, MUI, MUI icons, or Emotion. Vite chunk groups are `react`, `ra-core`, and `heroui`; no group is named MUI.

- [ ] Add a dependency-boundary test before uninstalling. It reads `package.json` and production source text and fails when forbidden packages remain:

```ts
const forbiddenNames = ['react-admin', 'ra-ui-materialui'];
const forbiddenScopes = ['@mui/', '@emotion/'];
```

Create `admin/src/test/dependencyBoundary.test.ts` and assert forbidden package names/scopes are absent from direct dependencies, every `package-lock.json.packages` key, and production `src` imports after cleanup. Exclude `src/test/**` and `**/*.test.{ts,tsx}` from the source walk so the test's own package-name fixture cannot trigger itself.

- [ ] Run it red:

```bash
npm --prefix admin test -- --run src/test/dependencyBoundary.test.ts
```

- [ ] Scan all remaining coupling before editing:

```bash
rg -n "from ['\"](?:react-admin|@mui|@emotion)|@mui/|@emotion/" admin/src admin/package.json admin/vite.config.ts --glob '!**/*.test.ts' --glob '!**/*.test.tsx' --glob '!test/**'
```

- [ ] Require the production-source scan to be empty before uninstalling. If it reports a file, return to that file's owning migration task and complete its specified HeroUI conversion; Task 11 does not add an unplanned compatibility shim. Delete `theme.ts` after its import scan is empty.

- [ ] Uninstall the presentation packages:

```bash
npm --prefix admin uninstall react-admin @mui/material @mui/icons-material @emotion/react @emotion/styled
```

- [ ] Update Vite code splitting:

```ts
groups: [
  { name: 'heroui', test: /node_modules[\\/](@heroui|react-aria|@react-aria|framer-motion|tailwind-variants)[\\/]/ },
  { name: 'ra-core', test: /node_modules[\\/](ra-core|@tanstack|react-hook-form)[\\/]/ },
  { name: 'react', test: /node_modules[\\/](react|react-dom|react-router)[\\/]/ },
],
```

- [ ] Run the boundary test, zero-result scans, dependency tree, and full frontend checks:

```bash
npm --prefix admin test -- --run src/test/dependencyBoundary.test.ts
! rg -n "from ['\"](?:react-admin|@mui|@emotion)|@mui/|@emotion/" admin/src admin/package.json admin/vite.config.ts --glob '!**/*.test.ts' --glob '!**/*.test.tsx' --glob '!test/**'
! rg -n '"node_modules/(react-admin|ra-ui-materialui|@mui/|@emotion/)' admin/package-lock.json
! npm --prefix admin ls react-admin ra-ui-materialui @mui/material @mui/icons-material @emotion/react @emotion/styled --all
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build
```

The lockfile scan and all-depth `npm ls` command must both report no forbidden package. Verify the production bundle also contains no `ra-ui-materialui`, MUI, or Emotion asset names.

- [ ] Commit dependency cleanup:

```bash
git add admin/package.json admin/package-lock.json admin/vite.config.ts admin/src
git commit -m "refactor(admin): remove Material UI presentation stack"
```

---

### Task 12: Add Visual, Accessibility, Responsive, and Secret-Safety E2E Coverage

**Files:**

- Modify: `admin/playwright.config.ts`
- Modify: `admin/e2e/support.ts`
- Modify: `admin/e2e/users.spec.ts`
- Modify: `admin/e2e/access.spec.ts`
- Create: `admin/e2e/adminFixtures.ts`
- Create: `admin/e2e/login.spec.ts`
- Create: `admin/e2e/visual.spec.ts`
- Create: `admin/e2e/accessibility.spec.ts`
- Create: `admin/e2e/secret-safety.spec.ts`
- Create: `admin/e2e/webkit-smoke.spec.ts`
- Generate: `admin/e2e/visual.spec.ts-snapshots/*.png`

**Interfaces:**

- Deterministic visual fixtures intercept frontend API requests with fixed IDs, names, states, and timestamps. They never include a real access token or real API key plaintext.
- Required visual viewports are `1440x900`, `768x1024`, and `390x844`.
- Chromium runs the complete lifecycle, visual, and accessibility suites. WebKit runs login, navigation, one Users workflow, one confirmation modal, and logout.
- Set Playwright's automatic `trace`, `screenshot`, and `video` modes to `off` for every project. Lifecycle tests submit passwords and receive access tokens, so retry capture is forbidden. Only `visual.spec.ts` calls `toHaveScreenshot`, and it uses intercepted non-secret fixtures without submitting a password or revealing an API key.

- [ ] Change `playwright.config.ts` to `trace: 'off'`, `screenshot: 'off'`, and `video: 'off'` before running any new login or lifecycle test. Add Chromium and focused WebKit projects without overriding those security defaults.

- [ ] Extend test support with reusable assertions for no document overflow, no visible action intersection, unique H1, safe console messages, and focus restoration. Remove the old free-form screenshot helper after all intended captures move to `toHaveScreenshot`. Unit-test pure geometry helpers if introduced.

- [ ] Migrate `users.spec.ts` from `menuitem` assumptions to named sidebar/drawer links. Change the library policy lifecycle from the old edit modal to `/admin/libraries/:id`, assert refresh and Back on the durable route, and preserve wrong-password, direct deep-link return, non-admin rejection, Users CRUD, last-administrator conflict, mobile Users, and logout coverage.

- [ ] Migrate `access.spec.ts` to `/admin/access?tab=api-keys`, named HeroUI tabs, and HeroUI modal/confirmation roles. Preserve device rename/revoke and API key create/mask/reveal/copy/delete lifecycle assertions without putting plaintext in a failure message.

- [ ] Add `login.spec.ts` for direct-login fallback, deep-link restoration including search, 401 session clear, 403 session preservation/Access Denied, readiness unavailable without submit blocking, and explicit Sign out from Access Denied. This file inherits disabled trace/screenshot/video capture.

- [ ] Add deterministic fixture tests for all route interceptors before screenshots. A missing or unexpected request must fail the test rather than fall through to live data. At `1440x900`, `768x1024`, and `390x844`, assert `ResponsiveCollection` exposes only the intended representation in the computed accessibility tree.

- [ ] Add Login and shell/Users `toHaveScreenshot` baselines at all three viewports. Use only empty fields and intercepted readiness/auth fixtures; include a long user name, long ID, and stale-data banner.

- [ ] Add Access Devices, masked API Keys, and Tasks baselines at all three viewports. Cover successful empty Devices and all task/job status tones without ever revealing key plaintext.

- [ ] Add Libraries index and Library Edit baselines at all three viewports. Cover initial skeleton, initial error, advanced policy fields, CAS conflict with preserved draft, and the delete confirmation focused on Cancel.

- [ ] Add Google Drive and OneDrive baselines at all three viewports. Cover a long breadcrumb, loading more, successful bind result, and popup-blocked inline recovery. OAuth windows are intercepted and contain no credential.

- [ ] Add representative mobile Drawer, initial-error Retry, stale-data Alert, and short confirmation baselines. Each state must also pass the overflow/intersection helpers.

- [ ] Add axe checks on login, Users, User Edit, Access, Tasks, Library Edit, and both Storage routes:

```ts
const results = await new AxeBuilder({ page })
  .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
  .analyze();
expect(results.violations).toEqual([]);
```

- [ ] Add keyboard tests for skip link, sidebar/drawer navigation, tab order, password reveal, Tabs, every modal initial focus/trap/Escape/focus return, pending-close prevention, and account logout. Add 200% browser zoom and reduced-motion smoke checks.

- [ ] Add `secret-safety.spec.ts`. Confirm the project-level trace/screenshot/video settings are off, then test API key reveal against live DOM, URL, localStorage, sessionStorage, and captured console messages; switch tabs/reload/unmount and assert the secret is absent. Add a password submission check that neither safe UI errors nor console messages contain the password. Do not call screenshot or attachment APIs in this file.

- [ ] Configure WebKit and install its browser if the local Playwright cache does not already contain it:

```bash
npm --prefix admin exec playwright install webkit
```

- [ ] Run focused E2E while developing:

```bash
npm --prefix admin run e2e -- login.spec.ts --project=chromium
npm --prefix admin run e2e -- visual.spec.ts --project=chromium
npm --prefix admin run e2e -- accessibility.spec.ts --project=chromium
npm --prefix admin run e2e -- secret-safety.spec.ts --project=chromium
npm --prefix admin run e2e -- webkit-smoke.spec.ts --project=webkit
```

- [ ] Review every generated baseline image manually at original resolution. Reject clipped text, document overflow, duplicate headers, overlap, inconsistent control heights, accidental card nesting, and screenshot content containing a secret.

- [ ] Run the complete E2E suite:

```bash
npm --prefix admin run e2e
```

- [ ] Commit E2E and baselines:

```bash
git add admin/playwright.config.ts admin/e2e
git commit -m "test(admin): cover HeroUI visual and accessibility contracts"
```

---

### Task 13: Update Documentation and Perform Final Quality Verification

**Files:**

- Modify: `README.md`
- Modify: `docs/api-parity.md`
- Modify: `docs/superpowers/specs/2026-07-29-admin-heroui-rebuild-design.md` (status only if implementation is complete)
- Review: all files changed from `main...HEAD`

**Documentation Contract:**

- README describes HeroUI v3, Tailwind v4, headless `ra-core`, the `/admin/` dev/build commands, and the supported route map including `/admin/libraries/:id` and `?tab=api-keys`.
- API parity continues to describe unchanged backend coverage; note that the presentation migration added no server endpoint.
- Record the three viewport matrix, Chromium/WebKit scope, visual baseline update command, and the rule that secret-reveal tests never capture traces or screenshots.

- [ ] Update documentation close to the existing Admin build and route sections. Do not add a new long-form architecture document; link the approved design instead.

- [ ] Run formatting-independent patch checks and review the complete diff:

```bash
git diff --check
git diff --check main...HEAD
git diff --stat main...HEAD
git diff --name-status main...HEAD
```

- [ ] Inspect every added production file for unfinished branches or inert UI, then run the forbidden-stack scan:

```bash
! rg -n "from ['\"](?:react-admin|@mui|@emotion)|@mui/|@emotion/|ra-ui-materialui" admin/src admin/package.json admin/vite.config.ts --glob '!**/*.test.ts' --glob '!**/*.test.tsx' --glob '!test/**'
! rg -n '"node_modules/(react-admin|ra-ui-materialui|@mui/|@emotion/)' admin/package-lock.json
! npm --prefix admin ls react-admin ra-ui-materialui @mui/material @mui/icons-material @emotion/react @emotion/styled --all
```

- [ ] Reinstall from lockfile and run the full clean frontend matrix:

```bash
npm --prefix admin ci
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build
npm --prefix admin run e2e
```

- [ ] Inspect the build output. Record raw and gzip sizes for React, `ra-core`, HeroUI, and application chunks. Confirm there is no MUI/Emotion/React Admin UI chunk and investigate any single unexpected application chunk above 500 kB raw.

- [ ] Run a production browser QA pass at `1440x900`, `768x1024`, and `390x844`. Verify every route, drawer/sidebar transition, Back/Forward, refresh/deep link, long text, 200% zoom, reduced motion, loading/empty/error/stale state, destructive dialog, and cloud restart workflow.

- [ ] Perform a final security/lifecycle review:

  - every interval and request has cleanup;
  - stale responses cannot overwrite authoritative data;
  - errors remain explicit and safe;
  - drafts survive failures;
  - no secret reaches storage, URL, toast, log, trace, screenshot, or error text;
  - no optimistic destructive mutation can leave the UI ahead of server state;
  - no duplicated network polling was introduced by Strict Mode.

- [ ] Use `superpowers:requesting-code-review` for an independent diff review, apply only validated findings, then rerun the affected focused tests.

- [ ] Use `superpowers:verification-before-completion` and preserve the exact successful command output in the task handoff.

- [ ] Mark the design implementation status complete only after every acceptance criterion and command above passes, then commit documentation:

```bash
git add README.md docs/api-parity.md docs/superpowers/specs/2026-07-29-admin-heroui-rebuild-design.md
git commit -m "docs: document HeroUI admin architecture"
```

## Completion Definition

The work is complete only when every checkbox is resolved, existing production lifecycle workflows pass on the same backend contracts, the new library deep link and Access query state survive reload/navigation, all supported viewports are visually reviewed, accessibility and focus checks pass, API key plaintext is absent from every prohibited surface, and the final dependency and bundle scans prove that Material UI, Emotion, and the React Admin Material UI package are gone.
