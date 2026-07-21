# React Admin Users Vertical Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a production-served `/admin/` React Admin application that authenticates TJXY administrators and completes the local-user management workflow end to end.

**Architecture:** A strict TypeScript Vite application adapts TJXY's PascalCase command API through focused HTTP, auth, resource-provider, and user-command modules. The Rust server validates and serves the built SPA only below `/admin`, with a scoped `index.html` fallback that cannot change API 404 behavior.

**Tech Stack:** React 19.2.7, React Admin 5.15.1, MUI 9.2.0, Vite 8.1.5, TypeScript 6.0.3, Vitest 4.1.10, Testing Library, Playwright 1.61.1, Axum 0.8.9, tower-http 0.7.0, Rust 1.88.0.

## Global Constraints

- Keep the SPA same-origin at `/admin/`; do not add CORS.
- Store the access token only in `sessionStorage`; never log credentials or authorization headers.
- Use canonical `Authorization: MediaBrowser ...` headers, not query-string tokens.
- Map backend `Id` to React Admin `id` without discarding PascalCase fields.
- Keep rename, password, policy, and delete as separate pessimistic commands.
- Scope static fallback to GET/HEAD paths below `/admin/`; unknown API routes remain 404.
- The production binary reads `TJXY_ADMIN_DIST_DIR`, defaulting to `admin/dist`, and fails explicitly when `index.html` is missing.
- Do not add placeholder navigation for unimplemented `PLAN.md` section 16 pages.
- Use exact dependency versions and commit `admin/package-lock.json`.
- Preserve the existing API-only `build_router` for focused Rust tests.

---

## File Map

### Frontend project and shared boundaries

- Create `admin/package.json`: exact dependencies and verification scripts.
- Create `admin/package-lock.json`: npm-resolved dependency lock.
- Create `admin/index.html`: Vite entry document.
- Create `admin/tsconfig.json`, `admin/tsconfig.app.json`, `admin/tsconfig.node.json`: strict browser and tool TypeScript projects.
- Create `admin/vite.config.ts`: `/admin/` base and exact development proxies.
- Create `admin/vitest.config.ts`, `admin/src/test/setup.ts`: deterministic jsdom tests.
- Create `admin/eslint.config.js`: type-aware React/TypeScript lint rules.
- Create `admin/src/main.tsx`: browser entrypoint.
- Create `admin/src/api/types.ts`: TJXY DTO and command payload contracts.
- Create `admin/src/api/httpClient.ts`: canonical headers, response parsing, and `ApiError`.
- Create `admin/src/auth/session.ts`: token and browser device identity lifecycle used by the HTTP boundary.
- Create `admin/src/auth/authProvider.ts`: React Admin authentication contract.

### Users resource and UI

- Create `admin/src/api/dataProvider.ts`: Users-only React Admin provider.
- Create `admin/src/users/userCommands.ts`: password and policy command functions.
- Create `admin/src/App.tsx`: `/admin` React Admin composition root.
- Create `admin/src/theme.ts`: restrained operational theme with stable controls.
- Create `admin/src/layout/AdminLayout.tsx`: product header and Users-only navigation.
- Create `admin/src/auth/LoginPage.tsx`: accessible administrator login.
- Create `admin/src/users/UserList.tsx`: responsive user list.
- Create `admin/src/users/UserCreate.tsx`: create command form.
- Create `admin/src/users/UserShow.tsx`: read-only user details.
- Create `admin/src/users/UserEdit.tsx`: separate rename/password/policy/delete panels.
- Create colocated `*.test.ts` and `*.test.tsx` files for each behavior boundary.

### Rust static serving and release integration

