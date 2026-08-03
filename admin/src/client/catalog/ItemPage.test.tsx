import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { ItemPage } from './ItemPage';

const api = vi.hoisted(() => ({
  getChildren: vi.fn(),
  getItem: vi.fn(),
  getLibraries: vi.fn(),
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
  People: [
    { Id: 'person-1', Name: 'Johan Renck', Role: 'Director', Type: 'Crew' },
    { Id: 'person-2', Name: 'Jessie Buckley', Role: 'Lyudmilla Ignatenko', Type: 'Actor' },
    { Id: 'person-3', Name: 'Jared Harris', Role: 'Valery Legasov', Type: 'Actor' },
    { Id: 'person-4', Name: 'Stellan Skarsgård', Role: 'Boris Shcherbina', Type: 'Actor' },
    { Id: 'person-5', Name: 'Emily Watson', Role: 'Ulana Khomyuk', Type: 'Actor' },
  ],
  HasMediaSources: false,
};

beforeEach(() => {
  api.getItem.mockResolvedValue(series);
  api.getLibraries.mockResolvedValue([
    { Id: 'library-tv', Name: 'TV Shows', CollectionType: 'tvshows' },
  ]);
  api.getChildren.mockImplementation((id: string) => Promise.resolve(id === 'series-1'
    ? [
        { Id: 'season-2', Name: 'Season 2', Type: 'Season', IsFolder: true, IndexNumber: 2 },
        { Id: 'season-1', Name: 'Season 1', Type: 'Season', IsFolder: true, IndexNumber: 1 },
      ]
    : [
        { Id: 'episode-2', Name: 'Please Remain Calm', Type: 'Episode', IndexNumber: 2, Overview: 'Second episode.', ImageTags: { Primary: 'still-2' } },
        { Id: 'episode-1', Name: '1:23:45', Type: 'Episode', IndexNumber: 1, Overview: 'First episode.', ImageTags: { Primary: 'still-1' } },
      ]));
});

it('renders a precise HeroUI breadcrumb path back through the library and item hierarchy', async () => {
  const episode = {
    Id: 'episode-8',
    Name: 'Episode 8',
    Type: 'Episode',
    IsFolder: false,
    ParentId: 'season-1',
  };
  const season = {
    Id: 'season-1',
    Name: 'Season 1',
    Type: 'Season',
    IsFolder: true,
    ParentId: 'series-1',
  };
  api.getItem.mockImplementation((itemId: string) => Promise.resolve({
    'episode-8': episode,
    'season-1': season,
    'series-1': series,
  }[itemId]));

  renderItem('episode-8');

  const breadcrumb = await screen.findByRole('navigation', { name: 'Item breadcrumb' });
  expect(await within(breadcrumb).findByRole('link', { name: 'Home' })).toBeVisible();
  expect(await within(breadcrumb).findByRole('link', { name: 'Libraries' })).toBeVisible();
  expect(await within(breadcrumb).findByRole('link', { name: 'TV Shows' })).toBeVisible();
  expect(await within(breadcrumb).findByRole('link', { name: 'Chernobyl' })).toBeVisible();
  expect(await within(breadcrumb).findByRole('link', { name: 'Season 1' })).toBeVisible();
  expect(await within(breadcrumb).findByText('Episode 8')).toHaveAttribute('aria-current', 'page');
});

it('renders rich series metadata and loads ordered episodes for the selected season', async () => {
  renderItem('series-1');

  expect(await screen.findByRole('heading', { name: 'Chernobyl' })).toBeInTheDocument();
  expect(screen.getByRole('radiogroup', { name: '8.7 out of 10 from 7,000 votes' })).toBeVisible();
  expect(screen.getByText('8.7')).toBeVisible();
  expect(screen.getByText('7,000 votes')).toBeVisible();
  expect(screen.getByText('1h 0m')).toBeInTheDocument();
  expect(screen.getByText('TV-MA')).toBeInTheDocument();
  expect(screen.getByText('United States')).toBeInTheDocument();
  expect(screen.getByText('English')).toBeInTheDocument();
  expect(screen.getByText('Johan Renck')).toBeInTheDocument();
  expect(screen.getByRole('radio', { name: 'Season 1' })).toHaveAttribute('aria-checked', 'true');

  const episodeOne = await screen.findByText('1:23:45');
  const episodeTwo = screen.getByText('Please Remain Calm');
  expect(episodeOne.compareDocumentPosition(episodeTwo) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.getByRole('img', { name: 'Still for episode 1: 1:23:45' })).toBeVisible();
  expect(screen.getByRole('img', { name: 'Still for episode 2: Please Remain Calm' })).toBeVisible();
  expect(screen.getByText('No video source available')).toBeVisible();
  expect(screen.getByText('Add a media file to this title before starting playback.')).toBeVisible();
  expect(screen.queryByText(/demo|development/i)).not.toBeInTheDocument();
  const seasonsHeading = screen.getByRole('heading', { name: 'Seasons' });
  const detailsHeading = screen.getByRole('heading', { name: 'Details' });
  expect(seasonsHeading.compareDocumentPosition(detailsHeading) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(api.getChildren).toHaveBeenCalledWith('season-1');
});

it('shows two credit rows by default and expands the complete cast and crew list', async () => {
  const user = userEvent.setup();
  renderItem('series-1');

  expect(await screen.findByText('Johan Renck')).toBeVisible();
  expect(screen.getByText('Jessie Buckley')).toBeVisible();
  expect(screen.queryByText('Jared Harris')).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'View all 5 credits' }));

  expect(screen.getByText('Jared Harris')).toBeVisible();
  expect(screen.getByText('Emily Watson')).toBeVisible();

  await user.click(screen.getByRole('button', { name: 'Show fewer credits' }));
  expect(screen.queryByText('Jared Harris')).not.toBeInTheDocument();
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

it('uses square primary artwork for an audio item', async () => {
  api.getItem.mockResolvedValueOnce({ Id: 'track-1', Name: 'First Light', Type: 'Audio', IsFolder: false });
  api.getChildren.mockResolvedValueOnce([]);
  renderItem('track-1');

  const artwork = await screen.findByRole('img', { name: 'Poster for First Light' });
  expect(artwork.parentElement).toHaveClass('aspect-square');
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
