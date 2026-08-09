import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

import { fixtureIds, installAdminFixtures, installLoginFixtures } from './adminFixtures';
import { assertNoHorizontalOverflow, assertUniqueH1 } from './support';

const authenticatedRoutes = [
  { h1: 'Users', path: '/admin/users' },
  { h1: 'Create user', path: '/admin/users/create' },
  { h1: 'Fixture Administrator', path: `/admin/users/${fixtureIds.adminId}/show` },
  { h1: 'Edit Fixture Administrator', path: `/admin/users/${fixtureIds.adminId}` },
  { h1: 'Access', path: '/admin/access' },
  { h1: 'Access', path: '/admin/access?tab=api-keys' },
  { h1: 'Tasks', path: '/admin/tasks' },
  { h1: 'Libraries', path: '/admin/libraries' },
  {
    h1: 'International Film Archive With A Deliberately Long Operational Name',
    path: `/admin/libraries/${fixtureIds.libraryId}`,
  },
  { h1: 'Google Drive', path: '/admin/storage/google-drive' },
  { h1: 'OneDrive', path: '/admin/storage/onedrive' },
  { h1: 'Authentication required', path: '/admin/authentication-error' },
  { h1: 'Access denied', path: '/admin/access-denied' },
  { h1: 'Page not found', path: '/admin/not-a-route' },
] as const;

test('login has no WCAG A or AA violations', async ({ context, page }) => {
  const fixtures = await installLoginFixtures(context);
  await page.goto('/app/login');
  await expect(page.getByRole('heading', { level: 1, name: /Welcome back|欢迎回来/u })).toBeVisible();
  await assertAxe(page);
  fixtures.assertComplete();
});

for (const route of authenticatedRoutes) {
  test(`${route.path} has no WCAG A or AA violations`, async ({ context, page }) => {
    const fixtures = await installAdminFixtures(context);
    await page.goto(route.path);
    await expect(page.getByRole('heading', { level: 1, name: route.h1 })).toBeVisible();
    await assertUniqueH1(page);
    await assertAuthenticatedRouteReady(page, route.path);
    await assertAxe(page);
    fixtures.assertComplete();
  });
}

test('skip link, mobile drawer, reduced motion, and effective 200% viewport remain usable', async ({ context, page }) => {
  const fixtures = await installAdminFixtures(context);
  await page.setViewportSize({ width: 720, height: 450 });
  await page.goto('/admin/users');
  await expect(page.getByRole('heading', { level: 1, name: 'Users' })).toBeVisible();
  await expect(page.getByRole('grid', { name: 'Users' }).getByRole('rowheader').first()).toBeVisible();
  await page.keyboard.press('Tab');
  const skipLink = page.getByRole('link', { name: 'Skip to content' });
  await expect(skipLink).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('#main-content')).toBeFocused();
  expect(await page.evaluate(() => matchMedia('(prefers-reduced-motion: reduce)').matches)).toBe(true);
  await assertNoHorizontalOverflow(page);

  await page.setViewportSize({ width: 390, height: 844 });
  const trigger = page.getByRole('button', { name: 'Open navigation' });
  await trigger.click();
  await expect(page.getByRole('dialog', { name: 'Navigation' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(trigger).toBeFocused();
  fixtures.assertComplete();
});

async function assertAxe(page: Page) {
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .analyze();
  expect(results.violations).toEqual([]);
}

async function assertAuthenticatedRouteReady(page: Page, path: string) {
  if (path === '/admin/users') {
    await expect(page.getByRole('grid', { name: 'Users' }).getByRole('rowheader').first()).toBeVisible();
    return;
  }
  if (path === '/admin/users/create') {
    await expect(page.getByLabel('Name')).toBeVisible();
    return;
  }
  if (path.endsWith('/show')) {
    await expect(page.getByRole('group', { name: 'User details' })).toBeVisible();
    return;
  }
  if (path.startsWith('/admin/users/')) {
    await expect(page.getByLabel(/^New password/u)).toBeVisible();
    return;
  }
  if (path === '/admin/authentication-error') {
    await expect(page.getByRole('link', { name: 'Go to sign in' })).toBeVisible();
    return;
  }
  if (path === '/admin/access-denied') {
    await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();
    return;
  }
  if (path.startsWith('/admin/access')) {
    const collectionName = path.includes('tab=api-keys') ? 'API Keys' : 'Devices';
    await expect(page.getByRole('grid', { name: collectionName }).getByRole('rowheader').first())
      .toBeVisible();
    return;
  }
  if (path === '/admin/tasks') {
    await expect(page.getByRole('grid', { name: 'Recent durable jobs' }).getByRole('rowheader').first())
      .toBeVisible();
    return;
  }
  if (path === '/admin/libraries') {
    await expect(page.getByRole('grid', { name: 'Libraries' }).getByRole('rowheader').first())
      .toBeVisible();
    return;
  }
  if (path.startsWith('/admin/libraries/')) {
    await expect(page.getByRole('grid', { name: 'Pinned background candidates' }).getByRole('rowheader').first())
      .toBeVisible();
    return;
  }
  if (path.startsWith('/admin/storage/')) {
    await expect(page.getByRole('button', { name: /Target library/u })).toBeVisible();
    return;
  }
  await expect(page.getByRole('link', { name: 'Back to Users' })).toBeVisible();
}
