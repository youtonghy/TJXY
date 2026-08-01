import { expect, test, type Page } from '@playwright/test';

import { installAdminFixtures, installLoginFixtures } from './adminFixtures';
import { assertUniqueH1, login } from './support';

test('serves the system mark on both sign-in surfaces', async ({ context, page }) => {
  const fixtures = await installLoginFixtures(context);

  await page.goto('/admin/login');
  await expect(page.locator('img[src*="tjxy-mark"]')).toHaveJSProperty('naturalWidth', 512);

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
  await expect(page.getByRole('heading', { level: 1, name: 'Dashboard' })).toBeVisible();
  await assertUniqueH1(page);
  fixtures.assertComplete();
});

test('restores a deep link including its search parameters after login', async ({ context, page }) => {
  const fixtures = await installLoginFixtures(context);
  await installSuccessfulLogin(page);
  await page.goto('/admin/access?tab=api-keys');
  await expect(page).toHaveURL(/\/admin\/login$/);
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
  await expect(page).toHaveURL(/\/admin\/login$/);
  await expect.poll(async () => page.evaluate(() => sessionStorage.getItem('tjxy.admin.token')))
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
  await expect.poll(async () => page.evaluate(() => sessionStorage.getItem('tjxy.admin.token')))
    .not.toBeNull();
  await page.getByRole('button', { name: 'Sign out' }).click();
  await expect(page).toHaveURL(/\/admin\/login$/);
  await expect.poll(async () => page.evaluate(() => sessionStorage.getItem('tjxy.admin.token')))
    .toBeNull();
  fixtures.assertComplete();
});

test('reports unavailable readiness without blocking sign in', async ({ context, page }) => {
  const fixtures = await installLoginFixtures(context);
  await page.route('**/health/ready', async (route) => {
    await route.fulfill({ body: 'not ready', contentType: 'text/plain', status: 503 });
  });
  await page.goto('/admin/login');
  await expect(page.getByRole('status')).toContainText('Server unavailable');
  await expect(page.getByRole('button', { name: 'Sign in' })).toBeEnabled();
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
