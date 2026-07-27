import { expect, test, type Page } from '@playwright/test';

import {
  assertActionControlsDoNotIntersect,
  assertNoHorizontalOverflow,
  login,
  monitorPage,
  screenshot,
} from './support';

const adminPassword = 'admin-password';
const robertPassword = 'robert-password';

test.describe.serial('administrator library lifecycle', () => {
  test('manages libraries and persisted scan policies through the production application', async ({ page }, testInfo) => {
    const diagnostics = monitorPage(page);

    await page.goto('/admin/libraries');
    await login(page, 'Admin', adminPassword);
    await page.getByRole('menuitem', { name: 'Google Drive' }).click();
    await expect(page).toHaveURL(/\/admin\/storage\/google-drive$/);
    await expect(page.getByRole('heading', { name: 'Google Drive' })).toBeVisible();
    await page.getByRole('menuitem', { name: 'OneDrive' }).click();
    await expect(page).toHaveURL(/\/admin\/storage\/onedrive$/);
    await expect(page.getByRole('heading', { name: 'OneDrive' })).toBeVisible();
    await page.getByRole('menuitem', { name: 'Libraries' }).click();
    await expect(page).toHaveURL(/\/admin\/libraries$/);
    await expect(page.getByRole('table', { name: 'Libraries' })).toBeVisible();

    await page.getByRole('button', { name: 'Add library' }).click();
    await page.getByRole('textbox', { name: 'Library name' }).fill('Movies');
    await page.getByRole('button', { name: 'Create library' }).click();
    await expect(page.getByRole('rowheader', { name: 'Movies' })).toBeVisible();

    await page.getByRole('menuitem', { name: 'Tasks' }).click();
    await expect(page).toHaveURL(/\/admin\/tasks$/);
    await expect(page.getByRole('table', { name: 'Scheduled tasks' })).toBeVisible();
    await page.getByRole('button', { name: 'Start' }).click();
    const fullScanJob = page.getByRole('table', { name: 'Recent durable jobs' })
      .getByRole('row')
      .filter({ hasText: 'FullMediaScan' });
    await expect(fullScanJob.first()).toBeVisible({ timeout: 15_000 });
    await expect(fullScanJob.first()).toContainText(/Pending|Running|Completed/);
    await page.reload();
    await expect(fullScanJob.first()).toBeVisible();
    await assertNoHorizontalOverflow(page);
    await screenshot(page, testInfo, 'tasks-desktop.png');

    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await expect(page.getByRole('table', { name: 'Scheduled tasks' })).toBeVisible();
    await assertNoHorizontalOverflow(page);
    await screenshot(page, testInfo, 'tasks-mobile.png');
    await page.setViewportSize({ width: 1440, height: 900 });

    await page.getByRole('menuitem', { name: 'Libraries' }).click();
    await expect(page).toHaveURL(/\/admin\/libraries$/);

    await page.getByRole('button', { name: 'Edit Movies' }).click();
    await page.getByRole('combobox', { name: 'Scan profile' }).click();
    await page.getByRole('option', { name: 'Hybrid' }).click();
    await page.getByRole('button', { name: 'Save scan policy' }).click();
    const movies = page.getByRole('row').filter({
      has: page.getByRole('rowheader', { name: 'Movies' }),
    });
    await expect(movies).toContainText('Hybrid');
    await expect(movies).toContainText('background');

    await page.getByRole('button', { name: 'Manage background candidates for Movies' }).click();
    await expect(page.getByRole('dialog', { name: 'Background candidates for Movies' })).toBeVisible();
    await expect(page.getByRole('table', { name: 'Pinned background candidates' }))
      .toContainText('No background candidates are pinned.');
    await page.getByRole('button', { name: 'Close' }).click();

    await page.reload();
    await expect(movies).toContainText('Hybrid');
    await expect(movies).toContainText('background');
    await assertNoHorizontalOverflow(page);
    await screenshot(page, testInfo, 'libraries-desktop.png');

    await page.getByRole('button', { name: 'Edit Movies' }).click();
    await page.getByRole('textbox', { name: 'Library name' }).fill('Archive Movies');
    await page.getByRole('button', { name: 'Rename' }).click();
    await expect(page.getByRole('rowheader', { name: 'Archive Movies' })).toBeVisible();

    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await expect(page.getByRole('rowheader', { name: 'Archive Movies' })).toBeVisible();
    await assertNoHorizontalOverflow(page);
    await screenshot(page, testInfo, 'libraries-mobile.png');
    await page.setViewportSize({ width: 1440, height: 900 });

    await page.getByRole('button', { name: 'Edit Archive Movies' }).click();
    await page.getByRole('button', { name: 'Delete library' }).click();
    await page.getByRole('button', { name: 'Confirm delete' }).click();
    await expect(page.getByRole('rowheader', { name: 'Archive Movies' })).toHaveCount(0);
    await expect(page.getByText('No libraries are configured.')).toBeVisible();

    expect(diagnostics.pageErrors, 'page errors').toEqual([]);
    expect(diagnostics.consoleErrors, 'console errors').toEqual([]);
    expect(diagnostics.failedRequests, 'failed same-origin requests').toEqual([]);
  });
});

