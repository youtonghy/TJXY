import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderWithAdmin } from '../test/renderWithAdmin';
import { DashboardPage } from './DashboardPage';
import {
  getDashboardSnapshot,
  getLoginHistory,
  getWatchHistory,
  type DashboardSnapshot,
} from './dashboardApi';

vi.mock('./dashboardApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./dashboardApi')>();
  return {
    ...actual,
    getDashboardSnapshot: vi.fn(),
    getLoginHistory: vi.fn(),
    getWatchHistory: vi.fn(),
  };
});

vi.mock('./DashboardCharts', () => ({
  PlaybackTrendChart: () => <div>Playback activity chart</div>,
  TopItemsChart: () => <div>Most played chart</div>,
}));

const snapshotMock = vi.mocked(getDashboardSnapshot);
const loginMock = vi.mocked(getLoginHistory);
const watchMock = vi.mocked(getWatchHistory);

const snapshot: DashboardSnapshot = {
  summary: {
    from: '2026-07-31T00:00:00Z',
    to: '2026-07-31T12:00:00Z',
    usersTotal: 12,
    usersDisabled: 1,
    catalogTotal: 263,
    movies: 100,
    series: 100,
    episodes: 63,
    playCount: 24,
    uniqueViewers: 7,
    currentlyWatching: 1,
    trend: [],
    topItems: [],
  },
  nowPlaying: [{
    sessionId: '11111111-1111-4111-8111-111111111111',
    userId: '22222222-2222-4222-8222-222222222222',
    userName: 'Alex',
    itemId: '33333333-3333-4333-8333-333333333333',
    itemName: 'Arrival',
    itemType: 'Movie',
    runtimeTicks: 10_000,
    positionTicks: 4_000,
    clientName: 'TJXY Web',
    deviceName: 'Browser',
    startedAt: '2026-07-31T11:00:00Z',
    lastEventAt: '2026-07-31T11:59:30Z',
  }],
};

beforeEach(() => {
  snapshotMock.mockReset();
  loginMock.mockReset();
  watchMock.mockReset();
  snapshotMock.mockResolvedValue(snapshot);
  loginMock.mockResolvedValue({
    items: [{
      sessionId: '44444444-4444-4444-8444-444444444444',
      userId: '22222222-2222-4222-8222-222222222222',
      userName: 'Alex',
      clientName: 'TJXY Web',
      clientVersion: '0.1.0',
      deviceName: 'Browser',
      createdAt: '2026-07-31T10:00:00Z',
      lastSeenAt: '2026-07-31T11:00:00Z',
      expiresAt: '2026-08-31T10:00:00Z',
      revokedAt: null,
      status: 'Active',
    }],
    totalRecordCount: 1,
    startIndex: 0,
  });
  watchMock.mockResolvedValue({
    items: [{
      sessionId: '55555555-5555-4555-8555-555555555555',
      userId: '22222222-2222-4222-8222-222222222222',
      userName: 'Alex',
      itemId: '33333333-3333-4333-8333-333333333333',
      itemName: 'Arrival',
      itemType: 'Movie',
      runtimeTicks: 10_000,
      positionTicks: 4_000,
      startedAt: '2026-07-31T11:00:00Z',
      lastEventAt: '2026-07-31T11:59:30Z',
      stoppedAt: null,
    }],
    totalRecordCount: 1,
    startIndex: 0,
  });
});

it('renders server KPIs, current viewers, and successful login records', async () => {
  renderWithAdmin(<DashboardPage />, { initialEntries: ['/admin'] });

  expect(await screen.findByRole('heading', { name: 'Dashboard' })).toBeVisible();
  const rangeTabs = screen.getByRole('tablist', { name: 'Dashboard time range' });
  expect(rangeTabs).toHaveClass('grid', 'w-full', 'grid-cols-3');
  for (const tab of screen.getAllByRole('tab', { name: /Today|7 days|30 days/u })) {
    expect(tab).toHaveClass('whitespace-nowrap');
  }
  expect(screen.getByRole('button', { name: 'Refresh dashboard' })).toHaveClass('shrink-0');
  expect(screen.getAllByText('100', { selector: 'p' })).toHaveLength(2);
  expect(screen.getByText('24', { selector: 'p' })).toBeVisible();
  expect(screen.getByRole('grid', { name: 'Currently watching users' })).toHaveTextContent('Arrival');
  expect(await screen.findByRole('grid', { name: 'Login records' })).toHaveTextContent('Alex');
  expect(snapshotMock).toHaveBeenCalledWith('today', expect.any(AbortSignal));
});

it('reloads the summary for a new range and switches to watch history', async () => {
  renderWithAdmin(<DashboardPage />, { initialEntries: ['/admin'] });
  const user = userEvent.setup();

  await screen.findByRole('heading', { name: 'Dashboard' });
  await screen.findByRole('grid', { name: 'Login records' });
  await user.click(screen.getByRole('tab', { name: '7 days' }));
  await waitFor(() => { expect(snapshotMock).toHaveBeenLastCalledWith('7d', expect.any(AbortSignal)); });

  await user.click(screen.getByRole('tab', { name: 'Watch history' }));
  expect(await screen.findByRole('grid', { name: 'Watch history' })).toHaveTextContent('Arrival');
  expect(watchMock).toHaveBeenCalledWith(0, 25, expect.any(AbortSignal));
});
