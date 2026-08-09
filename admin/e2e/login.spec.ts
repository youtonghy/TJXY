import { expect, test, type Page } from '@playwright/test';

import { installAdminFixtures, installLoginFixtures } from './adminFixtures';
import { assertUniqueH1, login } from './support';

test('redirects the legacy admin URL to the shared branded sign-in', async ({ context, page }) => {
  const fixtures = await installLoginFixtures(context);

  await page.goto('/admin/login');
  await expect(page.locator('img[src*="tjxy-mark"]')).toHaveJSProperty('naturalWidth', 512);
  await expect(page).toHaveURL(/\/app\/login\?redirect=%2Fadmin$/);

  await page.goto('/app/login');
  await expect(page.locator('img[src*="tjxy-mark"]')).toHaveJSProperty('naturalWidth', 512);
  fixtures.assertComplete();
});

test('uses Dashboard as the direct-login fallback', async ({ context, page }) => {
  const fixtures = await installLoginFixtures(context);
  await installSuccessfulLogin(page);
  await page.goto('/admin/login');
  await login(page, 'Fixture Administrator', 'fixture-password');
  await expect(page).toHaveURL(/\/admin$/);
  await expect(page.getByRole('heading', { level: 1, name: /Dashboard|仪表盘/u })).toBeVisible();
  await assertUniqueH1(page);
  fixtures.assertComplete();
});

test('restores a deep link including its search parameters after login', async ({ context, page }) => {
  const fixtures = await installLoginFixtures(context);
  await installSuccessfulLogin(page);
  await page.goto('/admin/access?tab=api-keys');
  await expect(page).toHaveURL(/\/app\/login\?redirect=%2Fadmin%2Faccess%3Ftab%3Dapi-keys$/);
  await login(page, 'Fixture Administrator', 'fixture-password');
  await expect(page).toHaveURL(/\/admin\/access\?tab=api-keys$/);
  await expect(page.getByRole('tab', { name: 'API Keys' })).toHaveAttribute('aria-selected', 'true');
  fixtures.assertComplete();
});

test('clears a rejected session after a 401', async ({ context, page }) => {
  const fixtures = await installAdminFixtures(context);
  await page.route('**/Users/Me', async (route) => {
    await route.fulfill({ body: '{}', contentType: 'application/json', status: 401 });
  });
  await page.goto('/admin/users');
  await expect(page).toHaveURL(/\/app\/login/);
  await expect.poll(async () => page.evaluate(() => sessionStorage.getItem('tjxy.web.token')))
    .toBeNull();
  fixtures.assertComplete();
});

test('preserves a 403 session on Access Denied until explicit sign out', async ({ context, page }) => {
  const fixtures = await installAdminFixtures(context);
  await page.route('**/Users/Me', async (route) => {
    await route.fulfill({ body: '{}', contentType: 'application/json', status: 403 });
  });
  await page.goto('/admin/users');
  await expect(page.getByRole('heading', { level: 1, name: 'Access denied' })).toBeVisible();
  await expect.poll(async () => page.evaluate(() => sessionStorage.getItem('tjxy.web.token')))
    .not.toBeNull();
  await page.getByRole('button', { name: 'Sign out' }).click();
  await expect(page).toHaveURL(/\/app\/login/);
  await expect.poll(async () => page.evaluate(() => sessionStorage.getItem('tjxy.web.token')))
    .toBeNull();
  fixtures.assertComplete();
});

test('legacy administrator login keeps the shared sign-in available when readiness fails', async ({ context, page }) => {
  const fixtures = await installLoginFixtures(context);
  await page.route('**/health/ready', async (route) => {
    await route.fulfill({ body: 'not ready', contentType: 'text/plain', status: 503 });
  });
  await page.goto('/admin/login');
  await expect(page.getByRole('heading', { name: /Welcome back|欢迎回来/u })).toBeVisible();
  await expect(page.locator('button[type="submit"]')).toBeEnabled();
  fixtures.assertComplete();
});

async function installSuccessfulLogin(page: Page) {
  await page.route('**/Users/AuthenticateByName', async (route) => {
    await route.fulfill({
      body: JSON.stringify({ AccessToken: 'fixture-login-session' }),
      contentType: 'application/json',
      status: 200,
    });
  });
}
