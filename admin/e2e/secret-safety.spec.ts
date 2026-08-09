import { expect, test, type Page } from '@playwright/test';

import playwrightConfig from '../playwright.config';
import { installAdminFixtures } from './adminFixtures';
import { login } from './support';

const adminPassword = 'admin-password';
const appName = 'Secret safety lifecycle';

test('all projects disable automatic sensitive artifacts', () => {
  expect(playwrightConfig.use?.trace).toBe('off');
  expect(playwrightConfig.use?.screenshot).toBe('off');
  expect(playwrightConfig.use?.video).toBe('off');
  for (const project of playwrightConfig.projects ?? []) {
    expect(project.use?.trace ?? 'off').toBe('off');
    expect(project.use?.screenshot ?? 'off').toBe('off');
    expect(project.use?.video ?? 'off').toBe('off');
  }
});

test('API key plaintext stays out of URLs, storage, console, and remounted panels', async ({ page }) => {
  const consoleMessages: string[] = [];
  page.on('console', (message) => { consoleMessages.push(message.text()); });

  await page.goto('/admin/access');
  await login(page, 'Admin', adminPassword);
  await page.getByRole('tab', { name: 'API Keys' }).click();
  await page.getByRole('button', { name: 'Create API key' }).click();
  await page.getByRole('textbox', { name: 'Application name' }).fill(appName);
  await page.getByRole('button', { name: 'Create key' }).click();

  const record = page.getByRole('grid', { name: 'API Keys' }).getByRole('row').filter({ hasText: appName });
  await record.getByRole('button', { name: `Show key for ${appName}` }).click();
  const rawKey = await record.getByLabel('Visible API key').innerText();
  expect(/^[0-9a-f]{64}$/u.test(rawKey), 'revealed API key uses the canonical format').toBe(true);
  await expect(record.getByLabel('Visible API key')).toBeVisible();
  await assertSecretAbsentFromPassiveSurfaces(page, rawKey, consoleMessages);

  await page.getByRole('tab', { name: 'Devices' }).click();
  await expect(page.locator('body')).not.toContainText(rawKey);
  await page.getByRole('tab', { name: 'API Keys' }).click();
  await expect(page.getByRole('grid', { name: 'API Keys' }).getByRole('row').filter({ hasText: appName }).locator('code'))
    .toHaveText('****************');
  await page.reload();
  await expect(page.getByRole('grid', { name: 'API Keys' }).getByRole('row').filter({ hasText: appName }).locator('code'))
    .toHaveText('****************');

  await page.getByRole('button', { name: `Delete key for ${appName}` }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Delete key' }).click();
  await expect(page.getByRole('grid', { name: 'API Keys' }).getByRole('row').filter({ hasText: appName }))
    .toHaveCount(0);
});

test('failed password submission exposes only safe feedback', async ({ page }) => {
  const submittedPassword = 'password-that-must-not-appear-outside-the-field';
  const consoleMessages: string[] = [];
  page.on('console', (message) => { consoleMessages.push(message.text()); });
  await page.goto('/app/login');
  await login(page, 'Admin', submittedPassword);
  const alert = page.getByRole('alert');
  await expect(alert).toContainText('Sign in failed');
  await expect(alert).not.toContainText(submittedPassword);
  expect(page.url().includes(submittedPassword)).toBe(false);
  expect(consoleMessages.some((message) => message.includes(submittedPassword))).toBe(false);
  expect(await storageContains(page, submittedPassword)).toBe(false);
});

test('OAuth navigation omits the admin-page Referer header', async ({ context, page }) => {
  const fixtures = await installAdminFixtures(context);
  await page.goto('/admin/storage/google-drive');
  await page.getByRole('button', { name: /Target library/u }).click();
  await page.getByRole('option', {
    name: 'International Film Archive With A Deliberately Long Operational Name',
  }).click();

  const popupPromise = page.waitForEvent('popup');
  await page.getByRole('button', { name: 'Authorize Google Drive' }).click();
  const popup = await popupPromise;
  await popup.waitForURL(/\/oauth-fixture\/authorize$/u);
  await expect.poll(() => fixtures.oauthReferrers.length).toBe(1);
  expect(fixtures.oauthReferrers).toEqual(['']);
  await popup.close();
  fixtures.assertComplete();
});

async function assertSecretAbsentFromPassiveSurfaces(
  page: Page,
  secret: string,
  consoleMessages: readonly string[],
) {
  expect(page.url().includes(secret)).toBe(false);
  expect(consoleMessages.some((message) => message.includes(secret))).toBe(false);
  expect(await storageContains(page, secret)).toBe(false);
}

async function storageContains(page: Page, secret: string): Promise<boolean> {
  return page.evaluate((candidate) => {
    for (const storage of [window.localStorage, window.sessionStorage]) {
      for (let index = 0; index < storage.length; index += 1) {
        const key = storage.key(index);
        if (key !== null && (key.includes(candidate) || (storage.getItem(key) ?? '').includes(candidate))) {
          return true;
        }
      }
    }
    return false;
  }, secret);
}
