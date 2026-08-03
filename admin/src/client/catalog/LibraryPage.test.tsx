import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';

import { LibraryPage } from './LibraryPage';

const api = vi.hoisted(() => ({ getItems: vi.fn(), getLibraryFilterFacets: vi.fn() }));
vi.mock('../api/catalogApi', () => api);
vi.mock('../ui/MediaTile', () => ({ MediaTile: ({ item }: { item: { Name: string } }) => <div>{item.Name}</div> }));

beforeEach(() => {
  api.getLibraryFilterFacets.mockResolvedValue({ Genres: ['Drama', 'Science Fiction'], ProductionYears: [2021, 2016] });
  api.getItems.mockResolvedValue({
    Items: [{ Genres: ['Drama'], Id: 'movie-1', Name: 'Arrival', ProductionYear: 2016, Type: 'Movie' }],
    StartIndex: 0,
    TotalRecordCount: 1,
  });
});

it('keeps CellSelect filters collapsed by default and sends expanded selections to the paged server query', async () => {
  const user = userEvent.setup();
  render(
    <MemoryRouter initialEntries={['/app/libraries/library-1']}>
      <Routes><Route element={<><LibraryPage /><LocationProbe /></>} path="/app/libraries/:id" /></Routes>
    </MemoryRouter>,
  );

  expect(await screen.findByText('Arrival')).toBeVisible();
  expect(screen.queryByRole('button', { name: /Media type/ })).not.toBeInTheDocument();
  const filterRegion = screen.getByRole('region', { name: 'Library filters' });
  const filters = filterRegion.querySelector('[data-slot="disclosure-group"]');
  expect(filters).toHaveClass('rounded-lg');

  await user.click(screen.getByRole('button', { name: 'Filter titles' }));
  await chooseCellOption(user, /Media type/, 'Movies');
  await chooseCellOption(user, /Genre/, 'Drama');
  await chooseCellOption(user, /Year/, '2016');
  await chooseCellOption(user, /Sort by/, 'Newest release');

  await waitFor(() => {
    expect(api.getItems).toHaveBeenLastCalledWith(expect.objectContaining({
      genre: 'Drama',
      includeItemTypes: 'Movie',
      productionYear: 2016,
      recursive: true,
      sortBy: 'ProductionYear',
      sortOrder: 'Descending',
      startIndex: 0,
    }));
  });
  expect(screen.getByTestId('location')).toHaveTextContent('type=Movie');
  expect(screen.getByTestId('location')).toHaveTextContent('genre=Drama');
  expect(screen.getByTestId('location')).toHaveTextContent('year=2016');
  expect(screen.getByRole('button', { name: 'Movies Media type' })).toBeVisible();
});

async function chooseCellOption(user: ReturnType<typeof userEvent.setup>, triggerName: RegExp, optionName: string) {
  await user.click(screen.getByRole('button', { name: triggerName }));
  await user.click(await screen.findByRole('option', { name: optionName }));
}

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.search}</output>;
}