- Create `crates/server/src/admin_assets.rs`: validated admin distribution and scoped service.
- Modify `crates/server/src/lib.rs`: export `build_router_with_admin_dist` without changing `build_router`.
- Modify `crates/server/src/main.rs`: parse `TJXY_ADMIN_DIST_DIR` and require admin assets.
- Modify `crates/server/Cargo.toml`, `Cargo.lock`: tower-http filesystem service.
- Create `crates/server/tests/admin_assets.rs`: static root, deep link, content type, method, and API 404 contracts.
- Modify `.gitignore`: Node/build/browser artifacts.
- Modify `.github/workflows/ci.yml`: frontend install, checks, build, and browser gate.
- Modify `README.md` and `docs/api-parity.md`: commands, deployment, scope, and limitations.
- Create `admin/playwright.config.ts`, `admin/e2e/users.spec.ts`, `admin/scripts/run-e2e-server.sh`: production-build browser workflow.

---

### Task 1: Frontend Toolchain And Typed HTTP Boundary

**Files:**
- Create: `admin/package.json`
- Create: `admin/package-lock.json`
- Create: `admin/index.html`
- Create: `admin/tsconfig.json`
- Create: `admin/tsconfig.app.json`
- Create: `admin/tsconfig.node.json`
- Create: `admin/vite.config.ts`
- Create: `admin/vitest.config.ts`
- Create: `admin/eslint.config.js`
- Create: `admin/src/test/setup.ts`
- Create: `admin/src/api/types.ts`
- Create: `admin/src/api/httpClient.ts`
- Create: `admin/src/auth/session.ts`
- Test: `admin/src/api/httpClient.test.ts`
- Modify: `.gitignore`

**Interfaces:**
- Produces: `ApiError`, `apiRequest<T>()`, `mediaBrowserIdentityHeader()`, `UserRecord`, `AuthenticationResult`, `UserPolicy`, `getAccessToken()`, `clearSession()`, `getDeviceId()`.
- Consumes: existing `/Users` JSON and MediaBrowser authentication contract.

Use this exact request interface:

```ts
export type RequestAuth = 'none' | 'identity' | 'token';
export interface ApiRequestOptions extends RequestInit { auth?: RequestAuth }
export async function apiRequest<T = undefined>(
  path: string,
  options: ApiRequestOptions = {},
): Promise<T>;
```

`token` is the default. Login passes `identity`; public requests, if later needed, pass
`none`. Only relative paths beginning with `/` are accepted.

- [ ] **Step 1: Add the exact project manifest and strict tool configuration**

Use this dependency set in `admin/package.json`:

```json
{
  "name": "tjxy-admin",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "engines": { "node": ">=22.12.0" },
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "typecheck": "tsc -b --pretty false",
    "lint": "eslint . --max-warnings 0",
    "test": "vitest",
    "e2e": "playwright test"
  },
  "dependencies": {
    "@emotion/react": "11.14.0",
    "@emotion/styled": "11.14.1",
    "@mui/icons-material": "9.2.0",
    "@mui/material": "9.2.0",
    "react": "19.2.7",
    "react-admin": "5.15.1",
    "react-dom": "19.2.7"
  },
  "devDependencies": {
    "@playwright/test": "1.61.1",
    "@testing-library/jest-dom": "7.0.0",
    "@testing-library/react": "16.3.2",
    "@testing-library/user-event": "14.6.1",
    "@types/node": "22.20.1",
    "@types/react": "19.2.17",
    "@types/react-dom": "19.2.3",
    "@vitejs/plugin-react": "6.0.3",
    "eslint": "10.7.0",
    "eslint-plugin-react-hooks": "7.1.1",
    "eslint-plugin-react-refresh": "0.5.3",
    "jsdom": "29.1.1",
    "typescript": "6.0.3",
    "typescript-eslint": "8.65.0",
    "vite": "8.1.5",
    "vitest": "4.1.10"
  }
}
```

Configure Vite with `base: "/admin/"`, `build.outDir: "dist"`, and development proxies for `/Users`, `/System`, and `/health` to `TJXY_DEV_SERVER ?? "http://127.0.0.1:8096"`. Do not rewrite paths or change origin. Configure Vitest with jsdom, restored mocks, and `src/test/setup.ts` importing `@testing-library/jest-dom/vitest`.

