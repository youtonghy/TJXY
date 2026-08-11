import { expect, test, type Page } from '@playwright/test';

import { fixtureIds, installAdminFixtures, installLoginFixtures } from './adminFixtures';
import {
  assertActionControlsDoNotIntersect,
  assertNoHorizontalOverflow,
  assertUniqueH1,
} from './support';

const viewports = [
  { height: 900, label: '1440', width: 1440 },
  { height: 1024, label: '768', width: 768 },
  { height: 844, label: '390', width: 390 },
] as const;

const routes = [
  { h1: 'Users', name: 'users', path: '/admin/users' },
  { h1: 'Create user', name: 'user-create', path: '/admin/users/create' },
  {
    h1: 'Fixture Administrator',
    name: 'user-show',
    path: `/admin/users/${fixtureIds.adminId}/show`,
  },
  { h1: 'Edit Fixture Administrator', name: 'user-edit', path: `/admin/users/${fixtureIds.adminId}` },
  { h1: 'Access', name: 'access-devices', path: '/admin/access' },
  { h1: 'Access', name: 'access-api-keys', path: '/admin/access?tab=api-keys' },
  { h1: 'Tasks', name: 'tasks', path: '/admin/tasks' },
  { h1: 'Libraries', name: 'libraries', path: '/admin/libraries' },
  {
    h1: 'International Film Archive With A Deliberately Long Operational Name',
    name: 'library-edit',
    path: `/admin/libraries/${fixtureIds.libraryId}`,
  },
  { h1: 'Google Drive', name: 'google-drive', path: '/admin/storage/google-drive' },
  { h1: 'OneDrive', name: 'onedrive', path: '/admin/storage/onedrive' },
  { h1: 'Authentication required', name: 'authentication-error', path: '/admin/authentication-error' },
  { h1: 'Access denied', name: 'access-denied', path: '/admin/access-denied' },
  { h1: 'Page not found', name: 'not-found', path: '/admin/not-a-route' },
] as const;

for (const viewport of viewports) {
  test(`login visual at ${viewport.label}px`, async ({ context, page }) => {
    const fixtures = await installLoginFixtures(context);
    await page.setViewportSize(viewport);
    await page.goto('/app/login');
    await expect(page.getByRole('heading', { level: 1, name: /Welcome back|欢迎回来/u })).toBeVisible();
    await assertPageLayout(page);
    await expect(page).toHaveScreenshot(`login-${viewport.label}.png`, screenshotOptions);
    fixtures.assertComplete();
  });

  for (const route of routes) {
    test(`${route.name} visual at ${viewport.label}px`, async ({ context, page }) => {
      const fixtures = await installAdminFixtures(context);
      await page.setViewportSize(viewport);
      await page.goto(route.path);
      await expect(page.getByRole('heading', { level: 1, name: route.h1 })).toBeVisible();
      await assertUniqueH1(page);
      await assertResponsiveRepresentation(page, route.name, viewport.width);
      await assertRouteReady(page, route.name, viewport.width);
      await assertPageLayout(page);
      await expect(page).toHaveScreenshot(
        `${route.name}-${viewport.label}.png`,
        screenshotOptions,
      );
      fixtures.assertComplete();
    });
  }
}

test('mobile navigation drawer visual', async ({ context, page }) => {
  const fixtures = await installAdminFixtures(context);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/admin/users');
  await page.getByRole('button', { name: 'Open navigation' }).click();
  await expect(page.getByRole('dialog', { name: 'Navigation' })).toBeVisible();
  await expect(page).toHaveScreenshot('navigation-drawer-390.png', screenshotOptions);
  fixtures.assertComplete();
});

test('library delete confirmation visual', async ({ context, page }) => {
  const fixtures = await installAdminFixtures(context);
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(`/admin/libraries/${fixtureIds.libraryId}`);
  await page.getByRole('button', { name: 'Delete library' }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Delete International Film Archive With A Deliberately Long Operational Name?',
  });
  await expect(dialog.getByRole('button', { name: 'Cancel' })).toBeFocused();
  await expect(page).toHaveScreenshot('library-delete-confirmation-1440.png', screenshotOptions);
  fixtures.assertComplete();
});

