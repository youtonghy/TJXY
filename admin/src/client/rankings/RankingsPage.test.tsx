import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';

import { RankingsPage } from './RankingsPage';

const api = vi.hoisted(() => ({
  getServerRanking: vi.fn(),
  getTmdbRanking: vi.fn(),
}));

vi.mock('../api/portalApi', () => api);

beforeEach(() => {
  api.getTmdbRanking.mockReset();
  api.getServerRanking.mockReset();
  api.getTmdbRanking.mockImplementation((kind: string) => Promise.resolve([{
    Name: kind === 'Movie' ? 'Popular movie' : 'Popular series',
    Overview: 'Overview',
    PosterUrl: 'https://image.tmdb.org/t/p/w500/poster.jpg',
    ProductionYear: 2026,
    Rank: 1,
    Rating: 8.2,
    TmdbId: 123,
  }]));
  api.getServerRanking.mockResolvedValue([{
    Id: 'local-1', ItemType: 'Movie', Name: 'Yesterday title', PlayCount: 7,
    ProductionYear: 2025, Rank: 1, UniqueViewers: 3,
  }]);
});

it('shows TMDB movie, TMDB series, and yesterday server rankings', async () => {
  render(<MemoryRouter><RankingsPage /></MemoryRouter>);

  expect(await screen.findByText('Popular movie')).toBeVisible();
  const rankingGrid = screen.getByRole('grid', { name: 'TMDB movie rankings' });
  expect(rankingGrid).toBeVisible();
  expect(screen.getByRole('columnheader', { name: 'Rank' })).toBeVisible();
  expect(screen.getByRole('columnheader', { name: 'Title' })).toBeVisible();
  expect(screen.getByRole('columnheader', { name: 'Rating' })).toBeVisible();
  expect(screen.getByRole('tab', { name: 'TMDB movies' })).toBeVisible();
  expect(screen.getByRole('tab', { name: 'TMDB series' })).toBeVisible();
  expect(screen.getByRole('tab', { name: 'Yesterday on TJXY' })).toBeVisible();
  expect(api.getTmdbRanking).toHaveBeenCalledWith('Movie');
  expect(api.getTmdbRanking).toHaveBeenCalledWith('Series');
  expect(api.getServerRanking).toHaveBeenCalledOnce();
});

it('keeps the server ranking available when TMDB cannot be loaded', async () => {
  const user = userEvent.setup();
  api.getTmdbRanking.mockRejectedValue(new Error('TMDB unavailable'));

  render(<MemoryRouter><RankingsPage /></MemoryRouter>);

  expect(await screen.findByText('TMDB ranking is unavailable. Check the TMDB setting and network connection.')).toBeVisible();
  await user.click(screen.getByRole('tab', { name: 'Yesterday on TJXY' }));
  expect(await screen.findByText('Yesterday title')).toBeVisible();
});