test.describe.serial('administrator user lifecycle', () => {
  test('manages local users through the production application', async ({ page }, testInfo) => {
    const diagnostics = monitorPage(page);

    await page.goto('/admin/users');
    await expect(page.getByRole('heading', { name: 'TJXY Admin' })).toBeVisible();
    diagnostics.expectHttpConsoleError(/^\/Users\/AuthenticateByName$/);
    await login(page, 'Admin', 'wrong-password');
    await expect(page.getByRole('alert')).toContainText('Your session is not valid.');

    await login(page, 'Admin', adminPassword);
    await expect(page).toHaveURL(/\/admin\/users$/);
    await expect(page.getByRole('table', { name: 'Users' })).toContainText('Admin');
    await assertNoHorizontalOverflow(page);
    await screenshot(page, testInfo, 'users-desktop.png');

    await page.getByRole('link', { name: /create/i }).click();
    await page.getByLabel('Name').fill('Bob');
    await page.getByLabel('Initial password').fill('bob-password');
    await page.getByRole('button', { name: /save/i }).click();
    await expect(page.getByText('Bob', { exact: true })).toBeVisible();
    await page.getByRole('link', { name: /edit/i }).click();

    await page.getByRole('textbox', { name: 'Name' }).fill('Robert');
    await page.getByRole('button', { name: 'Save name' }).click();
    await expect(page.getByRole('textbox', { name: 'Name' })).toHaveValue('Robert');

    const disabled = page.getByRole('switch', { name: 'Disabled' });
    await disabled.check();
    await page.getByRole('button', { name: 'Save access policy' }).click();
    await disabled.uncheck();
    await page.getByRole('switch', { name: 'Administrator' }).check();
    await page.getByRole('button', { name: 'Save access policy' }).click();

    await page.getByLabel(/^New password/).fill(robertPassword);
    await page.getByLabel(/^Confirm password/).fill(robertPassword);
    await page.getByRole('button', { name: 'Save password' }).click();
    await expect(page.getByLabel(/^New password/)).toHaveValue('');

    const editUrl = page.url();
    await page.reload();
    await expect(page).toHaveURL(editUrl);
    await expect(page.getByRole('textbox', { name: 'Name' })).toHaveValue('Robert');
    await expect(page.getByRole('switch', { name: 'Administrator' })).toBeChecked();
    await assertNoHorizontalOverflow(page);
    await screenshot(page, testInfo, 'edit-desktop.png');

    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await expect(page.getByRole('textbox', { name: 'Name' })).toHaveValue('Robert');
    await assertNoHorizontalOverflow(page);
    await assertActionControlsDoNotIntersect(page);
    await screenshot(page, testInfo, 'edit-mobile.png');
    await page.setViewportSize({ width: 1440, height: 900 });

    await logout(page);
    await login(page, 'Robert', robertPassword);
    await expect(page.getByRole('table', { name: 'Users' })).toBeVisible();

    await editUser(page, 'Admin');
    await deleteCurrentUser(page);
    await expect(
      page.getByRole('table', { name: 'Users' }).getByText('Admin', { exact: true }),
    ).toHaveCount(0);

    await createUser(page, 'Viewer', 'viewer-password');
    await logout(page);
    await login(page, 'Viewer', 'viewer-password');
    await expect(page).toHaveURL(/\/admin\/login$/);
    await expect(page.getByText(/administrator access is required/i)).toBeVisible();

    await login(page, 'Robert', robertPassword);
    await editUser(page, 'Viewer');
    await deleteCurrentUser(page);
    await expect(page.getByRole('table', { name: 'Users' })).not.toContainText('Viewer');

    await editUser(page, 'Robert');
    await page.getByRole('button', { name: 'Delete user' }).click();
    diagnostics.expectHttpConsoleError(/^\/Users\/[^/]+$/);
    diagnostics.expectFailedHttpResponse(409, /^\/Users\/[^/]+$/);
    await page.getByRole('button', { name: 'Confirm delete' }).click();
    await expect(page.getByText('The last enabled administrator cannot be removed.')).toBeVisible();
    await expect(page.getByRole('textbox', { name: 'Name' })).toHaveValue('Robert');

    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/admin/users');
    await expect(page.locator('header').getByText('Users', { exact: true })).toBeVisible();
    await expect(page.locator('[role="table"][aria-label="Users"]')).toContainText('Robert');
    await assertNoHorizontalOverflow(page);
    await assertActionControlsDoNotIntersect(page);
    await screenshot(page, testInfo, 'users-mobile.png');

    expect(diagnostics.pageErrors, 'page errors').toEqual([]);
    expect(diagnostics.consoleErrors, 'console errors').toEqual([]);
    expect(diagnostics.failedRequests, 'failed same-origin requests').toEqual([]);
  });
});

async function logout(page: Page) {
  await page.getByRole('button', { name: /profile|account/i }).click();
  await page.getByRole('menuitem', { name: /logout/i }).click();
  await expect(page).toHaveURL(/\/admin\/login$/);
}

async function createUser(page: Page, name: string, password: string) {
  await page.getByRole('menuitem', { name: 'Users' }).click();
  await page.getByRole('link', { name: /create/i }).click();
  await page.getByLabel('Name').fill(name);
  await page.getByLabel('Initial password').fill(password);
  await page.getByRole('button', { name: /save/i }).click();
  await expect(page.getByText(name, { exact: true })).toBeVisible();
  await page.getByRole('menuitem', { name: 'Users' }).click();
}

async function editUser(page: Page, name: string) {
  await page.getByRole('menuitem', { name: 'Users' }).click();
  const row = page.getByRole('row').filter({
    has: page.getByText(name, { exact: true }),
  });
  await row.getByRole('link', { name: /edit user/i }).click();
  await expect(page.getByRole('textbox', { name: 'Name' })).toHaveValue(name);
}

async function deleteCurrentUser(page: Page) {
  await page.getByRole('button', { name: 'Delete user' }).click();
  await page.getByRole('button', { name: 'Confirm delete' }).click();
  await expect(page).toHaveURL(/\/admin\/users$/);
}
