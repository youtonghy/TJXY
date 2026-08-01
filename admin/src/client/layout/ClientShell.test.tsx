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

  expect(screen.getByRole('navigation', { name: 'Primary' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Libraries' })).toHaveAttribute('aria-current', 'page');

  await user.click(screen.getByRole('button', { name: 'Open navigation' }));

  const dialog = await screen.findByRole('dialog', { name: 'Browse TJXY' });
  expect(dialog).toHaveTextContent('Home');
  expect(dialog).toHaveTextContent('Libraries');
  expect(dialog).toHaveTextContent('Search');
  expect(screen.getByRole('navigation', { name: 'Mobile navigation' })).toBeVisible();
});

it('exposes rankings and a profile destination from the account menu', async () => {
  const user = userEvent.setup();
  renderShell('/app/');

  expect(screen.getByRole('link', { name: 'Rankings' })).toHaveAttribute('href', '/app/rankings');
  await user.click(screen.getByRole('button', { name: 'Open account menu for Admin' }));

  expect(await screen.findByRole('menuitem', { name: 'Profile & stats' })).toBeVisible();
});

it('uses the shared system mark for the home brand link', () => {
  renderShell('/app/');

  const brand = screen.getByRole('link', { name: 'TJXY home' });
  expect(brand.querySelector('img')).toHaveAttribute('src', '/brand/tjxy-mark.webp');
});

it('groups the desktop header controls as one accessible navigation toolbar', () => {
  renderShell('/app/rankings');

  const toolbar = screen.getByRole('toolbar', { name: 'TJXY navigation' });
  expect(within(toolbar).getByRole('link', { name: 'Rankings' })).toHaveAttribute('aria-current', 'page');
  expect(within(toolbar).getByRole('button', { name: 'Switch to dark theme' })).toBeVisible();
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
