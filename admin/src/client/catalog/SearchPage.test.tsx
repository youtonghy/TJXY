import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';

import { SearchPage } from './SearchPage';

const api = vi.hoisted(() => ({
  getPopular: vi.fn(),
  searchHints: vi.fn(),
}));

vi.mock('../api/catalogApi', () => api);
vi.mock('../ui/MediaImage', () => ({
  MediaImage: ({ alt }: { alt: string }) => <div aria-label={alt} role="img" />,
}));

beforeEach(() => {
  api.getPopular.mockResolvedValue([
    { Id: 'popular-1', Name: 'Popular title', Type: 'Movie', ProductionYear: 2025 },
  ]);
  api.searchHints.mockResolvedValue([]);
});

it('shows the same three-part page heading used by the other catalog pages', () => {
  render(<MemoryRouter initialEntries={['/app/search']}><SearchPage /></MemoryRouter>);

  expect(screen.getByText('Explore your library')).toBeVisible();
  expect(screen.getByRole('heading', { level: 1, name: 'Search' })).toBeVisible();
  expect(screen.getByText('Find something to watch.')).toBeVisible();
});

it('shows popular recommendations before a search is entered', async () => {
  render(<MemoryRouter initialEntries={['/app/search']}><SearchPage /></MemoryRouter>);

  expect(await screen.findByRole('heading', { name: 'Popular recommendations' })).toBeVisible();
  expect(screen.getByRole('link', { name: /Popular title/ })).toHaveAttribute(
    'href',
    '/app/items/popular-1',
  );
  expect(screen.queryByRole('link', { name: 'View all' })).not.toBeInTheDocument();
  expect(api.getPopular).toHaveBeenCalledWith(12);
});
