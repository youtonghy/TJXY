import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { AuthProvider } from 'ra-core';

import { renderWithAdmin } from '../test/renderWithAdmin';
import { AdminShell } from './AdminShell';

function shellProvider(overrides: Partial<AuthProvider> = {}): AuthProvider {
  return {
    login: () => Promise.resolve(undefined),
    logout: () => Promise.resolve(undefined),
    checkAuth: () => Promise.resolve(undefined),
    checkError: () => Promise.resolve(undefined),
    getIdentity: () => Promise.resolve({ id: 'ada-id', fullName: '  Ada' }),
    ...overrides,
  };
}

describe('AdminShell', () => {
  it('renders grouped desktop navigation in the approved order with current route state', async () => {
    renderWithAdmin(<AdminShell><h1>Task workspace</h1></AdminShell>, {
      initialEntries: ['/admin/tasks'],
      authProvider: shellProvider(),
    });

    const navigation = screen.getByRole('navigation', { name: 'Primary' });
    expect(within(navigation).getAllByText(/Manage|Operations|Storage|System/u)).toHaveLength(5);
    expect(within(navigation).getAllByRole('link').map((link) => link.textContent.trim())).toEqual([
      'Dashboard',
      'Users',
      'Access',
      'Libraries',
      'Announcements',
      'Tasks',
      'Logs',
      'Google Drive',
      'OneDrive',
      'Metadata',
      'AI assistant',
      'Themes',
      'System settings',
      'About',
    ]);
    expect(within(navigation).getByRole('link', { name: 'Tasks' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(within(navigation).getByRole('link', { name: 'Dashboard' })).not.toHaveAttribute(
      'aria-current',
    );
    expect(screen.getByRole('main')).toHaveAttribute('id', 'main-content');
    const brandLinks = screen.getAllByRole('link', { name: 'TJXY Admin home' });
    expect(brandLinks).toHaveLength(2);
    for (const link of brandLinks) {
      expect(link.querySelector('img')).toHaveAttribute('src', '/brand/tjxy-mark.webp');
    }
    expect(await screen.findByText('Ada')).toBeVisible();
    expect(screen.getByText('A')).toBeVisible();
  });

  it('opens mobile navigation, closes after navigation, and restores trigger focus', async () => {
    const user = userEvent.setup();
    renderWithAdmin(<AdminShell><h1>Task workspace</h1></AdminShell>, {
      initialEntries: ['/admin/tasks'],
      authProvider: shellProvider(),
    });
    const trigger = screen.getByRole('button', { name: 'Open navigation' });
    expect(trigger).toHaveClass('size-10');

    await user.click(trigger);
    const drawer = await screen.findByRole('dialog', { name: 'Navigation' });
    expect(within(drawer).getByRole('link', { name: 'Access' })).toBeVisible();
    expect(screen.getAllByText('TJXY Admin')).toHaveLength(2);

    await user.click(within(drawer).getByRole('link', { name: 'Access' }));
    await waitFor(() => {
      expect(screen.queryByRole('dialog', { name: 'Navigation' })).not.toBeInTheDocument();
    });
    expect(trigger).toHaveFocus();
  });

  it('focuses main content from the skip link and signs out from the account menu', async () => {
    const user = userEvent.setup();
    const logout = vi.fn().mockResolvedValue(undefined);
    renderWithAdmin(<AdminShell><h1>Users</h1></AdminShell>, {
      authProvider: shellProvider({ logout }),
    });

    await user.click(screen.getByRole('link', { name: 'Skip to content' }));
    expect(screen.getByRole('main')).toHaveFocus();

    await user.click(await screen.findByRole('button', { name: 'Open account menu for Ada' }));
    await user.click(await screen.findByRole('menuitem', { name: 'Sign out' }));
    expect(logout).toHaveBeenCalledOnce();
  });

  it('renders identity failure as an explicit auth error instead of an empty avatar', async () => {
    renderWithAdmin(<AdminShell><h1>Users</h1></AdminShell>, {
      authProvider: shellProvider({
        getIdentity: () => Promise.reject(new ApiErrorForTest()),
      }),
    });

    expect(await screen.findByRole('alert')).toHaveTextContent('Administrator identity unavailable');
    expect(screen.queryByLabelText('Administrator avatar')).not.toBeInTheDocument();
  });
});

class ApiErrorForTest extends Error {
  status = 401;
}