test('blocked OAuth popup visual', async ({ context, page }) => {
  await page.addInitScript(() => {
    window.open = () => null;
  });
  const fixtures = await installAdminFixtures(context);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/admin/storage/google-drive');
  await chooseTargetLibrary(page);
  await page.getByRole('button', { name: 'Authorize Google Drive' }).click();
  await expect(page.getByText('The authorization window was blocked')).toBeVisible();
  await assertPageLayout(page);
  await expect(page).toHaveScreenshot('google-popup-blocked-390.png', screenshotOptions);
  fixtures.assertComplete();
});

const screenshotOptions = {
  animations: 'disabled' as const,
  caret: 'hide' as const,
  fullPage: true,
};

async function assertPageLayout(page: Page) {
  await page.evaluate(async () => { await document.fonts.ready; });
  await assertNoHorizontalOverflow(page);
  await assertActionControlsDoNotIntersect(page);
}

async function assertResponsiveRepresentation(page: Page, name: string, width: number) {
  const collection = responsiveCollection(name);
  if (collection === null) return;
  const region = page.getByRole('region', { name: collection.region });
  if (width < 640) {
    await expect(region.getByRole('list', { name: collection.mobile })).toBeVisible();
    await expect(region.getByRole('grid', { name: collection.desktop })).toHaveCount(0);
  } else {
    await expect(region.getByRole('grid', { name: collection.desktop })).toBeVisible();
    await expect(region.getByRole('list', { name: collection.mobile })).toHaveCount(0);
  }
}

async function assertRouteReady(page: Page, name: string, width: number) {
  const collection = responsiveCollection(name);
  if (collection !== null) {
    const region = page.getByRole('region', { name: collection.region });
    if (width < 640) {
      await expect(region.getByRole('listitem').first()).toBeVisible();
    } else {
      await expect(region.getByRole('rowheader').first()).toBeVisible();
    }
    return;
  }

  switch (name) {
    case 'user-create':
      await expect(page.getByLabel('Name')).toBeVisible();
      break;
    case 'user-show':
      await expect(page.getByRole('group', { name: 'User details' })).toBeVisible();
      break;
    case 'user-edit':
      await expect(page.getByLabel(/^New password/u)).toBeVisible();
      break;
    case 'tasks':
      await expect(page.getByRole('list', { name: 'Scheduled tasks' }).getByRole('listitem').first())
        .toBeVisible();
      await expect(page.getByRole('grid', { name: 'Recent durable jobs' }).getByRole('rowheader').first())
        .toBeVisible();
      break;
    case 'google-drive':
    case 'onedrive':
      await expect(page.getByRole('button', { name: /Target library/u })).toBeVisible();
      break;
    case 'authentication-error':
      await expect(page.getByRole('link', { name: 'Go to sign in' })).toBeVisible();
      break;
    case 'access-denied':
      await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();
      break;
    case 'not-found':
      await expect(page.getByRole('link', { name: 'Back to Users' })).toBeVisible();
      break;
    default:
      break;
  }
}

function responsiveCollection(name: string) {
  switch (name) {
    case 'users':
      return { desktop: 'Users', mobile: 'Users mobile', region: 'Users collection' };
    case 'access-devices':
      return { desktop: 'Devices', mobile: 'Devices mobile', region: 'Devices collection' };
    case 'access-api-keys':
      return { desktop: 'API Keys', mobile: 'API Keys mobile', region: 'API keys collection' };
    case 'libraries':
      return { desktop: 'Libraries', mobile: 'Libraries mobile', region: 'Libraries collection' };
    default:
      return null;
  }
}

async function chooseTargetLibrary(page: Page) {
  await page.getByRole('button', { name: /Target library/u }).click();
  await page.getByRole('option', {
    name: 'International Film Archive With A Deliberately Long Operational Name',
  }).click();
}
