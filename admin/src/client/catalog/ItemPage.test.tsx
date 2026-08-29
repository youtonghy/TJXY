import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { ItemPage } from './ItemPage';

const api = vi.hoisted(() => ({
  getChildren: vi.fn(),
  getItem: vi.fn(),
  getLibraries: vi.fn(),
  getSimilarItems: vi.fn(),
  toggleFavorite: vi.fn(),
  togglePlayed: vi.fn(),
}));
const auth = vi.hoisted(() => ({ isAdministrator: true }));
const tasks = vi.hoisted(() => ({ listRecentTaskJobs: vi.fn() }));
const playback = vi.hoisted(() => ({
  getPlaybackInfo: vi.fn(),
  issuePlaybackTicket: vi.fn(),
}));

vi.mock('../api/catalogApi', () => api);
vi.mock('../auth/ClientAuthContext', () => ({
  useClientAuth: () => ({ user: { Id: 'user-1', Name: 'Viewer', Policy: { IsAdministrator: auth.isAdministrator } } }),
}));
vi.mock('../../tasks/taskApi', () => tasks);
vi.mock('../api/playbackApi', () => playback);
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
  auth.isAdministrator = true;
  tasks.listRecentTaskJobs.mockReset();
  tasks.listRecentTaskJobs.mockResolvedValue([]);
  api.getSimilarItems.mockReset();
  api.getSimilarItems.mockResolvedValue([]);
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
  playback.getPlaybackInfo.mockReset();
  playback.getPlaybackInfo.mockResolvedValue({
    PlaySessionId: 'session-1',
    MediaSources: [{ Id: 'source-1', DirectStreamUrl: '/Videos/movie-1', SupportsDirectPlay: true }],
  });
  playback.issuePlaybackTicket.mockReset();
  playback.issuePlaybackTicket.mockResolvedValue({
    Id: 'ticket-1',
    Ticket: 'ticket-value',
    ExpiresAt: '2099-01-01T00:00:00Z',
    StreamUrl: '/Videos/movie-1/stream?PlaybackTicket=ticket-value',
  });
});

afterEach(() => {
  vi.useRealTimers();
});

it('polls a Partial movie until lazy metadata becomes complete', async () => {
  vi.useFakeTimers();
  const partial = {
    Id: 'movie-lazy',
    Name: 'Lazy Movie',
    Type: 'Movie',
    IsFolder: false,
    MetadataState: 'Partial' as const,
  };
  api.getItem
    .mockResolvedValueOnce(partial)
    .mockResolvedValueOnce({
      ...partial,
      MetadataState: 'Complete',
      Overview: 'Resolved metadata.',
    });

  renderItem('movie-lazy');
  await act(async () => { await Promise.resolve(); });

  expect(screen.getByText('Metadata scan in progress')).toBeVisible();
  await act(async () => { await vi.advanceTimersByTimeAsync(2_500); });

  expect(screen.getByText('Resolved metadata.')).toBeVisible();
  expect(screen.queryByText('Metadata scan in progress')).not.toBeInTheDocument();
  expect(api.getItem).toHaveBeenCalledTimes(2);
});

it('shows an administrator when metadata resolution finds no match', async () => {
  vi.useFakeTimers();
  const partial = {
    Id: 'movie-lazy',
    Name: 'Lazy Movie',
    Type: 'Movie',
    IsFolder: false,
    MetadataState: 'Partial' as const,
  };
  api.getItem.mockResolvedValue(partial);
  tasks.listRecentTaskJobs.mockResolvedValue([{
    id: 'job-1',
    taskKind: 'ResolveMetadata',
    scopeType: 'CatalogItem',
    scopeId: 'movie-lazy',
    outcome: 'NoMetadataMatch',
  }]);

  renderItem('movie-lazy');
  await act(async () => { await Promise.resolve(); });
  await act(async () => { await vi.advanceTimersByTimeAsync(2_500); });

  expect(screen.getByText('No metadata match')).toBeVisible();
  expect(screen.queryByText('Metadata scan in progress')).not.toBeInTheDocument();
});

