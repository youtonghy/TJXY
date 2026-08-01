import { dashboardWindow, getDashboardSnapshot, getLoginHistory } from './dashboardApi';

const client = vi.hoisted(() => ({ apiRequest: vi.fn() }));
vi.mock('../api/httpClient', () => client);

beforeEach(() => {
  client.apiRequest.mockReset();
});

it('uses local calendar boundaries for dashboard ranges', () => {
  const now = new Date(2026, 6, 31, 18, 30, 0);
  const range = dashboardWindow('7d', now);
  const from = new Date(range.from);
  expect(from.getFullYear()).toBe(2026);
  expect(from.getMonth()).toBe(6);
  expect(from.getDate()).toBe(25);
  expect(from.getHours()).toBe(0);
  expect(range.to).toBe(now.toISOString());
});

it('validates and maps the dashboard snapshot contract', async () => {
  client.apiRequest
    .mockResolvedValueOnce({
      From: '2026-07-31T00:00:00Z',
      To: '2026-07-31T10:00:00Z',
      UsersTotal: 2,
      UsersDisabled: 0,
      CatalogTotal: 12,
      Movies: 5,
      Series: 3,
      Episodes: 4,
      PlayCount: 8,
      UniqueViewers: 2,
      CurrentlyWatching: 1,
      Trend: [{ BucketStart: '2026-07-31T09:00:00Z', PlayCount: 2, UniqueViewers: 1 }],
      TopItems: [{
        ItemId: '11111111-1111-4111-8111-111111111111',
        Name: 'Arrival',
        ItemType: 'Movie',
        ProductionYear: 2016,
        PlayCount: 4,
        UniqueViewers: 2,
      }],
    })
    .mockResolvedValueOnce([]);

  const snapshot = await getDashboardSnapshot('today');
  expect(snapshot.summary.movies).toBe(5);
  expect(snapshot.summary.topItems[0]?.name).toBe('Arrival');
  expect(snapshot.nowPlaying).toEqual([]);
  expect(client.apiRequest).toHaveBeenCalledTimes(2);
});

it('requests a stable history page', async () => {
  client.apiRequest.mockResolvedValue({ Items: [], TotalRecordCount: 0, StartIndex: 25 });
  await expect(getLoginHistory(25, 25)).resolves.toEqual({
    items: [], totalRecordCount: 0, startIndex: 25,
  });
  expect(client.apiRequest).toHaveBeenCalledWith(
    '/Admin/Dashboard/LoginHistory?startIndex=25&limit=25',
    {},
  );
});