- [ ] **Step 2: Install dependencies and commit the generated lock**

Run:

```bash
npm --prefix admin install --package-lock-only
npm --prefix admin ci
```

Expected: `admin/package-lock.json` is generated; npm reports zero install errors.

- [ ] **Step 3: Write failing HTTP boundary tests**

Cover these exact cases in `httpClient.test.ts` using a mocked `global.fetch`:

```ts
it('sends canonical identity and token headers without query credentials', async () => {
  sessionStorage.setItem('tjxy.admin.token', 'secret-token');
  fetchMock.mockResolvedValue(new Response(JSON.stringify({ Id: 'u1' }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  }));
  await apiRequest('/Users/Me');
  const request = fetchMock.mock.calls[0][0] as Request;
  expect(request.url).not.toContain('secret-token');
  expect(request.headers.get('Authorization')).toBe('MediaBrowser Token="secret-token"');
});

it.each([204, 205])('returns undefined for an empty %s response', async status => {
  fetchMock.mockResolvedValue(new Response(null, { status }));
  await expect(apiRequest('/Users/u1/Policy', { method: 'POST' })).resolves.toBeUndefined();
});

it('throws a typed conflict without echoing an opaque response body', async () => {
  fetchMock.mockResolvedValue(new Response('database detail', { status: 409 }));
  await expect(apiRequest('/Users/u1', { method: 'DELETE' })).rejects.toMatchObject({
    name: 'ApiError', status: 409, category: 'conflict',
  });
});
```

Also cover network failure, malformed JSON on a successful JSON response, 400, 401, 403, 404, and 503 category mapping.

- [ ] **Step 4: Run the HTTP tests to verify red**

Run: `npm --prefix admin test -- --run src/api/httpClient.test.ts`

Expected: FAIL because `apiRequest` and `ApiError` do not exist.

- [ ] **Step 5: Implement DTOs and HTTP behavior**

Define `UserRecord` with both `Id: string` and `id: string`, exact `Name`, `ServerId`, `HasPassword`, `HasConfiguredPassword`, and the complete current `Policy`. `apiRequest<T>` must build a `Request`, add JSON content type only when a body exists, use identity header for login and token header otherwise, accept only JSON for nonempty success bodies, and throw:

```ts
export type ApiErrorCategory =
  | 'network' | 'invalid-response' | 'validation' | 'authentication'
  | 'authorization' | 'not-found' | 'conflict' | 'unavailable' | 'unexpected';

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly category: ApiErrorCategory,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}
```

Use fixed, nonsecret client values `Client="TJXY Admin"`, `Device="Browser"`, `Version="0.1.0"`. Implement `session.ts` in this task with `tjxy.admin.token` and `tjxy.admin.deviceId`; generate the device ID with `crypto.randomUUID()` once per tab. Never include response body text in `ApiError.message`.

- [ ] **Step 6: Verify the shared frontend boundary**

Run:

```bash
npm --prefix admin test -- --run src/api/httpClient.test.ts
npm --prefix admin run typecheck
npm --prefix admin run lint
```

Expected: all commands exit 0.

- [ ] **Step 7: Commit Task 1**

```bash
git add .gitignore admin/package.json admin/package-lock.json admin/index.html admin/tsconfig*.json admin/vite.config.ts admin/vitest.config.ts admin/eslint.config.js admin/src/test admin/src/api admin/src/auth/session.ts
git commit -m "feat(admin): add typed HTTP foundation"
```

---

### Task 2: Administrator Authentication Provider

**Files:**
- Create: `admin/src/auth/authProvider.ts`
- Test: `admin/src/auth/authProvider.test.ts`

**Interfaces:**
- Consumes: `apiRequest<T>()`, `AuthenticationResult`, `UserRecord` from Task 1.
- Produces: `authProvider: AuthProvider`.

- [ ] **Step 1: Write failing authentication tests**

