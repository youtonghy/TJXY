import { expect, test, type Page } from '@playwright/test';

import {
  assertActionControlsDoNotIntersect,
  assertNoHorizontalOverflow,
  assertUniqueH1,
  login,
  monitorPage,
} from './support';

const adminPassword = 'admin-password';
const robertPassword = 'robert-password';

test.describe.serial('administrator library lifecycle', () => {
  test('manages libraries and persisted scan policies through the production application', async ({ page }) => {
    const diagnostics = monitorPage(page);

    await page.goto('/admin/libraries');
    await login(page, 'Admin', adminPassword);
    await assertUniqueH1(page);
    await page.getByRole('link', { name: 'Google Drive' }).click();
    await expect(page).toHaveURL(/\/admin\/storage\/google-drive$/);
    await expect(page.getByRole('heading', { level: 1, name: 'Google Drive' })).toBeVisible();
    await assertUniqueH1(page);
    await page.getByRole('link', { name: 'OneDrive' }).click();
    await expect(page).toHaveURL(/\/admin\/storage\/onedrive$/);
    await expect(page.getByRole('heading', { level: 1, name: 'OneDrive' })).toBeVisible();
    await assertUniqueH1(page);
    await page.getByRole('link', { name: 'Libraries' }).click();
    await expect(page).toHaveURL(/\/admin\/libraries$/);
    const libraries = page.getByRole('grid', { name: 'Libraries' });
    await expect(page.getByText('No libraries are configured.')).toBeVisible();
    await assertUniqueH1(page);

    await page.getByRole('button', { name: 'Add library' }).click();
    await page.getByRole('textbox', { name: 'Library name' }).fill('Movies');
    await page.getByRole('button', { name: 'Create library' }).click();
    await expect(libraries).toBeVisible();
    await expect(libraries.getByRole('rowheader', { name: 'Movies' })).toBeVisible();

    await page.getByRole('link', { name: 'Tasks' }).click();
    await expect(page).toHaveURL(/\/admin\/tasks$/);
    await assertUniqueH1(page);
    const scheduledTasks = page.getByRole('list', { name: 'Scheduled tasks' });
    await expect(scheduledTasks).toBeVisible();
    await scheduledTasks.getByRole('button', { name: 'Start Scan Media Library' }).click();
    const fullScanJob = page.getByRole('grid', { name: 'Recent durable jobs' })
      .getByRole('row')
      .filter({ hasText: 'FullMediaScan' });
    await expect(fullScanJob.first()).toBeVisible({ timeout: 15_000 });
    await expect(fullScanJob.first()).toContainText(/Pending|Running|Completed/);
    await page.reload();
    await expect(fullScanJob.first()).toBeVisible();
    await assertNoHorizontalOverflow(page);

    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await expect(page.getByRole('list', { name: 'Scheduled tasks' })).toBeVisible();
    await assertNoHorizontalOverflow(page);
    await assertActionControlsDoNotIntersect(page);
    await page.setViewportSize({ width: 1440, height: 900 });

    await page.getByRole('link', { name: 'Libraries' }).click();
    await expect(page).toHaveURL(/\/admin\/libraries$/);

    await page.getByRole('link', { name: 'Edit Movies' }).click();
    await expect(page).toHaveURL(/\/admin\/libraries\/[^/?]+$/);
    await expect(page.getByRole('heading', { level: 1, name: 'Movies' })).toBeVisible();
    await assertUniqueH1(page);
    const scanProfile = page.getByRole('button', { name: /Scan profile/u });
    await scanProfile.click();
    await page.getByRole('option', { name: 'Manual' }).click();
    await persistLibraryPolicy(page);
    await expect(scanProfile).toContainText('Manual');

    await page.reload();
    await expect(page.getByRole('button', { name: /Scan profile/u })).toContainText('Manual');
    await assertNoHorizontalOverflow(page);

    await page.getByRole('textbox', { name: 'Library name' }).fill('Archive Movies');
    await page.getByRole('button', { name: 'Rename' }).click();
    await expect(page.getByRole('heading', { level: 1, name: 'Archive Movies' })).toBeVisible();

    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await expect(page.getByRole('heading', { level: 1, name: 'Archive Movies' })).toBeVisible();
    await assertNoHorizontalOverflow(page);
    await assertActionControlsDoNotIntersect(page);
    await page.setViewportSize({ width: 1440, height: 900 });

    await page.getByRole('button', { name: 'Delete library' }).click();
    const deleteLibraryDialog = page.getByRole('dialog', { name: 'Delete Archive Movies?' });
    await expect(deleteLibraryDialog.getByRole('button', { name: 'Cancel' })).toBeFocused();
    await deleteLibraryDialog.getByRole('button', { name: 'Delete library' }).click();
    await expect(page).toHaveURL(/\/admin\/libraries$/);
    await expect(page.getByRole('grid', { name: 'Libraries' })).toHaveCount(0);
    await expect(page.getByText('No libraries are configured.')).toBeVisible();

    diagnostics.assertExpectedResponsesObserved();
    expect(diagnostics.pageErrors, 'page errors').toEqual([]);
    expect(diagnostics.consoleErrors, 'console errors').toEqual([]);
    expect(diagnostics.httpErrors, 'unexpected HTTP error responses').toEqual([]);
    expect(diagnostics.failedRequests, 'failed same-origin requests').toEqual([]);
  });
});

