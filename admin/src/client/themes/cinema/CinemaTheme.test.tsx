import { render, screen, within } from '@testing-library/react';
import { Home, Library } from 'lucide-react';
import { MemoryRouter } from 'react-router-dom';

import { SystemLocaleProvider } from '../../../settings/SystemLocaleProvider';
import type { ThemeLoginFrameProps, ThemeShellProps } from '../types';
import { CinemaLoginFrame, CinemaThemeShell } from './CinemaTheme';

vi.mock('../../../settings/systemSettingsApi', () => ({
  getPublicSystemSettings: vi.fn().mockResolvedValue({
    locale: 'en-US', siteTitle: 'TJXY', siteSubtitle: 'Your media library',
    logoUrl: '/brand/tjxy-mark.webp', iconUrl: '/brand/favicon.svg', publicUrl: '',
    listenHost: '127.0.0.1', port: 8096, revision: 0, restartRequired: false,
    environmentOverrides: { siteTitle: false, publicUrl: false, listenAddress: false },
    theme: { id: 'cinema', schemaVersion: 1, options: {}, revision: 0 },
  }),
}));

const shellProps: ThemeShellProps = {
  children: <p>Page content</p>,
  navigation: [
    { id: 'home', to: '/app/', label: 'Home', icon: Home },
    { id: 'libraries', to: '/app/libraries', label: 'Libraries', icon: Library },
  ],
  pathname: '/app/libraries',
  siteTitle: 'TJXY',
  logoUrl: '/brand/tjxy-mark.webp',
  userName: 'Admin',
  announcements: <button type="button">Announcements</button>,
  colorMode: 'light',
  onToggleColorMode: vi.fn(),
  onNavigate: vi.fn(),
  onSignOut: vi.fn(),
  options: { density: 'comfortable', contentWidth: 'wide', accent: 'crimson' },
};

it('renders the cinema brand, active navigation, and responsive utilities', async () => {
  renderWithLocale(<MemoryRouter initialEntries={['/app/libraries']}><CinemaThemeShell {...shellProps} /></MemoryRouter>);

  const navigation = await screen.findByRole('navigation', { name: 'Client navigation' });
  expect(screen.getByRole('link', { name: 'TJXY home' })).toHaveAttribute('href', '/app/');
  expect(screen.getByText('Screening room')).toBeVisible();
  expect(within(navigation).getByRole('link', { name: 'Libraries' })).toHaveClass('is-active');
  expect(screen.getByRole('button', { name: 'Announcements' })).toBeVisible();
});

it('renders localized screening copy in the cinema login feature', async () => {
  const props: ThemeLoginFrameProps = {
    actions: <button type="button">Language</button>,
    children: <h1>Welcome back</h1>,
    logoUrl: '/brand/tjxy-mark.webp',
    options: { accent: 'crimson' },
    siteSubtitle: 'Your media library',
    siteTitle: 'TJXY',
  };

  const { container } = renderWithLocale(<CinemaLoginFrame {...props} />);

  expect(await screen.findByText('Private screenings, organized around your library.')).toBeVisible();
  expect(container.querySelector('.cinema-login__screen')).toHaveAttribute('aria-hidden', 'true');
  expect(screen.getByRole('heading', { name: 'Welcome back' })).toBeVisible();
});

function renderWithLocale(children: React.ReactNode) {
  window.localStorage.setItem('tjxy-system-locale', 'en-US');
  return render(<SystemLocaleProvider>{children}</SystemLocaleProvider>);
}
