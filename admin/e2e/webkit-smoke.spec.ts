import { expect, test } from '@playwright/test';

import { assertNoHorizontalOverflow, assertUniqueH1, login } from './support';

test('WebKit supports login, navigation, a Users workflow, modal focus, and logout', async ({ page }) => {
  await page.goto('/admin/users');
  await login(page, 'Admin', 'admin-password');
  await expect(page.getByRole('heading', { level: 1, name: 'Users' })).toBeVisible();
  await assertUniqueH1(page);

  await page.getByRole('link', { name: 'Access' }).click();
  await page.getByRole('tab', { name: 'API Keys' }).click();
  await expect(page).toHaveURL(/\/admin\/access\?tab=api-keys$/);
  await page.getByRole('link', { name: 'Users' }).click();

  await page.getByRole('link', { name: 'Create user' }).click();
  await page.getByRole('textbox', { name: 'Name' }).fill('WebKit Operator');
  await page.getByLabel('Initial password').fill('webkit-operator-password');
  await page.getByRole('button', { name: 'Create user' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'WebKit Operator' })).toBeVisible();
  await page.getByRole('link', { name: 'Edit user' }).click();

  const deleteTrigger = page.getByRole('button', { name: 'Delete user' });
  await deleteTrigger.click();
  const dialog = page.getByRole('dialog', { name: 'Delete WebKit Operator?' });
  const cancel = dialog.getByRole('button', { name: 'Cancel' });
  await expect(cancel).toBeFocused();
  await cancel.click();
  await expect(deleteTrigger).toBeFocused();

  await deleteTrigger.click();
  await dialog.getByRole('button', { name: 'Delete user' }).click();
  await expect(page).toHaveURL(/\/admin\/users$/);
  await assertNoHorizontalOverflow(page);

  await page.getByRole('button', { name: 'Open account menu for Admin' }).click();
  await page.getByRole('menuitem', { name: 'Sign out' }).click();
  await expect(page).toHaveURL(/\/admin\/login$/);
});