Test login with `Username` and `Pw`, then require `/Users/Me` to return `Policy.IsAdministrator === true` and `Policy.IsDisabled === false`. Assert that the token is not retained on failed verification. Cover reload validation, `401` cleanup, `403` preservation, identity mapping, permissions, and logout semantics:

```ts
await authProvider.login({ username: 'Admin', password: 'correct horse' });
expect(sessionStorage.getItem('tjxy.admin.token')).toBe('issued-token');
await expect(authProvider.getIdentity?.()).resolves.toEqual({
  id: 'admin-id', fullName: 'Admin', avatar: undefined,
});

await expect(authProvider.login({ username: 'Bob', password: 'pw' }))
  .rejects.toMatchObject({ status: 403 });
expect(sessionStorage.getItem('tjxy.admin.token')).toBeNull();
```

- [ ] **Step 2: Run the auth tests to verify red**

Run: `npm --prefix admin test -- --run src/auth/authProvider.test.ts`

Expected: FAIL because the provider is missing.

- [ ] **Step 3: Implement session and auth provider**

The Task 1 session module uses only these keys:

```ts
const TOKEN_KEY = 'tjxy.admin.token';
const DEVICE_KEY = 'tjxy.admin.deviceId';
```

`authProvider.login` sends the identity header, stores the token only long enough to call `/Users/Me`, then clears it unless the returned user is an enabled administrator. `checkAuth` calls `/Users/Me` so stale or revoked sessions fail closed. `checkError` clears only on 401. `logout` clears token and device ID and resolves without claiming server revocation. `getPermissions` resolves `'administrator'` only after validation.

- [ ] **Step 4: Verify auth behavior**

Run:

```bash
npm --prefix admin test -- --run src/auth/authProvider.test.ts
npm --prefix admin run typecheck
npm --prefix admin run lint
```

Expected: all pass.

- [ ] **Step 5: Commit Task 2**

```bash
git add admin/src/auth
git commit -m "feat(admin): authenticate administrators"
```

---

### Task 3: Users Data Provider And Explicit Commands

**Files:**
- Create: `admin/src/api/dataProvider.ts`
- Test: `admin/src/api/dataProvider.test.ts`
- Create: `admin/src/users/userCommands.ts`
- Test: `admin/src/users/userCommands.test.ts`

**Interfaces:**
- Consumes: `apiRequest`, `ApiError`, `UserRecord`, policy payloads.
- Produces: `dataProvider: DataProvider`, `updateUserPassword(id, input)`, `updateUserPolicy(id, input)`.

- [ ] **Step 1: Write failing resource-provider tests**

Assert exact methods, URLs, bodies, and result shapes:

```ts
await expect(dataProvider.getList('users', {
  pagination: { page: 2, perPage: 2 },
  sort: { field: 'Name', order: 'ASC' },
  filter: {}, meta: undefined,
})).resolves.toEqual({
  data: [expect.objectContaining({ Id: 'u3', id: 'u3' })],
  total: 3,
});

await dataProvider.update('users', {
  id: 'u2', data: { Name: 'Robert' }, previousData: bob,
});
expect(fetchRequest).toHaveBeenNthCalledWith(1, '/Users?userId=u2', expect.objectContaining({
  method: 'POST', body: JSON.stringify({ Name: 'Robert' }),
}));
expect(fetchRequest).toHaveBeenNthCalledWith(2, '/Users/u2');
```

Cover stable sort tie-breaking by `Id`, out-of-range pages, `getOne`, create payload, delete returning `previousData`, URL encoding, resource rejection, and explicit rejection of unused bulk/reference operations.

- [ ] **Step 2: Write failing command tests**

Assert exact payloads:

```ts
await updateUserPassword('u2', { newPassword: 'new password', resetPassword: false });
expect(apiRequest).toHaveBeenCalledWith('/Users/u2/Password', {
  method: 'POST',
  body: JSON.stringify({ NewPw: 'new password', ResetPassword: false }),
});

await updateUserPolicy('u2', { isAdministrator: true, isDisabled: false });
expect(apiRequest).toHaveBeenCalledWith('/Users/u2/Policy', {
  method: 'POST',
  body: JSON.stringify({
    IsAdministrator: true,
    IsDisabled: false,
    AuthenticationProviderId: 'TJXY.LocalAuthentication',
    PasswordResetProviderId: 'TJXY.LocalPasswordReset'
  }),
});
```