test.describe.serial('administrator user lifecycle', () => {
  test('manages local users through the production application', async ({ page }) => {
    const diagnostics = monitorPage(page);

    await page.goto('/admin/users');
    await expect(page.getByRole('heading', { level: 1, name: /Welcome back|欢迎回来/u })).toBeVisible();
    diagnostics.expectHttpConsoleError(/^\/Users\/AuthenticateByName$/);
    diagnostics.expectHttpErrorResponse(401, /^\/Users\/AuthenticateByName$/);
    await login(page, 'Admin', 'wrong-password');
    await expect(page.getByRole('alert')).toContainText('Sign in failed');

    await login(page, 'Admin', adminPassword);
    await expect(page).toHaveURL(/\/admin\/users$/);
    await assertUniqueH1(page);
    await expect(page.getByRole('grid', { name: 'Users' })).toContainText('Admin');
    await assertNoHorizontalOverflow(page);

    await page.getByRole('link', { name: 'Create user' }).click();
    await page.getByLabel('Name').fill('Bob');
    await page.getByLabel('Initial password').fill('bob-password');
    await page.getByRole('button', { name: 'Create user' }).click();
    await expect(page.getByRole('heading', { level: 1, name: 'Bob' })).toBeVisible();
    await page.getByRole('link', { name: 'Edit user' }).click();

    await page.getByRole('textbox', { name: 'Name' }).fill('Robert');
    await persistIdentity(page);
    await expect(page.getByRole('textbox', { name: 'Name' })).toHaveValue('Robert');

    const disabled = page.getByRole('switch', { name: 'Disabled' });
    await disabled.check({ force: true });
    await expect(disabled).toBeChecked();
    await persistAccessPolicy(page);
    await expect(disabled).toBeChecked();
    await disabled.uncheck({ force: true });
    await expect(disabled).not.toBeChecked();
    const administrator = page.getByRole('switch', { name: 'Administrator' });
    await administrator.check({ force: true });
    await expect(administrator).toBeChecked();
    await persistAccessPolicy(page);
    await expect(disabled).not.toBeChecked();
    await expect(administrator).toBeChecked();

    const newPassword = page.getByLabel(/^New password/u);
    const confirmPassword = page.getByLabel(/^Confirm password/u);
    await newPassword.fill(robertPassword);
    await confirmPassword.fill(robertPassword);
    await expect(newPassword).toHaveValue(robertPassword);
    const passwordSaved = page.waitForResponse((response) => (
      response.request().method() === 'POST'
        && /\/Users\/[^/]+\/Password$/u.test(new URL(response.url()).pathname)
    ));
    await page.getByRole('button', { name: 'Save password' }).click();
    expect((await passwordSaved).ok(), 'password update response').toBe(true);
    await expect(newPassword).toHaveValue('');

    const editUrl = page.url();
    await page.reload();
    await expect(page).toHaveURL(editUrl);
    await expect(page.getByRole('textbox', { name: 'Name' })).toHaveValue('Robert');
    await expect(page.getByRole('switch', { name: 'Administrator' })).toBeChecked();
    await assertNoHorizontalOverflow(page);

    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await expect(page.getByRole('textbox', { name: 'Name' })).toHaveValue('Robert');
    await assertNoHorizontalOverflow(page);
    await assertActionControlsDoNotIntersect(page);
    await page.setViewportSize({ width: 1440, height: 900 });

    await logout(page);
    await login(page, 'Robert', robertPassword);
    await expect(page).toHaveURL(editUrl);
    await page.getByRole('navigation', { name: 'Primary' })
      .getByRole('link', { name: 'Users' })
      .click();
    await expect(page.getByRole('grid', { name: 'Users' })).toBeVisible();

    await createUser(page, 'Viewer', 'viewer-password');
    await logout(page);
    await login(page, 'Viewer', 'viewer-password');
    await expect(page).toHaveURL(/\/admin\/login$/);
    await expect(page.getByRole('alert')).toContainText('Sign in failed');

    await login(page, 'Robert', robertPassword);
    await editUser(page, 'Viewer');
    await deleteCurrentUser(page);
    await expect(page.getByRole('grid', { name: 'Users' })).not.toContainText('Viewer');

    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/admin/users');
    await expect(page.getByRole('heading', { level: 1, name: 'Users' })).toBeVisible();
    await expect(page.getByRole('list', { name: 'Users mobile' })).toContainText('Robert');
    await assertNoHorizontalOverflow(page);
    await assertActionControlsDoNotIntersect(page);

    diagnostics.assertExpectedResponsesObserved();
    expect(diagnostics.pageErrors, 'page errors').toEqual([]);
    expect(diagnostics.consoleErrors, 'console errors').toEqual([]);
    expect(diagnostics.httpErrors, 'unexpected HTTP error responses').toEqual([]);
    expect(diagnostics.failedRequests, 'failed same-origin requests').toEqual([]);
  });
});

