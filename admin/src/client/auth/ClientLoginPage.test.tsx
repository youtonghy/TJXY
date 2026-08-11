import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';

import { SystemLocaleProvider } from '../../settings/SystemLocaleProvider';
import { ClientThemeRuntime } from '../themes/ThemeRuntime';
import { ClientLoginPage } from './ClientLoginPage';

const languageApi = vi.hoisted(() => ({
  getSystemLanguage: vi.fn(),
}));

vi.mock('../../settings/systemLanguageApi', () => ({
  getSystemLanguage: languageApi.getSystemLanguage,
  saveSystemLanguage: vi.fn(),
}));

vi.mock('../../settings/systemSettingsApi', () => ({
  getPublicSystemSettings: vi.fn().mockResolvedValue({
    locale: 'en-US', siteTitle: 'TJXY', siteSubtitle: 'Your media library',
    logoUrl: '/brand/tjxy-mark.webp', iconUrl: '/brand/favicon.svg', publicUrl: '',
    listenHost: '127.0.0.1', port: 8096, mediaBrowserRoots: [],
    invalidMediaBrowserRootIndexes: [], revision: 0, restartRequired: false,
    environmentOverrides: { siteTitle: false, publicUrl: false, listenAddress: false, mediaBrowserRoots: false },
    theme: { id: 'classic', schemaVersion: 1, options: {}, revision: 0 },
  }),
}));

vi.mock('./ClientAuthContext', () => ({
  useClientAuth: () => ({ isLoading: false, signIn: vi.fn(), user: null }),
}));

beforeEach(() => {
  languageApi.getSystemLanguage.mockReset();
  languageApi.getSystemLanguage.mockResolvedValue({ locale: 'en-US', revision: 1, supportedLocales: ['zh-CN', 'en-US'] });
  window.localStorage.setItem('tjxy-system-locale', 'en-US');
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
