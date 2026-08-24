import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';

import { SystemLocaleProvider } from '../../settings/SystemLocaleProvider';
import { ClientThemeRuntime } from '../themes/ThemeRuntime';
import { ClientLoginPage } from './ClientLoginPage';

const languageApi = vi.hoisted(() => ({
  getSystemLanguage: vi.fn(),
}));
const settingsApi = vi.hoisted(() => ({
  getPublicSystemSettings: vi.fn(),
}));
const clientAuth = vi.hoisted(() => ({
  adoptAuthentication: vi.fn(),
  signIn: vi.fn(),
  signInWithPasskey: vi.fn(),
}));

vi.mock('../../settings/systemLanguageApi', () => ({
  getSystemLanguage: languageApi.getSystemLanguage,
  saveSystemLanguage: vi.fn(),
}));

vi.mock('../../settings/systemSettingsApi', () => ({
  getPublicSystemSettings: settingsApi.getPublicSystemSettings,
}));

vi.mock('./ClientAuthContext', () => ({
  useClientAuth: () => ({ ...clientAuth, isLoading: false, user: null }),
}));

beforeEach(() => {
  vi.unstubAllEnvs();
  clientAuth.adoptAuthentication.mockReset().mockResolvedValue(undefined);
  clientAuth.signIn.mockReset().mockResolvedValue(undefined);
  clientAuth.signInWithPasskey.mockReset().mockResolvedValue(undefined);
  languageApi.getSystemLanguage.mockReset();
  languageApi.getSystemLanguage.mockResolvedValue({ locale: 'en-US', revision: 1, supportedLocales: ['zh-CN', 'en-US'] });
  settingsApi.getPublicSystemSettings.mockReset().mockResolvedValue({
    locale: 'en-US', siteTitle: 'TJXY', siteSubtitle: 'Your media library',
    logoUrl: '/brand/tjxy-mark.webp', iconUrl: '/brand/favicon.svg', publicUrl: '',
    listenHost: '127.0.0.1', port: 8096, revision: 0, restartRequired: false,
    environmentOverrides: { siteTitle: false, publicUrl: false, listenAddress: false },
    passkeyEnabled: false,
    theme: { id: 'classic', schemaVersion: 1, options: {}, revision: 0 },
  });
  window.localStorage.removeItem('tjxy.web.rememberCredentials');
  window.localStorage.removeItem('tjxy.web.savedUsername');
  window.localStorage.removeItem('tjxy-device-locale');
  window.localStorage.setItem('tjxy-system-locale', 'en-US');
});

it('does not show server address settings in the web application', async () => {
  window.localStorage.setItem('tjxy.api.baseUrl', 'http://old-server.example:8096');
  render(
    <SystemLocaleProvider><ClientThemeRuntime><MemoryRouter initialEntries={['/app/login']}>
      <ClientLoginPage />
    </MemoryRouter></ClientThemeRuntime></SystemLocaleProvider>,
  );

  expect(await screen.findByRole('heading', { name: 'Welcome back' })).toBeVisible();
  expect(screen.queryByRole('textbox', { name: /Server address/ })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Save server' })).not.toBeInTheDocument();
});

it('uses the shared system mark in the client sign-in brand', async () => {
  const { container } = render(
    <SystemLocaleProvider><ClientThemeRuntime><MemoryRouter initialEntries={['/app/login']}>
      <ClientLoginPage />
    </MemoryRouter></ClientThemeRuntime></SystemLocaleProvider>,
  );

  expect(await screen.findByText('Your media library')).toBeVisible();
  expect(container.querySelector('img')).toHaveAttribute('src', '/brand/tjxy-mark.webp');
});

it('places the inline language selector at the top right and updates the login copy', async () => {
  const user = userEvent.setup();
  render(
    <SystemLocaleProvider>
      <ClientThemeRuntime><MemoryRouter initialEntries={['/app/login']}>
        <ClientLoginPage />
      </MemoryRouter></ClientThemeRuntime>
    </SystemLocaleProvider>,
  );

  const selector = await screen.findByRole('button', { name: /Interface language/ });
  expect(selector.closest('.absolute')).toHaveClass('right-5', 'top-5');
  await user.click(selector);
  await user.click(await screen.findByRole('option', { name: '中文' }));
  expect(await screen.findByRole('heading', { name: '欢迎回来' })).toBeVisible();
});

it('uses the entered username for Passkey sign-in', async () => {
  const user = userEvent.setup();
  settingsApi.getPublicSystemSettings.mockResolvedValueOnce({
    locale: 'en-US', siteTitle: 'TJXY', siteSubtitle: 'Your media library',
    logoUrl: '/brand/tjxy-mark.webp', iconUrl: '/brand/favicon.svg', publicUrl: '',
    listenHost: '127.0.0.1', port: 8096, revision: 0, restartRequired: false,
    environmentOverrides: { siteTitle: false, publicUrl: false, listenAddress: false },
    passkeyEnabled: true,
    theme: { id: 'classic', schemaVersion: 1, options: {}, revision: 0 },
  });
  render(
    <SystemLocaleProvider><ClientThemeRuntime><MemoryRouter initialEntries={['/app/login']}>
      <ClientLoginPage />
    </MemoryRouter></ClientThemeRuntime></SystemLocaleProvider>,
  );

  await user.type(await screen.findByRole('textbox', { name: 'Username' }), ' Alice ');
  await user.click(await screen.findByRole('button', { name: 'Sign in with Passkey' }));

  expect(clientAuth.signInWithPasskey).toHaveBeenCalledWith('Alice');
});

it('shows a Passkey-specific error when credential selection fails', async () => {
  const user = userEvent.setup();
  settingsApi.getPublicSystemSettings.mockResolvedValueOnce({
    locale: 'en-US', siteTitle: 'TJXY', siteSubtitle: 'Your media library',
    logoUrl: '/brand/tjxy-mark.webp', iconUrl: '/brand/favicon.svg', publicUrl: '',
    listenHost: '127.0.0.1', port: 8096, revision: 0, restartRequired: false,
    environmentOverrides: { siteTitle: false, publicUrl: false, listenAddress: false },
    passkeyEnabled: true,
    theme: { id: 'classic', schemaVersion: 1, options: {}, revision: 0 },
  });
  clientAuth.signInWithPasskey.mockRejectedValueOnce(new Error('failed'));
  render(
    <SystemLocaleProvider><ClientThemeRuntime><MemoryRouter initialEntries={['/app/login']}>
      <ClientLoginPage />
    </MemoryRouter></ClientThemeRuntime></SystemLocaleProvider>,
  );

  await user.click(await screen.findByRole('button', { name: 'Sign in with Passkey' }));

  expect(clientAuth.signInWithPasskey).toHaveBeenCalledWith(undefined);
  expect(await screen.findByText('Passkey sign-in failed')).toBeVisible();
  expect(screen.queryByText('Check your username and password.')).not.toBeInTheDocument();
});