- [ ] **Step 3: Run provider and command tests to verify red**

Run: `npm --prefix admin test -- --run src/api/dataProvider.test.ts src/users/userCommands.test.ts`

Expected: FAIL because modules are missing.

- [ ] **Step 4: Implement the provider and command clients**

Use `encodeURIComponent(String(params.id))` for path/query IDs. Adapt every returned record with:

```ts
export function toAdminUser(user: TjxyUser): UserRecord {
  if (!user.Id) throw new ApiError(0, 'invalid-response', 'The server returned an invalid user.');
  return { ...user, id: user.Id };
}
```

Only `users` is accepted. Sorting operates on `Name`; unsupported sort fields fail explicitly. Mutations refetch after a 204 command. Do not implement `updateMany` or `deleteMany` by issuing partial loops.

- [ ] **Step 5: Verify the provider boundary**

Run:

```bash
npm --prefix admin test -- --run src/api/dataProvider.test.ts src/users/userCommands.test.ts
npm --prefix admin run typecheck
npm --prefix admin run lint
```

Expected: all pass.

- [ ] **Step 6: Commit Task 3**

```bash
git add admin/src/api/dataProvider* admin/src/users/userCommands*
git commit -m "feat(admin): adapt user management API"
```

---

### Task 4: Login And Users Operational UI

**Files:**
- Create: `admin/src/main.tsx`
- Create: `admin/src/App.tsx`
- Create: `admin/src/theme.ts`
- Create: `admin/src/layout/AdminLayout.tsx`
- Create: `admin/src/auth/LoginPage.tsx`
- Create: `admin/src/users/UserList.tsx`
- Create: `admin/src/users/UserCreate.tsx`
- Create: `admin/src/users/UserShow.tsx`
- Create: `admin/src/users/UserEdit.tsx`
- Create: `admin/src/users/UserStatus.tsx`
- Test: colocated component tests.

**Interfaces:**
- Consumes: `authProvider`, `dataProvider`, user commands.
- Produces: `App`, React Admin `users` resource, accessible workflows at `/admin/login` and `/admin/users/*`.

- [ ] **Step 1: Write failing UI tests**

Render with React Admin test context and mocked providers. Cover:

- login fields use `username` and `password`, submit through `useLogin`, retain username after failure, and never render password text;
- user list renders name, Administrator/User, Enabled/Disabled, show/edit icon labels, and switches to a nonoverflowing stacked layout below 600px;
- create sends one create command;
- edit rename sends only rename;
- password submit sends only password and clears the password inputs;
- policy submit sends only policy and refetches;
- delete uses pessimistic confirmation and renders the 409 final-admin message;
- disabled controls while each command is pending prevent duplicate submissions.

Use user-visible assertions, for example:

```tsx
await user.click(screen.getByRole('button', { name: 'Save access policy' }));
expect(updateUserPolicy).toHaveBeenCalledWith('u2', {
  isAdministrator: true,
  isDisabled: false,
});
expect(updateUserPassword).not.toHaveBeenCalled();
```

- [ ] **Step 2: Run UI tests to verify red**

Run: `npm --prefix admin test -- --run`

Expected: FAIL because the components are missing.

- [ ] **Step 3: Implement the composition root and theme**

Compose exactly one resource:

```tsx
export function App() {
  return (
    <Admin
      basename="/admin"
      authProvider={authProvider}
      dataProvider={dataProvider}
      layout={AdminLayout}
      loginPage={LoginPage}
      theme={theme}
      requireAuth
    >
      <Resource
        name="users"
        list={UserList}
        create={UserCreate}
        edit={UserEdit}
        show={UserShow}
        icon={PeopleIcon}
        options={{ label: 'Users' }}
      />
    </Admin>
  );
}
```