async function logout(page: Page) {
  await page.getByRole('button', { name: /Open account menu for/i }).click();
  await page.getByRole('menuitem', { name: 'Sign out' }).click();
  await expect(page).toHaveURL(/\/admin\/login$/);
}

async function createUser(page: Page, name: string, password: string) {
  await page.getByRole('navigation', { name: 'Primary' })
    .getByRole('link', { name: 'Users' })
    .click();
  await page.getByRole('link', { name: 'Create user' }).click();
  await page.getByLabel('Name').fill(name);
  await page.getByLabel('Initial password').fill(password);
  await page.getByRole('button', { name: 'Create user' }).click();
  await expect(page.getByRole('heading', { level: 1, name })).toBeVisible();
  await page.getByRole('navigation', { name: 'Breadcrumb' })
    .getByRole('link', { name: 'Users' })
    .click();
  await expect(page.getByRole('grid', { name: 'Users' }).getByRole('rowheader', { name })).toBeVisible();
}

async function editUser(page: Page, name: string) {
  await page.getByRole('navigation', { name: 'Primary' })
    .getByRole('link', { name: 'Users' })
    .click();
  await page.getByRole('link', { name: `Edit ${name}` }).click();
  await expect(page.getByRole('textbox', { name: 'Name' })).toHaveValue(name);
}

async function deleteCurrentUser(page: Page) {
  await page.getByRole('button', { name: 'Delete user' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('button', { name: 'Cancel' })).toBeFocused();
  await dialog.getByRole('button', { name: 'Delete user' }).click();
  await expect(page).toHaveURL(/\/admin\/users$/);
}

async function persistAccessPolicy(page: Page) {
  const policySaved = page.waitForResponse((response) => (
    response.request().method() === 'POST'
      && /\/Users\/[^/]+\/Policy$/u.test(new URL(response.url()).pathname)
  ));
  const userReloaded = page.waitForResponse((response) => (
    response.request().method() === 'GET'
      && /\/Users\/[^/]+$/u.test(new URL(response.url()).pathname)
  ));
  const save = page.getByRole('button', { name: 'Save access policy' });
  await save.click();
  await Promise.all([policySaved, userReloaded]);
  await expect(save).toBeEnabled();
}

async function persistIdentity(page: Page) {
  const identitySaved = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return response.request().method() === 'POST'
      && url.pathname === '/Users'
      && url.searchParams.has('userId');
  });
  const userReloaded = page.waitForResponse((response) => (
    response.request().method() === 'GET'
      && /\/Users\/[^/]+$/u.test(new URL(response.url()).pathname)
  ));
  const save = page.getByRole('button', { name: 'Save identity' });
  await save.click();
  await Promise.all([identitySaved, userReloaded]);
  await expect(save).toBeEnabled();
}

async function persistLibraryPolicy(page: Page) {
  const policySaved = page.waitForResponse((response) => (
    response.request().method() === 'POST'
      && new URL(response.url()).pathname === '/Library/VirtualFolders/LibraryOptions'
  ));
  const libraryReloaded = page.waitForResponse((response) => (
    response.request().method() === 'GET'
      && new URL(response.url()).pathname === '/Library/VirtualFolders'
  ));
  const save = page.getByRole('button', { name: 'Save scan policy' });
  await save.click();
  await Promise.all([policySaved, libraryReloaded]);
  await expect(save).toBeEnabled();
}
