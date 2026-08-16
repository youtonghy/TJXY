import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';

import { ProfilePage } from './ProfilePage';

const api = vi.hoisted(() => ({
  changePassword: vi.fn(),
  getProfile: vi.fn(),
  getUserInsights: vi.fn(),
  listPersonalSessions: vi.fn(),
  revokePersonalSession: vi.fn(),
  updateProfile: vi.fn(),
}));

vi.mock('../api/portalApi', () => api);
vi.mock('../auth/ClientAuthContext', () => ({ useClientAuth: () => ({ signOut: vi.fn() }) }));

beforeEach(() => {
  api.updateProfile.mockResolvedValue({ Bio: 'Updated.', Username: 'Admin Two' });
  api.getProfile.mockResolvedValue({ Bio: 'Film lover.', Username: 'Admin' });
  api.listPersonalSessions.mockResolvedValue([]);
  api.getUserInsights.mockResolvedValue({
    Daily: [{ Date: '2026-07-31', WatchedTicks: 18_000_000_000 }],
    Genres: [{ Name: 'Drama', WatchedTicks: 18_000_000_000 }],
    Media: { Movies: 2, Series: 1 },
    PlayCount: 4,
    Recent: [{ Id: 'movie-1', Name: 'Arrival', ProductionYear: 2016, Type: 'Movie' }],
    Timeline: [{ At: '2026-07-31T12:00:00Z', ItemId: 'movie-1', Kind: 'MovieWatched', Name: 'Arrival' }],
    UniqueTitles: 3,
    WatchedTicks: 18_000_000_000,
  });
});

it('submits profile and password changes through one confirmed account update', async () => {
  const user = userEvent.setup();
  render(<MemoryRouter><ProfilePage /></MemoryRouter>);
  await screen.findByRole('heading', { name: 'Admin' });
  await user.click(screen.getByRole('button', { name: 'Edit profile' }));
  await user.clear(screen.getByRole('textbox', { name: 'Username' }));
  await user.type(screen.getByRole('textbox', { name: 'Username' }), 'Admin Two');
  await user.type(screen.getByLabelText('Current password'), 'old password');
  await user.type(screen.getByLabelText('New password'), 'new password');
  await user.type(screen.getByLabelText('Confirm new password'), 'new password');
  await user.click(screen.getByRole('button', { name: 'Save changes' }));

  expect(api.updateProfile).toHaveBeenCalledWith(expect.objectContaining({
    CurrentPassword: 'old password',
    NewPassword: 'new password',
    Username: 'Admin Two',
  }));
  expect(api.changePassword).not.toHaveBeenCalled();
});

it('shows profile details and opens one dedicated edit dialog', async () => {
  const user = userEvent.setup();
  render(<MemoryRouter><ProfilePage /></MemoryRouter>);

  expect(await screen.findByRole('heading', { name: 'Admin' })).toBeVisible();
  expect(screen.getByText('Film lover.')).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Edit profile' }));

  expect(await screen.findByRole('dialog', { name: 'Edit account' })).toBeVisible();
  expect(screen.getByRole('textbox', { name: 'Username' })).toHaveValue('Admin');
  expect(screen.getByRole('textbox', { name: 'Biography' })).toHaveValue('Film lover.');
  expect(screen.getByLabelText('Current password')).toBeVisible();
  expect(screen.getByLabelText('New password')).toBeVisible();
});

it('reloads all statistic cards when the selected range changes', async () => {
  const user = userEvent.setup();
  render(<MemoryRouter><ProfilePage /></MemoryRouter>);
  await screen.findByRole('heading', { name: 'Admin' });

  await user.click(screen.getByRole('button', { name: '30 days' }));

  expect(api.getUserInsights).toHaveBeenLastCalledWith('30d');
  expect(screen.getByText('Watch time')).toBeVisible();
  expect(screen.getByRole('group', { name: 'Viewing KPIs' })).toBeVisible();
  expect(screen.getByRole('img', { name: 'Daily watch time bar chart' })).toBeVisible();
  expect(screen.getByRole('heading', { name: 'Genre mix' })).toBeVisible();
  expect(screen.getByRole('img', { name: 'Genre watch time radar chart' })).toBeVisible();
  expect(screen.getByText('Drama: 30m')).toBeInTheDocument();
  expect(screen.getByRole('heading', { name: 'Movies and series' })).toBeVisible();
  expect(screen.getByRole('img', { name: 'Movies and series pie chart' })).toBeVisible();
  expect(screen.getByLabelText('2 movies')).toBeVisible();
  expect(screen.getByLabelText('1 series')).toBeVisible();
  expect(screen.getByRole('heading', { name: 'Cumulative watch time' })).toBeVisible();
  expect(screen.getByRole('img', { name: 'Cumulative watch time area chart' })).toBeVisible();
  expect(screen.getByText('07-31: 30m')).toBeInTheDocument();
  expect(screen.getByRole('heading', { name: 'Viewing timeline' })).toBeVisible();
  expect(screen.getByRole('link', { name: /Arrival/ })).toHaveAttribute('href', '/app/items/movie-1');
});

it('keeps the genre radar visible when every genre has zero watch time', async () => {
  api.getUserInsights.mockResolvedValueOnce({
    Daily: [],
    Genres: [
      { Name: 'Adventure', WatchedTicks: 0 },
      { Name: 'Action', WatchedTicks: 0 },
      { Name: 'Comedy', WatchedTicks: 0 },
    ],
    Media: { Movies: 0, Series: 0 },
    PlayCount: 0,
    Recent: [],
    Timeline: [],
    UniqueTitles: 0,
    WatchedTicks: 0,
  });

  render(<MemoryRouter><ProfilePage /></MemoryRouter>);

  expect(await screen.findByRole('img', { name: 'Genre watch time radar chart' })).toBeVisible();
  expect(screen.getByText('Adventure: 0m; Action: 0m; Comedy: 0m')).toBeInTheDocument();
});
