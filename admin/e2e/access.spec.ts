import { expect, test } from '@playwright/test';

import {
  assertActionControlsDoNotIntersect,
  assertNoHorizontalOverflow,
  login,
  monitorPage,
  screenshot,
} from './support';

const adminPassword = 'admin-password';
const appName = 'Playwright Access';
const secondaryDeviceId = 'playwright-access-secondary';

test.use({ screenshot: 'off', trace: 'off', video: 'off' });

test.describe.serial('administrator access lifecycle', () => {
  test('manages durable API keys and secondary devices through the production application', async ({ page }, testInfo) => {
    const diagnostics = monitorPage(page);

    await page.goto('/admin/access');
    await login(page, 'Admin', adminPassword);
    await expect(page).toHaveURL(/\/admin\/access$/);
    await page.getByRole('tab', { name: 'API Keys' }).click();
    await page.getByRole('button', { name: 'Create API key' }).click();
    await page.getByRole('textbox', { name: 'Application name' }).fill(appName);
    await page.getByRole('button', { name: 'Create key' }).click();

    const keyRow = page.getByRole('row').filter({ hasText: appName });
    await expect(keyRow).toBeVisible();
    await expect(keyRow.locator('code')).toHaveText('****************');
    await keyRow.getByRole('button', { name: `Show key for ${appName}` }).click();
    const rawKey = await keyRow.locator('code').innerText();
    expect(/^[0-9a-f]{64}$/u.test(rawKey), 'revealed key has canonical format').toBe(true);
    const authenticated = await page.request.get('/Users/Me', {
      headers: { Authorization: tokenHeader(rawKey) },
    });
    expect(authenticated.status()).toBe(200);

    await page.reload();
    await page.getByRole('tab', { name: 'API Keys' }).click();
    const reloadedKeyRow = page.getByRole('row').filter({ hasText: appName });
    await expect(reloadedKeyRow.locator('code')).toHaveText('****************');
    await reloadedKeyRow.getByRole('button', { name: `Show key for ${appName}` }).click();
    const recoveredKey = await reloadedKeyRow.locator('code').innerText();
    expect(recoveredKey === rawKey, 'reload recovers the same key').toBe(true);
    await reloadedKeyRow.getByRole('button', { name: `Hide key for ${appName}` }).click();

    const secondaryLogin = await page.request.post('/Users/AuthenticateByName', {
      headers: { Authorization: secondaryIdentity() },
      data: { Username: 'Admin', Pw: adminPassword },
    });
    expect(secondaryLogin.status()).toBe(200);
    const secondaryBody = await secondaryLogin.json() as { AccessToken?: unknown };
    expect(typeof secondaryBody.AccessToken === 'string', 'secondary login returns a token').toBe(true);
    const secondaryToken = String(secondaryBody.AccessToken);

    await page.getByRole('tab', { name: 'Devices' }).click();
    await page.getByRole('button', { name: 'Reload devices' }).click();
    await page.getByRole('button', { name: 'Edit Secondary browser' }).click();
    const customName = page.getByRole('textbox', { name: 'Custom device name' });
    await customName.fill('Playwright secondary');
    await page.getByRole('button', { name: 'Save device name' }).click();
    await expect(page.getByText('Playwright secondary', { exact: true })).toBeVisible();
    await assertNoHorizontalOverflow(page);
    await assertActionControlsDoNotIntersect(page);
    await page.getByRole('button', { name: 'Revoke Playwright secondary' }).click();
    await expect(page.getByRole('dialog')).toContainText('Playwright secondary');
    await page.getByRole('button', { name: 'Revoke device' }).click();
    const revokedDevice = await page.request.get('/Users/Me', {
      headers: { Authorization: tokenHeader(secondaryToken) },
    });
    expect(revokedDevice.status()).toBe(401);

    await page.getByRole('tab', { name: 'API Keys' }).click();
    await assertNoHorizontalOverflow(page);
    await assertActionControlsDoNotIntersect(page);
    await screenshot(page, testInfo, 'access-desktop.png');

    await page.setViewportSize({ width: 768, height: 1024 });
    await page.reload();
    await page.getByRole('tab', { name: 'API Keys' }).click();
    await expect(page.getByRole('row').filter({ hasText: appName })).toBeVisible();
    await assertNoHorizontalOverflow(page);
    await assertActionControlsDoNotIntersect(page);

    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await page.getByRole('tab', { name: 'API Keys' }).click();
    const mobileKeyRow = page.getByRole('row').filter({ hasText: appName });
    await expect(mobileKeyRow.locator('code')).toHaveText('****************');
    await assertNoHorizontalOverflow(page);
    await assertActionControlsDoNotIntersect(page);
    await screenshot(page, testInfo, 'access-api-keys-mobile.png');
    await page.getByRole('tab', { name: 'Devices' }).click();
    await expect(page.getByRole('table', { name: 'Devices' })).toBeVisible();
    await assertNoHorizontalOverflow(page);
    await assertActionControlsDoNotIntersect(page);
    await screenshot(page, testInfo, 'access-devices-mobile.png');

    await page.setViewportSize({ width: 1440, height: 900 });
    await page.reload();
    await page.getByRole('tab', { name: 'API Keys' }).click();
    await page.getByRole('button', { name: `Delete key for ${appName}` }).click();
    await expect(page.getByRole('dialog')).toContainText(appName);
    await page.getByRole('button', { name: 'Delete key' }).click();
    await expect(page.getByRole('row').filter({ hasText: appName })).toHaveCount(0);
    const deletedKey = await page.request.get('/Users/Me', {
      headers: { Authorization: tokenHeader(rawKey) },
    });
    expect(deletedKey.status()).toBe(401);

    expect(diagnostics.pageErrors, 'page errors').toEqual([]);
    expect(diagnostics.consoleErrors, 'console errors').toEqual([]);
    expect(diagnostics.failedRequests, 'failed same-origin requests').toEqual([]);
  });
});

function tokenHeader(token: string): string {
  return `MediaBrowser Token="${token}"`;
}

function secondaryIdentity(): string {
  return `MediaBrowser Client="TJXY E2E", Device="Secondary browser", DeviceId="${secondaryDeviceId}", Version="0.1.0"`;
}
