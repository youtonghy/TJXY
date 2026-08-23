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
const passkeyApi = vi.hoisted(() => ({
  deletePasskey: vi.fn(),
  listPasskeys: vi.fn(),
  registerPasskey: vi.fn(),
}));
const systemSettings = vi.hoisted(() => ({ passkeyEnabled: false }));

const sessions = Array.from({ length: 6 }, (_, index) => ({
  ApplicationVersion: '0.1.0',
  ClientName: 'TJXY Web',
  CreatedAt: `2026-08-20T${String(10 + index).padStart(2, '0')}:00:00Z`,
  DeviceId: `device-${String(index + 1)}`,
  DeviceName: `Browser ${String(index + 1)}`,
  Id: `session-${String(index + 1)}`,
  IsCurrent: index === 5,
  LastActivityDate: `2026-08-20T${String(10 + index).padStart(2, '0')}:00:00Z`,
}));

vi.mock('../api/portalApi', () => api);
vi.mock('../auth/ClientAuthContext', () => ({ useClientAuth: () => ({ signOut: vi.fn() }) }));
vi.mock('../auth/passkeyApi', () => passkeyApi);
vi.mock('../../settings/SystemLocaleProvider', () => ({
  useSystemLocale: () => ({ locale: 'en-US', passkeyEnabled: systemSettings.passkeyEnabled }),
}));

beforeEach(() => {
  systemSettings.passkeyEnabled = false;
  passkeyApi.listPasskeys.mockResolvedValue([]);
  passkeyApi.deletePasskey.mockResolvedValue(undefined);
  passkeyApi.registerPasskey.mockResolvedValue(undefined);
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
  expect(screen.queryByText('Passkey')).not.toBeInTheDocument();
});

it('lists and deletes Passkeys when passwordless login is enabled', async () => {
  systemSettings.passkeyEnabled = true;
  passkeyApi.listPasskeys.mockResolvedValueOnce([{
    CreatedAt: '2026-08-23T10:00:00Z',
    Id: 'passkey-1',
    LastUsedAt: '2026-08-23T10:00:00Z',
    Name: 'MacBook Touch ID',
  }]);
  const user = userEvent.setup();
  render(<MemoryRouter><ProfilePage /></MemoryRouter>);

  await screen.findByRole('heading', { name: 'Admin' });
  await user.click(screen.getByRole('button', { name: 'Edit profile' }));
  expect(await screen.findByText('MacBook Touch ID')).toBeVisible();

  api.updateProfile.mockClear();
  await user.click(screen.getByRole('button', { name: 'Delete MacBook Touch ID' }));

  expect(passkeyApi.deletePasskey).toHaveBeenCalledWith('passkey-1');
  expect(screen.queryByText('MacBook Touch ID')).not.toBeInTheDocument();
  expect(api.updateProfile).not.toHaveBeenCalled();
});

it('shows only the four latest sessions and manages every session in a dialog', async () => {
  api.listPersonalSessions.mockResolvedValueOnce([...sessions].reverse());
  const user = userEvent.setup();
  render(<MemoryRouter><ProfilePage /></MemoryRouter>);

  await screen.findByRole('heading', { name: 'Admin' });
  const summary = screen.getByRole('list', { name: 'Signed-in devices' });
  expect(summary).toHaveTextContent('Browser 6');
  expect(summary).toHaveTextContent('Browser 3');
  expect(summary).not.toHaveTextContent('Browser 2');
  expect(summary).not.toHaveTextContent('Browser 1');
  expect(screen.queryByRole('button', { name: 'Revoke' })).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Manage all 6 devices' }));

  const dialog = await screen.findByRole('dialog', { name: 'Manage signed-in devices' });
  expect(dialog).toHaveTextContent('Browser 6');
  expect(dialog).toHaveTextContent('Browser 1');
  expect(screen.getAllByRole('button', { name: 'Revoke' })).toHaveLength(5);
  expect(screen.getByRole('button', { name: 'Sign out' })).toBeVisible();
});

it('revokes an older session from the device manager and removes it from the list', async () => {
  api.listPersonalSessions.mockResolvedValueOnce(sessions);
  api.revokePersonalSession.mockResolvedValueOnce(undefined);
  const user = userEvent.setup();
  render(<MemoryRouter><ProfilePage /></MemoryRouter>);
  await screen.findByRole('heading', { name: 'Admin' });
  await user.click(screen.getByRole('button', { name: 'Manage all 6 devices' }));

  const firstRevoke = screen.getAllByRole('button', { name: 'Revoke' }).at(0);
  if (!firstRevoke) throw new Error('Expected at least one revocable session.');
  await user.click(firstRevoke);

  expect(api.revokePersonalSession).toHaveBeenCalledWith('session-5');
  expect(await screen.findByText('5 active sessions')).toBeVisible();
  expect(screen.getByRole('dialog', { name: 'Manage signed-in devices' })).not.toHaveTextContent('Browser 5');
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
