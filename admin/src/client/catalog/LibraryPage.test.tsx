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

it('uses controlled NativeSelect filters and sends them to the paged server query', async () => {
  const user = userEvent.setup();
  render(
    <MemoryRouter initialEntries={['/app/libraries/library-1']}>
      <Routes><Route element={<><LibraryPage /><LocationProbe /></>} path="/app/libraries/:id" /></Routes>
    </MemoryRouter>,
  );

  expect(await screen.findByText('Arrival')).toBeVisible();
  expect(await screen.findByRole('option', { name: 'Science Fiction' })).toBeVisible();
  expect(await screen.findByRole('option', { name: '2021' })).toBeVisible();
  await user.selectOptions(screen.getByRole('combobox', { name: 'Media type' }), 'Movie');
  await user.selectOptions(screen.getByRole('combobox', { name: 'Genre' }), 'Drama');
  await user.selectOptions(screen.getByRole('combobox', { name: 'Year' }), '2016');
  await user.selectOptions(screen.getByRole('combobox', { name: 'Sort by' }), 'ProductionYear:Descending');

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
});

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.search}</output>;
}
