import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';

import { LibrariesPage } from './LibrariesPage';

const api = vi.hoisted(() => ({
  getLatest: vi.fn(),
  getLibraries: vi.fn(),
  latestTypesForLibrary: (library: { CollectionType?: string }) => (
    library.CollectionType === 'movies' ? 'Movie' : library.CollectionType === 'tvshows' ? 'Series' : undefined
  ),
}));

vi.mock('../api/catalogApi', () => api);
vi.mock('../ui/MediaImage', () => ({
  MediaImage: ({ alt }: { alt: string }) => <div aria-label={alt} role="img" />,
}));

beforeEach(() => {
  api.getLibraries.mockResolvedValue([
    { Id: 'movies', Name: 'Movies', CollectionType: 'movies' },
    { Id: 'series', Name: 'TV Shows', CollectionType: 'tvshows' },
  ]);
  api.getLatest.mockImplementation(({ parentId }: { parentId: string }) => Promise.resolve(
    Array.from({ length: 12 }, (_, index) => ({
      Id: `${parentId}-${String(index)}`,
      Name: `${parentId} ${String(index)}`,
      Type: parentId === 'movies' ? 'Movie' : 'Series',
    })),
  ));
});

it('expands each library into a two-row media section with View all', async () => {
  render(<MemoryRouter><LibrariesPage /></MemoryRouter>);

  expect(await screen.findByRole('heading', { name: 'Movies' })).toBeVisible();
  expect(screen.getByRole('heading', { name: 'TV Shows' })).toBeVisible();
  expect(screen.getAllByRole('link', { name: 'View all' }).map((link) => link.getAttribute('href')))
    .toEqual(['/app/libraries/movies', '/app/libraries/series']);
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
});
