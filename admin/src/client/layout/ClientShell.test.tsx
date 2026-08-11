import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { ClientShell } from './ClientShell';
import { getAiModels } from '../ai/aiApi';
import { SystemLocaleProvider } from '../../settings/SystemLocaleProvider';
import { ClientThemeRuntime } from '../themes/ThemeRuntime';

vi.mock('../ai/aiApi', () => ({ getAiModels: vi.fn() }));
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

const getAiModelsMock = vi.mocked(getAiModels);

vi.mock('../auth/ClientAuthContext', () => ({
  useClientAuth: () => ({
    signOut: vi.fn(),
    user: { Id: 'user-1', Name: 'Admin' },
  }),
}));

beforeEach(() => {
  getAiModelsMock.mockReset();
  getAiModelsMock.mockResolvedValue([{ id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11', displayName: 'Cinema Guide', isDefault: true }]);
  localStorage.clear();
  localStorage.setItem('tjxy-system-locale', 'en-US');
  document.documentElement.classList.remove('dark', 'light');
  document.documentElement.removeAttribute('data-theme');
});

it('persists the selected color theme and exposes the next theme as its action', async () => {
  const user = userEvent.setup();
  renderShell('/app/libraries');

  const themeToggle = await screen.findByRole('button', { name: 'Switch to dark mode' });
  expect(document.documentElement).toHaveAttribute('data-theme', 'light');

  await user.click(themeToggle);

  expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
  expect(document.documentElement).toHaveClass('dark');
  expect(localStorage.getItem('tjxy-color-theme')).toBe('dark');
  expect(screen.getByRole('button', { name: 'Switch to light mode' })).toBeVisible();
});

it('presents the same primary destinations in the top bar and mobile navigation', async () => {
  const user = userEvent.setup();
  renderShell('/app/libraries');

  const topNavigation = await screen.findByRole('navigation', { name: 'TJXY navigation' });
  expect(topNavigation).toBeVisible();
  expect(within(topNavigation).getByRole('link', { name: 'Libraries' })).toHaveAttribute('aria-current', 'page');

  await user.click(screen.getByRole('button', { name: 'Open navigation' }));

  const mobileNavigation = await screen.findByRole('navigation', { name: 'Mobile navigation' });
  expect(mobileNavigation).toHaveTextContent('Home');
  expect(mobileNavigation).toHaveTextContent('Libraries');
  expect(mobileNavigation).toHaveTextContent('Search');
  expect(await within(mobileNavigation).findByRole('link', { name: 'AI assistant' })).toBeInTheDocument();
  expect(within(mobileNavigation).getByRole('link', { name: 'Libraries' })).toHaveAttribute('aria-current', 'page');
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});

it('exposes configured AI, rankings, and a profile destination from the account menu', async () => {
  const user = userEvent.setup();
  renderShell('/app/');

  expect(await screen.findByRole('link', { name: 'Rankings' })).toHaveAttribute('href', '/app/rankings');
  expect(await screen.findByRole('link', { name: 'AI assistant' })).toHaveAttribute('href', '/app/ai');
  await user.click(screen.getByRole('button', { name: 'Open account menu for Admin' }));

  expect(await screen.findByRole('menuitem', { name: 'Profile & stats' })).toBeVisible();
});

it('hides the AI assistant destination when no model is configured', async () => {
  getAiModelsMock.mockResolvedValue([]);
  renderShell('/app/');

  expect(await screen.findByRole('link', { name: 'Rankings' })).toBeVisible();
  expect(screen.queryByRole('link', { name: 'AI assistant' })).not.toBeInTheDocument();
});

it('uses the shared system mark for the home brand link', async () => {
  renderShell('/app/');

  const brand = await screen.findByRole('link', { name: 'TJXY home' });
  expect(brand.querySelector('img')).toHaveAttribute('src', '/brand/tjxy-mark.webp');
});

it('groups the header controls in the HeroUI Pro navigation landmark', async () => {
  renderShell('/app/rankings');

  const navigation = await screen.findByRole('navigation', { name: 'TJXY navigation' });
  expect(within(navigation).getByRole('link', { name: 'Rankings' })).toHaveAttribute('aria-current', 'page');
  expect(within(navigation).getByRole('button', { name: 'Switch to dark mode' })).toBeVisible();
  expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
});

function renderShell(path: string) {
  return render(
    <SystemLocaleProvider><ClientThemeRuntime><MemoryRouter initialEntries={[path]}>
      <ClientShell>
        <p>Page content</p>
      </ClientShell>
    </MemoryRouter></ClientThemeRuntime></SystemLocaleProvider>,
  );
}