Use zero negative letter spacing, 4-8px radii, 40px stable icon-button targets, white/`#f6f7f8` surfaces, `#172126` text, `#087f75` primary/status, `#b45309` warning, and MUI error red. Do not create dashboard cards or links to missing pages.

- [ ] **Step 4: Implement independent user command surfaces**

Use separate `SimpleForm`/React Hook Form boundaries or local forms so each command has one submit handler. Rename starts from current `Name`; password requires confirmation and supports an explicit reset-to-empty checkbox; policy uses switches for administrator and disabled flags. Refetch the record after success. Map known categories to concise messages and keep form state on failure.

Desktop list uses a dense table. At `sm` and below, hide the table header and render each row as a stable two-column definition layout with actions on their own line. Use MUI icons with `aria-label` and tooltips.

- [ ] **Step 5: Verify UI, type, lint, and production build**

Run:

```bash
npm --prefix admin test -- --run
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin run build
```

Expected: all exit 0 and `admin/dist/index.html` references `/admin/assets/...`.

- [ ] **Step 6: Commit Task 4**

```bash
git add admin/src/main.tsx admin/src/App.tsx admin/src/theme.ts admin/src/layout admin/src/auth/LoginPage* admin/src/users
git commit -m "feat(admin): add local user workflows"
```

---

### Task 5: Scoped Rust SPA Serving And Required Production Assets

**Files:**
- Create: `crates/server/src/admin_assets.rs`
- Modify: `crates/server/src/lib.rs`
- Modify: `crates/server/src/main.rs`
- Modify: `crates/server/Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/server/tests/admin_assets.rs`
- Modify: `crates/server/tests/system_routes.rs`

**Interfaces:**
- Consumes: `AppState`, existing `build_router`, `tower_http::services::{ServeDir, ServeFile}`.
- Produces: `build_router_with_admin_dist(state, path) -> Result<Router, AdminAssetsError>` and `AdminAssetsError`.

- [ ] **Step 1: Write failing static-service integration tests**

Create a temporary distribution containing `index.html` and `assets/app.js`. Assert:

```rust
let app = build_router_with_admin_dist(state(), dist.path()).unwrap();
assert_eq!(request(&app, Method::GET, "/admin").await.status(), StatusCode::PERMANENT_REDIRECT);
assert_eq!(location, "/admin/");
assert_html(request(&app, Method::GET, "/admin/").await, "TJXY Admin");
assert_html(request(&app, Method::GET, "/admin/users/u1").await, "TJXY Admin");
assert_eq!(request(&app, Method::GET, "/admin/assets/app.js").await.headers()[CONTENT_TYPE], "text/javascript");
assert_eq!(request(&app, Method::GET, "/admin/assets/missing.js").await.status(), StatusCode::NOT_FOUND);
assert_eq!(request(&app, Method::POST, "/admin/users/u1").await.status(), StatusCode::METHOD_NOT_ALLOWED);
assert_eq!(request(&app, Method::GET, "/not-an-api").await.status(), StatusCode::NOT_FOUND);
assert_eq!(request(&app, Method::GET, "/Users").await.status(), StatusCode::UNAUTHORIZED);
```

Also assert missing directory, directory without `index.html`, and non-file `index.html` return `AdminAssetsError` without exposing unrelated paths in `Debug` output.

- [ ] **Step 2: Run the static tests to verify red**

Run: `cargo +1.88.0 test -p tjxy-server --test admin_assets --locked`

Expected: FAIL because the constructor does not exist.

- [ ] **Step 3: Add tower-http and implement the scoped service**

Add:

```toml
tower-http = { version = "0.7.0", features = ["fs"] }
```

Validate `dist_dir` and `dist_dir/index.html` synchronously before constructing the router. Build a `ServeDir` for real files and a scoped fallback service that serves `index.html` only for GET/HEAD requests accepting HTML whose path is not below `/assets/`. Missing asset requests return 404, and method-not-allowed responses never call the fallback. Nest the service only at `/admin`, and add an exact permanent redirect from `/admin` to `/admin/`. Keep `build_router(state)` unchanged and merge/nest only in the new constructor.

