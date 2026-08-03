import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { ClientShell } from './ClientShell';

vi.mock('../auth/ClientAuthContext', () => ({
  useClientAuth: () => ({
    signOut: vi.fn(),
    user: { Id: 'user-1', Name: 'Admin' },
  }),
}));

beforeEach(() => {
  localStorage.clear();
  document.documentElement.classList.remove('dark', 'light');
  document.documentElement.removeAttribute('data-theme');
});

it('persists the selected color theme and exposes the next theme as its action', async () => {
  const user = userEvent.setup();
  renderShell('/app/libraries');

  const themeToggle = screen.getByRole('button', { name: 'Switch to dark theme' });
  expect(document.documentElement).toHaveAttribute('data-theme', 'light');

  await user.click(themeToggle);

  expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
  expect(document.documentElement).toHaveClass('dark');
  expect(localStorage.getItem('tjxy-color-theme')).toBe('dark');
  expect(screen.getByRole('button', { name: 'Switch to light theme' })).toBeVisible();
});

it('presents the same primary destinations in the top bar and mobile navigation', async () => {
  const user = userEvent.setup();
  renderShell('/app/libraries');

  const topNavigation = screen.getByRole('navigation', { name: 'TJXY navigation' });
  expect(topNavigation).toBeVisible();
  expect(within(topNavigation).getByRole('link', { name: 'Libraries' })).toHaveAttribute('aria-current', 'page');

  await user.click(screen.getByRole('button', { name: 'Open navigation' }));

  const mobileNavigation = await screen.findByRole('navigation', { name: 'Mobile navigation' });
  expect(mobileNavigation).toHaveTextContent('Home');
  expect(mobileNavigation).toHaveTextContent('Libraries');
  expect(mobileNavigation).toHaveTextContent('Search');
  expect(mobileNavigation).toHaveTextContent('AI assistant');
  expect(within(mobileNavigation).getByRole('link', { name: 'Libraries' })).toHaveAttribute('aria-current', 'page');
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});

it('exposes rankings and a profile destination from the account menu', async () => {
  const user = userEvent.setup();
  renderShell('/app/');

  expect(screen.getByRole('link', { name: 'Rankings' })).toHaveAttribute('href', '/app/rankings');
  expect(screen.getByRole('link', { name: 'AI assistant' })).toHaveAttribute('href', '/app/ai');
  await user.click(screen.getByRole('button', { name: 'Open account menu for Admin' }));

  expect(await screen.findByRole('menuitem', { name: 'Profile & stats' })).toBeVisible();
});

it('uses the shared system mark for the home brand link', () => {
  renderShell('/app/');

  const brand = screen.getByRole('link', { name: 'TJXY home' });
  expect(brand.querySelector('img')).toHaveAttribute('src', '/brand/tjxy-mark.webp');
});

it('groups the header controls in the HeroUI Pro navigation landmark', () => {
  renderShell('/app/rankings');

  const navigation = screen.getByRole('navigation', { name: 'TJXY navigation' });
  expect(within(navigation).getByRole('link', { name: 'Rankings' })).toHaveAttribute('aria-current', 'page');
  expect(within(navigation).getByRole('button', { name: 'Switch to dark theme' })).toBeVisible();
  expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
});

function renderShell(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <ClientShell>
        <p>Page content</p>
      </ClientShell>
    </MemoryRouter>,
  );
}
