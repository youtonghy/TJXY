import { render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { ItemPage } from './ItemPage';

const api = vi.hoisted(() => ({
  getChildren: vi.fn(),
  getItem: vi.fn(),
  toggleFavorite: vi.fn(),
  togglePlayed: vi.fn(),
}));

vi.mock('../api/catalogApi', () => api);
vi.mock('../auth/ClientAuthContext', () => ({
  useClientAuth: () => ({ user: { Id: 'user-1', Name: 'Viewer' } }),
}));
vi.mock('../ui/MediaImage', () => ({
  MediaImage: ({ alt }: { alt: string }) => <div aria-label={alt} role="img" />,
}));

const series = {
  Id: 'series-1',
  Name: 'Chernobyl',
  OriginalTitle: 'Chernobyl',
  Type: 'Series',
  IsFolder: true,
  ProductionYear: 2019,
  Overview: 'A disaster and its aftermath.',
  Tagline: 'Every lie we tell incurs a debt.',
  CommunityRating: 8.7,
  VoteCount: 7_000,
  RunTimeTicks: 36_000_000_000,
  PremiereDate: '2019-05-06T00:00:00Z',
  EndDate: '2019-06-03T00:00:00Z',
  Status: 'Ended',
  OfficialRating: 'TV-MA',
  OriginalLanguage: 'en',
  Genres: ['Drama'],
  Studios: ['HBO'],
  Countries: [{ Code: 'US', Name: 'United States' }],
  Languages: [{ Code: 'en', Name: 'English' }],
  People: [{ Id: 'person-1', Name: 'Johan Renck', Role: 'Director', Type: 'Crew' }],
  HasMediaSources: false,
};

beforeEach(() => {
  api.getItem.mockResolvedValue(series);
  api.getChildren.mockImplementation((id: string) => Promise.resolve(id === 'series-1'
    ? [
        { Id: 'season-2', Name: 'Season 2', Type: 'Season', IsFolder: true, IndexNumber: 2 },
        { Id: 'season-1', Name: 'Season 1', Type: 'Season', IsFolder: true, IndexNumber: 1 },
      ]
    : [
        { Id: 'episode-2', Name: 'Please Remain Calm', Type: 'Episode', IndexNumber: 2, Overview: 'Second episode.' },
        { Id: 'episode-1', Name: '1:23:45', Type: 'Episode', IndexNumber: 1, Overview: 'First episode.' },
      ]));
});

it('renders rich series metadata and loads ordered episodes for the selected season', async () => {
  renderItem('series-1');

  expect(await screen.findByRole('heading', { name: 'Chernobyl' })).toBeInTheDocument();
  expect(screen.getByText('8.7 · 7,000 votes')).toBeInTheDocument();
  expect(screen.getByText('1h 0m')).toBeInTheDocument();
  expect(screen.getByText('TV-MA')).toBeInTheDocument();
  expect(screen.getByText('United States')).toBeInTheDocument();
  expect(screen.getByText('English')).toBeInTheDocument();
  expect(screen.getByText('Johan Renck')).toBeInTheDocument();
  expect(screen.getByRole('tab', { name: 'Season 1' })).toHaveAttribute('aria-selected', 'true');

  const episodeOne = await screen.findByText('1:23:45');
  const episodeTwo = screen.getByText('Please Remain Calm');
  expect(episodeOne.compareDocumentPosition(episodeTwo) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(api.getChildren).toHaveBeenCalledWith('season-1');
});

it('does not invent rich facts for a sparse movie response', async () => {
  api.getItem.mockResolvedValueOnce({ Id: 'movie-1', Name: 'Sparse Movie', Type: 'Movie', IsFolder: false });
  api.getChildren.mockResolvedValueOnce([]);
  renderItem('movie-1');

  expect(await screen.findByRole('heading', { name: 'Sparse Movie' })).toBeInTheDocument();
  expect(screen.getByText('No additional details are available.')).toBeInTheDocument();
  expect(screen.queryByText('Rating')).not.toBeInTheDocument();
  expect(screen.queryByText('Demo metadata only')).not.toBeInTheDocument();
});

function renderItem(id: string) {
  return render(
    <MemoryRouter initialEntries={[`/app/items/${id}`]}>
      <Routes>
        <Route element={<ItemPage />} path="/app/items/:id" />
      </Routes>
    </MemoryRouter>,
  );
}