- [ ] **Step 4: Make the production binary require assets**

Add:

```rust
#[error("TJXY admin assets are invalid: {0}")]
AdminAssets(#[from] AdminAssetsError),
```

Read `TJXY_ADMIN_DIST_DIR` with default `admin/dist`, construct the router before binding the listener, and serve that router. Add a unit test for the default and explicit path helper without mutating global environment concurrently; pass an injected lookup closure if necessary.

- [ ] **Step 5: Verify Rust static and existing API contracts**

Run:

```bash
cargo +1.88.0 test -p tjxy-server --test admin_assets --test auth_routes --test system_routes --locked
cargo +1.88.0 clippy -p tjxy-server --all-targets --locked -- -D warnings
cargo +1.88.0 fmt --all -- --check
```

Expected: all pass; unknown API remains 404.

- [ ] **Step 6: Commit Task 5**

```bash
git add Cargo.lock crates/server/Cargo.toml crates/server/src/admin_assets.rs crates/server/src/lib.rs crates/server/src/main.rs crates/server/tests/admin_assets.rs crates/server/tests/system_routes.rs
git commit -m "feat(server): serve scoped admin SPA"
```

---

### Task 6: Production Browser Workflow And Visual Verification

**Files:**
- Create: `admin/playwright.config.ts`
- Create: `admin/e2e/users.spec.ts`
- Create: `admin/scripts/run-e2e-server.sh`
- Modify: `admin/package.json`
- Modify: `admin/package-lock.json`

**Interfaces:**
- Consumes: built `admin/dist`, production Rust binary, Users workflows.
- Produces: repeatable `npm --prefix admin run e2e` release gate and screenshot artifacts.

- [ ] **Step 1: Write the production server harness**

The shell script must use `mktemp -d`, trap cleanup, set a fixed test-only server UUID, SQLite URL, assets directory, bootstrap administrator, disabled Redis, and `TJXY_ADMIN_DIST_DIR` to the absolute `admin/dist`. It must bind `127.0.0.1:${TJXY_E2E_PORT:-18096}` and execute:

```bash
cargo +1.88.0 run -p tjxy-server --locked
```

Do not print the bootstrap password. Playwright `webServer` runs the script, waits for `/health/ready`, reuses no existing server in CI, and records trace/screenshot/video only on failure.

- [ ] **Step 2: Write the failing browser workflow**

Cover in one serial, independently named flow:

1. `/admin/users` deep link redirects to login.
2. Wrong password shows a generic authentication error.
3. Admin login opens Users and shows Admin.
4. Create `Bob` with a password.
5. Rename Bob to Robert.
6. Disable and re-enable Robert, then grant administrator.
7. Change Robert's password.
8. Reload the deep link and confirm persisted state.
9. Log out, log in as Robert, and confirm administrator access.
10. Delete the original Admin so Robert becomes the only enabled administrator.
11. Create a non-admin `Viewer`, log in as Viewer, and confirm the admin shell denies access.
12. Log back in as Robert, delete Viewer, then verify absence.
13. Confirm deleting Robert now displays the last-enabled-administrator conflict without removing it.

Assert no `pageerror`, unexpected console error, failed same-origin API request, horizontal document overflow, or intersecting action controls. Capture named desktop 1440x900 and mobile 390x844 screenshots after the Users list and edit screens settle.

- [ ] **Step 3: Run E2E to verify red or expose missing behavior**

Run:

```bash
npm --prefix admin run build
npm --prefix admin run e2e
```

Expected before fixes: at least one browser assertion fails with a specific missing UI or static behavior, not a harness timeout.

- [ ] **Step 4: Fix only evidence-backed browser failures**

Adjust selectors, responsive layout, focus management, or error mapping in the owning component. Do not weaken assertions, add sleeps, or hide console errors. Use Playwright web-first assertions and stable roles/names.

