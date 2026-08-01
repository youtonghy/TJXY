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

it('shows popular recommendations before a search is entered', async () => {
  render(<MemoryRouter initialEntries={['/app/search']}><SearchPage /></MemoryRouter>);

  expect(await screen.findByRole('heading', { name: 'Popular recommendations' })).toBeVisible();
  expect(screen.getByRole('link', { name: /Popular title/ })).toHaveAttribute(
    'href',
    '/app/items/popular-1',
  );
  expect(api.getPopular).toHaveBeenCalledWith(12);
});
