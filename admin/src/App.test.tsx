import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { App } from './App';

const authControl = vi.hoisted(() => {
  const state = { authenticated: true };
  const login = vi.fn(() => {
    state.authenticated = true;
    return Promise.resolve();
  });
  const logout = vi.fn(() => {
    state.authenticated = false;
    sessionStorage.clear();
    return Promise.resolve();
  });
  const checkAuth = vi.fn(() => (
    state.authenticated
      ? Promise.resolve()
      : Promise.reject(Object.assign(new Error('Authentication required'), { status: 401 }))
  ));
  return { state, login, logout, checkAuth };
});

const pageControl = vi.hoisted(() => ({ throwUsers: false }));

vi.mock('./auth/authProvider', () => ({
  authProvider: {
    login: authControl.login,
    logout: authControl.logout,
    checkAuth: authControl.checkAuth,
    checkError: () => Promise.resolve(),
    getIdentity: () => Promise.resolve({ id: 'admin-id', fullName: 'Admin' }),
    getPermissions: () => Promise.resolve('administrator'),
  },
}));

vi.mock('./users/UserList', async () => {
  const React = await import('react');
  return {
    UserList: () => {
      if (pageControl.throwUsers) throw new Error('private render stack');
      return React.createElement('h1', null, 'Users page');
    },
  };
});
vi.mock('./users/UserCreate', async () => {
  const React = await import('react');
  return { UserCreate: () => React.createElement('h1', null, 'Create user page') };
});
vi.mock('./users/UserEdit', async () => {
  const React = await import('react');
  return { UserEdit: () => React.createElement('h1', null, 'Edit user page') };
});
vi.mock('./users/UserShow', async () => {
  const React = await import('react');
  return { UserShow: () => React.createElement('h1', null, 'User details page') };
});
vi.mock('./access/AccessPage', async () => {
  const React = await import('react');
  return { AccessPage: () => React.createElement('h1', null, 'Access page') };
});
vi.mock('./tasks/TasksPage', async () => {
  const React = await import('react');
  return { TasksPage: () => React.createElement('h1', null, 'Tasks page') };
});
vi.mock('./libraries/LibrariesPage', async () => {
  const React = await import('react');
  return { LibrariesPage: () => React.createElement('h1', null, 'Libraries page') };
});
vi.mock('./libraries/LibraryEditPage', async () => {
  const React = await import('react');
  return { LibraryEditPage: () => React.createElement('h1', null, 'Library edit page') };
});
vi.mock('./storage/GoogleDrivePage', async () => {
  const React = await import('react');
  return { GoogleDrivePage: () => React.createElement('h1', null, 'Google Drive page') };
});
vi.mock('./storage/OneDrivePage', async () => {
  const React = await import('react');
  return { OneDrivePage: () => React.createElement('h1', null, 'OneDrive page') };
});
vi.mock('./settings/MetadataSettingsPage', async () => {
  const React = await import('react');
  return { MetadataSettingsPage: () => React.createElement('h1', null, 'Metadata page') };
});
vi.mock('./settings/AiSettingsPage', async () => {
  const React = await import('react');
  return { AiSettingsPage: () => React.createElement('h1', null, 'AI settings page') };
});
vi.mock('./dashboard/DashboardPage', async () => {
  const React = await import('react');
  return { DashboardPage: () => React.createElement('h1', null, 'Dashboard page') };
});

beforeEach(() => {
  window.localStorage.setItem('tjxy-system-locale', 'en-US');
  authControl.state.authenticated = true;
  authControl.login.mockClear();
  authControl.logout.mockClear();
  authControl.checkAuth.mockClear();
  pageControl.throwUsers = false;
  window.history.pushState({}, '', '/admin/users');
});

function renderRoute(path: string, authenticated = true) {
  authControl.state.authenticated = authenticated;
  window.history.pushState({}, '', path);
  return render(<App />);
}

it('restores an anonymous deep link including search after login', async () => {
  renderRoute('/admin/tasks?view=recent', false);

  expect(await screen.findByRole('heading', { name: 'Welcome back' })).toBeVisible();
  expect(`${window.location.pathname}${window.location.search}`).toBe('/app/login?redirect=%2Fadmin%2Ftasks%3Fview%3Drecent');
});

it('redirects the legacy administrator login URL to the shared login', async () => {
  renderRoute('/admin/login', false);

  expect(await screen.findByRole('heading', { name: 'Welcome back' })).toBeVisible();
  expect(window.location.pathname).toBe('/app/login');
  expect(window.location.search).toBe('?redirect=%2Fadmin');
});

it('renders the ordinary HeroUI client without mounting the administrator shell', async () => {
  sessionStorage.clear();
  renderRoute('/app/login');

  expect(await screen.findByRole('heading', { name: 'Welcome back' })).toBeVisible();
  expect(screen.getByText('Your media library')).toBeVisible();
  expect(screen.queryByText('TJXY Admin')).not.toBeInTheDocument();
  expect(screen.queryByRole('navigation', { name: 'Primary' })).not.toBeInTheDocument();
});

it.each([
  ['/admin', 'Dashboard page'],
  ['/admin/access', 'Access page'],
  ['/admin/tasks', 'Tasks page'],
  ['/admin/libraries', 'Libraries page'],
  ['/admin/storage/google-drive', 'Google Drive page'],
  ['/admin/storage/onedrive', 'OneDrive page'],
  ['/admin/settings/metadata', 'Metadata page'],
  ['/admin/settings/ai', 'AI settings page'],
])('renders %s inside the guarded shell', async (path, heading) => {
  renderRoute(path);

  const main = await screen.findByRole('main');
  expect(await within(main).findByRole('heading', { name: heading })).toBeVisible();
  expect(screen.getByRole('navigation', { name: 'Primary' })).toBeVisible();
});

it.each([
  ['/admin/users/create', 'Create user page'],
  ['/admin/users/ada/show', 'User details page'],
  ['/admin/users/ada', 'Edit user page'],
  ['/admin/libraries/library-id', 'Library edit page'],
])('retains resource deep link %s', async (path, heading) => {
  renderRoute(path);
  expect(await screen.findByRole('heading', { name: heading })).toBeVisible();
  expect(window.location.pathname).toBe(path);
});

it.each([
  ['/admin/authentication-error', 'Authentication required'],
  ['/admin/access-denied', 'Access denied'],
  ['/admin/not-a-route', 'Page not found'],
])('renders the named system state for %s', async (path, heading) => {
  renderRoute(path);
  expect(await screen.findByRole('heading', { name: heading })).toBeVisible();
});

it('resets a top-level render failure without exposing its stack', async () => {
  const user = userEvent.setup();
  vi.spyOn(console, 'error').mockImplementation(() => undefined);
  pageControl.throwUsers = true;
  renderRoute('/admin/users');

  expect(await screen.findByRole('heading', { name: 'The admin interface could not continue' }))
    .toBeVisible();
  expect(screen.queryByText('private render stack')).not.toBeInTheDocument();
  pageControl.throwUsers = false;
  await user.click(screen.getByRole('button', { name: 'Try again' }));

  await waitFor(() => {
    expect(screen.getByRole('heading', { name: 'Users page' })).toBeVisible();
  });
});