- [ ] **Step 5: Verify both viewports and inspect screenshots**

Run `npm --prefix admin run e2e` twice. Inspect every saved reference screenshot at original resolution. Confirm no clipping, overlap, blank application, unreadable text, or layout shift.

- [ ] **Step 6: Commit Task 6**

```bash
git add admin/package.json admin/package-lock.json admin/playwright.config.ts admin/e2e admin/scripts
git commit -m "test(admin): gate user workflow in browser"
```

---

### Task 7: CI, Documentation, And Release Audit

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`
- Modify: `docs/api-parity.md`
- Modify: `.gitignore`
- Modify any files found defective by the audit, scoped to this slice.

**Interfaces:**
- Consumes: all prior tasks.
- Produces: documented setup/deployment and automated release evidence.

- [ ] **Step 1: Add the frontend CI job**

Use `actions/setup-node` with Node `22.22.3` and npm cache keyed by `admin/package-lock.json`. Because the browser harness starts the real Rust server, install Rust 1.88.0 with the minimal profile in this job before running:

```yaml
- run: npm --prefix admin ci
- run: npm --prefix admin run typecheck
- run: npm --prefix admin run lint
- run: npm --prefix admin test -- --run
- run: npm --prefix admin run build
- run: npm --prefix admin exec -- playwright install --with-deps chromium
- run: npm --prefix admin run e2e
```

Upload Playwright artifacts only on failure. Keep Rust SQLite and PostgreSQL jobs unchanged except where the new server dependency requires lockfile use already present.

- [ ] **Step 2: Update user-facing documentation**

README must document Node floor, install/check/build commands, `TJXY_ADMIN_DIST_DIR`, required production build, `/admin/`, same-origin behavior, environment-bootstrap limitation, sessionStorage logout limitation, and first-slice scope. `docs/api-parity.md` marks React Admin login + Users CRUD complete but leaves bootstrap, devices, API keys, and all other admin domains incomplete.

- [ ] **Step 3: Run the strongest combined verification**

Run:

```bash
npm --prefix admin ci
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build
npm --prefix admin run e2e
cargo +1.88.0 fmt --all -- --check
cargo +1.88.0 clippy --workspace --all-targets --locked -- -D warnings
cargo +1.88.0 test --workspace --locked
TJXY_TEST_DATABASE_URL=postgresql://postgres:tjxy@127.0.0.1:55432/tjxy_test cargo +1.88.0 test -p tjxy-server --tests --locked
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml"); puts "workflow yaml ok"'
```

Expected: every command exits 0. If local PostgreSQL is unavailable, start the pinned CI PostgreSQL 17 image first and record the exact substitute URL; do not skip the database gate silently.

- [ ] **Step 4: Perform the post-verification quality review**

Inspect the final diff for:

- token/password leakage, XSS-prone raw HTML, open redirects, and broad CORS;
- global SPA fallback, directory traversal, symlink/path disclosure, or readiness lying about missing assets;
- swallowed `fetch`/JSON errors or generic mutations that can partially succeed;
- React state updates after unmount, duplicate submits, unstable list keys, unnecessary renders, or unbounded browser storage;
- mobile overflow, inaccessible icon-only actions, missing focus, and controls that resize during loading;
- placeholder navigation or claims that the rest of PLAN section 16 is complete.

Fix any finding with a focused regression test, then rerun its owning suite and the combined checks affected by the fix.

- [ ] **Step 5: Commit Task 7**

```bash
git add .github/workflows/ci.yml .gitignore README.md docs/api-parity.md
git commit -m "docs: gate React Admin users slice"
```

- [ ] **Step 6: Update the parent PLAN audit**

Mark only the React Admin login + Users CRUD vertical slice complete in the working task tracker. Re-read `PLAN.md` section 16 and retain every other numbered page as pending evidence. Continue with the highest-value backend contract needed by the next page rather than declaring the full PLAN complete.
