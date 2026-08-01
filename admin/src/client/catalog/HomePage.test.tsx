import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';

import { HomePage } from './HomePage';

const api = vi.hoisted(() => ({
  getLatest: vi.fn(),
  getLibraries: vi.fn(),
  getResumeItems: vi.fn(),
  latestTypesForLibrary: (library: { CollectionType?: string }) => library.CollectionType === 'movies' ? 'Movie' : library.CollectionType === 'tvshows' ? 'Series' : undefined,
}));

vi.mock('../api/catalogApi', () => api);
vi.mock('../ui/MediaImage', () => ({
  MediaImage: ({ alt }: { alt: string }) => <div aria-label={alt} role="img" />,
}));

beforeEach(() => {
  api.getResumeItems.mockResolvedValue([
    {
      Id: 'resume-1',
      Name: 'Continue movie',
      Type: 'Movie',
      RunTimeTicks: 1_000,
      UserData: { PlaybackPositionTicks: 400 },
    },
  ]);
  api.getLatest.mockImplementation((options?: { parentId?: string }) => Promise.resolve(
    options?.parentId === 'movies'
      ? Array.from({ length: 12 }, (_, index) => ({
        Id: `movie-${String(index + 1)}`,
        Name: `Movie ${String(index + 1)}`,
        Type: 'Movie',
      }))
      : [{ Id: 'series-1', Name: 'Series 1', Type: 'Series' }],
  ));
  api.getLibraries.mockResolvedValue([
    { Id: 'movies', Name: 'Movies', CollectionType: 'movies' },
    { Id: 'series', Name: 'TV shows', CollectionType: 'tvshows' },
  ]);
});

it('places resumable titles before library rows and links each library to its full collection', async () => {
  render(<MemoryRouter><HomePage /></MemoryRouter>);

  const continueHeading = await screen.findByRole('heading', { name: 'Continue watching' });
  const moviesHeading = screen.getByRole('heading', { name: 'Movies' });
  expect(continueHeading.compareDocumentPosition(moviesHeading) & Node.DOCUMENT_POSITION_FOLLOWING)
    .toBeTruthy();
  expect(screen.queryByRole('heading', { name: 'Recently added' })).not.toBeInTheDocument();
  expect(api.getLatest).toHaveBeenCalledWith({
    includeItemTypes: 'Movie',
    limit: 12,
    parentId: 'movies',
  });
  expect(api.getLatest).toHaveBeenCalledWith({
    includeItemTypes: 'Series',
    limit: 12,
    parentId: 'series',
  });
  expect(screen.getAllByRole('link', { name: 'View all' }).map((link) => link.getAttribute('href')))
    .toEqual(['/app/libraries/movies', '/app/libraries/series']);
  expect(screen.getByRole('link', { name: /Continue movie/ })).toHaveAttribute(
    'href',
    '/app/play/resume-1',
  );
  expect(screen.getByRole('progressbar', { name: '40% watched' })).toBeVisible();
});

it('omits the continue watching row when no resumable title exists', async () => {
  api.getResumeItems.mockResolvedValueOnce([]);

  render(<MemoryRouter><HomePage /></MemoryRouter>);

  expect(await screen.findByRole('heading', { name: 'Movies' })).toBeVisible();
  expect(screen.queryByRole('heading', { name: 'Continue watching' })).not.toBeInTheDocument();
});

it('limits each expanded library to two responsive rows', async () => {
  render(<MemoryRouter><HomePage /></MemoryRouter>);

  expect(await screen.findByRole('heading', { name: 'Movies' })).toBeVisible();
  expect(screen.getByRole('link', { name: /Movie 4/u }).parentElement).not.toHaveClass('hidden');
  expect(screen.getByRole('link', { name: /Movie 5/u }).parentElement)
    .toHaveClass('hidden', 'sm:block');
  expect(screen.getByRole('link', { name: /Movie 9/u }).parentElement)
    .toHaveClass('hidden', 'lg:block');
});

it('keeps successful library rows visible when another library fails', async () => {
  api.getLatest.mockImplementation((options?: { parentId?: string }) => (
    options?.parentId === 'series'
      ? Promise.reject(new Error('series unavailable'))
      : Promise.resolve([{ Id: 'movie-1', Name: 'Movie 1', Type: 'Movie' }])
  ));

  render(<MemoryRouter><HomePage /></MemoryRouter>);

  expect(await screen.findByRole('heading', { name: 'Movies' })).toBeVisible();
  expect(screen.queryByRole('heading', { name: 'TV shows' })).not.toBeInTheDocument();
  expect(screen.getByRole('alert')).toHaveTextContent('Some library sections are unavailable');
});
