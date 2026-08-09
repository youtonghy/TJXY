import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { AuthProvider } from 'ra-core';
import { Route, Routes, useLocation } from 'react-router-dom';

import { ApiError } from '../api/httpClient';
import { clearSession, setAccessToken } from '../auth/session';
import { renderWithAdmin } from '../test/renderWithAdmin';
import { AdminRouteGuard } from './AdminRouteGuard';

function LocationProbe() {
  const location = useLocation();
  const state = location.state as { nextPathname?: string; nextSearch?: string } | null;
  return (
    <output aria-label="Current location">
      {location.pathname}|{state?.nextPathname ?? ''}|{state?.nextSearch ?? ''}
    </output>
  );
}

function GuardRoutes() {
  return (
    <Routes>
      <Route path="/admin/login" element={<LocationProbe />} />
      <Route
        path="*"
        element={(
          <AdminRouteGuard>
            <div>Protected admin content</div>
          </AdminRouteGuard>
        )}
      />
    </Routes>
  );
}

function provider(overrides: Partial<AuthProvider>): AuthProvider {
  return {
    login: () => Promise.resolve(undefined),
    logout: () => Promise.resolve(undefined),
    checkAuth: () => Promise.resolve(undefined),
    checkError: () => Promise.resolve(undefined),
    ...overrides,
  };
}

describe('AdminRouteGuard', () => {
  it('shows pending auth before rendering authenticated children', async () => {
    let resolveAuth: (() => void) | undefined;
    const checkAuth = vi.fn(() => new Promise<void>((resolve) => { resolveAuth = resolve; }));
    renderWithAdmin(<GuardRoutes />, { authProvider: provider({ checkAuth }) });

    expect(screen.getByRole('heading', { name: 'Preparing the admin workspace' })).toBeVisible();
    expect(screen.queryByText('Protected admin content')).not.toBeInTheDocument();

    resolveAuth?.();
    expect(await screen.findByText('Protected admin content')).toBeVisible();
  });

  it('logs out a rejected 401 and preserves the deep-link destination for login', async () => {
    setAccessToken('stored-token');
    const logout = vi.fn(() => {
      clearSession();
      return Promise.resolve();
    });
    renderWithAdmin(<GuardRoutes />, {
      initialEntries: ['/admin/tasks?view=recent'],
      authProvider: provider({
        checkAuth: () => Promise.reject(
          new ApiError(401, 'authentication', 'Session invalid.'),
        ),
        logout,
      }),
    });

    await waitFor(() => {
      expect(screen.getByRole('status', { name: 'Current location' })).toHaveTextContent(
        '/admin/login|/admin/tasks|?view=recent',
      );
    });
    expect(logout).toHaveBeenCalledOnce();
    expect(sessionStorage.getItem('tjxy.web.token')).toBeNull();
  });

  it('preserves a stored session on 403 until explicit sign out', async () => {
    const user = userEvent.setup();
    setAccessToken('stored-token');
    const logout = vi.fn(() => {
      clearSession();
      return Promise.resolve();
    });
    renderWithAdmin(<GuardRoutes />, {
      authProvider: provider({
        checkAuth: () => Promise.reject(new ApiError(403, 'authorization', 'Forbidden.')),
        logout,
      }),
    });

    expect(await screen.findByRole('heading', { name: 'Access denied' })).toBeVisible();
    expect(logout).not.toHaveBeenCalled();
    expect(sessionStorage.getItem('tjxy.web.token')).toBe('stored-token');

    await user.click(screen.getByRole('button', { name: 'Sign out' }));
    await waitFor(() => {
      expect(logout).toHaveBeenCalledOnce();
    });
    expect(sessionStorage.getItem('tjxy.web.token')).toBeNull();
  });
});