it('does not show metadata scan administration status to a regular user', async () => {
  auth.isAdministrator = false;
  api.getItem.mockResolvedValue({
    Id: 'movie-lazy',
    Name: 'Lazy Movie',
    Type: 'Movie',
    IsFolder: false,
    MetadataState: 'Partial',
  });

  renderItem('movie-lazy');

  expect(await screen.findByRole('heading', { name: 'Lazy Movie' })).toBeVisible();
  expect(screen.queryByText('Metadata scan in progress')).not.toBeInTheDocument();
  expect(screen.queryByText('No metadata match')).not.toBeInTheDocument();
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
  expect(screen.queryByText('No video source available')).not.toBeInTheDocument();
  expect(screen.queryByText('Add a media file to this title before starting playback.')).not.toBeInTheDocument();
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

it('shows filtered same-type recommendations after cast and crew', async () => {
  api.getSimilarItems.mockResolvedValueOnce([
    { Id: 'movie-1', Name: 'Arrival', Type: 'Series', ProductionYear: 2016 },
    { Id: 'movie-2', Name: 'Station Eleven', Type: 'Series', ProductionYear: 2021 },
    { Id: 'movie-3', Name: 'Watched', Type: 'Series', UserData: { Played: true } },
    { Id: 'movie-4', Name: 'Wrong type', Type: 'Movie' },
    { Id: 'series-1', Name: 'Current', Type: 'Series' },
    { Id: 'series-2', Name: 'The Leftovers', Type: 'Series' },
    { Id: 'series-3', Name: 'Dark', Type: 'Series' },
    { Id: 'series-4', Name: 'Severance', Type: 'Series' },
  ]);
  renderItem('series-1');

  const recommendations = await screen.findByRole('region', { name: 'Recommended for you' });
  const carousel = await within(recommendations).findByRole('region', { name: 'Recommended titles' });
  expect(carousel).toHaveAttribute('aria-roledescription', 'carousel');
  expect(within(carousel).getAllByRole('group')).toHaveLength(4);
  expect(await within(carousel).findByRole('button', { name: 'Previous slide' })).toBeInTheDocument();
  expect(await within(carousel).findByRole('button', { name: 'Next slide' })).toBeInTheDocument();
  expect(within(recommendations).getByRole('link', { name: /Arrival/ })).toHaveAttribute('href', '/app/items/movie-1');
  expect(within(recommendations).getByRole('link', { name: /Station Eleven/ })).toHaveAttribute('href', '/app/items/movie-2');
  expect(within(recommendations).queryByText('Watched')).not.toBeInTheDocument();
  expect(within(recommendations).queryByText('Wrong type')).not.toBeInTheDocument();
  expect(within(recommendations).queryByText('Current')).not.toBeInTheDocument();
  expect(within(recommendations).getAllByRole('link')).toHaveLength(4);
  expect(within(recommendations).queryByText('Severance')).not.toBeInTheDocument();
  expect(api.getSimilarItems).toHaveBeenCalledWith('series-1', 4);

  const peopleHeading = screen.getByRole('heading', { name: 'Cast and crew' });
  const recommendationHeading = within(recommendations).getByRole('heading', { name: 'Recommended for you' });
  expect(peopleHeading.compareDocumentPosition(recommendationHeading) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it('renders six compact recommendation skeletons while the request is pending', async () => {
  api.getSimilarItems.mockReturnValueOnce(new Promise(() => undefined));
  renderItem('series-1');

  const recommendations = await screen.findByRole('region', { name: 'Recommended for you' });
  const loading = within(recommendations).getByRole('status', { name: 'Loading recommendations' });
  expect(loading.children).toHaveLength(6);
});

it('distinguishes an empty recommendation result from an unavailable request', async () => {
  renderItem('series-1');
  const emptyRecommendations = await screen.findByRole('region', { name: 'Recommended for you' });
  expect(await within(emptyRecommendations).findByText('No recommendations yet')).toBeVisible();

  api.getSimilarItems.mockReset();
  api.getSimilarItems.mockRejectedValueOnce(new Error('offline'));
  renderItem('series-1');
  expect(await screen.findByText('Recommendations are temporarily unavailable')).toBeVisible();
  expect(screen.getAllByRole('heading', { name: 'Chernobyl' }).length).toBeGreaterThan(0);
});

it.each([
  ['track-1', 'Audio'],
  ['season-1', 'Season'],
  ['episode-1', 'Episode'],
])('does not request or render recommendations for unsupported %s item types', async (id, type) => {
  api.getItem.mockResolvedValueOnce({ Id: id, Name: 'Unsupported item', Type: type, IsFolder: false });
  api.getChildren.mockResolvedValueOnce([]);
  renderItem(id);

  expect(await screen.findByRole('heading', { name: 'Unsupported item' })).toBeVisible();
  expect(api.getSimilarItems).not.toHaveBeenCalled();
  expect(screen.queryByRole('region', { name: 'Recommended for you' })).not.toBeInTheDocument();
});

it('does not invent rich facts for a sparse movie response', async () => {
  api.getItem.mockResolvedValueOnce({ Id: 'movie-1', Name: 'Sparse Movie', Type: 'Movie', IsFolder: false });
  api.getChildren.mockResolvedValueOnce([]);
  renderItem('movie-1');

  expect(await screen.findByRole('heading', { name: 'Sparse Movie' })).toBeInTheDocument();
  expect(screen.getByText('No additional details are available.')).toBeInTheDocument();
  expect(screen.queryByText('Rating')).not.toBeInTheDocument();
  expect(screen.queryByText('Demo metadata only')).not.toBeInTheDocument();
  await waitFor(() => {
    expect(api.getSimilarItems).toHaveBeenCalledWith('movie-1', 4);
  });
});

it('warns when a directly playable movie has no media source', async () => {
  api.getItem.mockResolvedValueOnce({
    Id: 'movie-1',
    Name: 'Missing File',
    Type: 'Movie',
    IsFolder: false,
    HasMediaSources: false,
  });
  renderItem('movie-1');

  expect(await screen.findByText('No video source available')).toBeVisible();
  expect(screen.getByText('Add a media file to this title before starting playback.')).toBeVisible();
});

it('keeps the primary split-button action on the built-in player', async () => {
  api.getItem.mockResolvedValueOnce({ Id: 'movie-1', Name: 'Playable Movie', Type: 'Movie', IsFolder: false });
  api.getChildren.mockResolvedValueOnce([]);
  const user = userEvent.setup();
  renderItem('movie-1');

  await user.click(await screen.findByRole('button', { name: 'Play' }));

  expect(await screen.findByText('Built-in player destination')).toBeVisible();
  expect(playback.getPlaybackInfo).not.toHaveBeenCalled();
});

it('prepares and copies a temporary playback link from the split-button menu', async () => {
  api.getItem.mockResolvedValueOnce({ Id: 'movie-1', Name: 'Playable Movie', Type: 'Movie', IsFolder: false });
  api.getChildren.mockResolvedValueOnce([]);
  const user = userEvent.setup();
  const writeText = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue();
  renderItem('movie-1');

  await user.click(await screen.findByRole('button', { name: 'More playback options' }));
  await user.click(await screen.findByRole('menuitem', { name: 'Copy temporary playback link' }));

  expect(playback.getPlaybackInfo).toHaveBeenCalledWith('movie-1');
  expect(playback.issuePlaybackTicket).toHaveBeenCalledWith('movie-1', 'source-1', 'session-1');
  expect(writeText).toHaveBeenCalledWith(
    'http://localhost:3000/Videos/movie-1/stream?PlaybackTicket=ticket-value',
  );
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
        <Route element={<div>Built-in player destination</div>} path="/app/play/:id" />
      </Routes>
    </MemoryRouter>,
  );
}
