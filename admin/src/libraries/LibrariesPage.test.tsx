import { ThemeProvider } from '@mui/material/styles';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { theme } from '../theme';
import {
  createLibrary,
  deleteLibrary,
  listLibraries,
  renameLibrary,
  updateLibraryPolicy,
} from './libraryApi';
import type { LibraryOption } from './libraryApi';
import { LibrariesPage } from './LibrariesPage';

const notify = vi.fn();
vi.mock('react-admin', () => ({
  Title: ({ title }: { title: string }) => <title>{title}</title>,
  useNotify: () => notify,
}));
vi.mock('./libraryApi', () => ({
  createLibrary: vi.fn(),
  deleteLibrary: vi.fn(),
  listLibraries: vi.fn(),
  renameLibrary: vi.fn(),
  updateLibraryPolicy: vi.fn(),
}));
vi.mock('./HybridCandidatesDialog', () => ({
  HybridCandidatesDialog: ({ library }: { library: LibraryOption }) => (
    <div role="dialog">Candidates for {library.name}</div>
  ),
}));

const listMock = vi.mocked(listLibraries);
const createMock = vi.mocked(createLibrary);
const renameMock = vi.mocked(renameLibrary);
const updateMock = vi.mocked(updateLibraryPolicy);
const deleteMock = vi.mocked(deleteLibrary);

const movies = {
  id: 'library-1',
  name: 'Movies',
  collectionType: 'movies',
  locations: ['tjxy://storage-root/root-1'],
  enabled: true,
  scanProfile: 'Lazy',
  profileVersion: 3,
  objectSelectionScope: 'title_layer',
  metadataPolicy: 'basic',
  expansionPolicy: 'on_browse',
  probePolicy: 'on_playback',
} satisfies LibraryOption;

beforeEach(() => {
  notify.mockReset();
  listMock.mockReset();
  createMock.mockReset();
  renameMock.mockReset();
  updateMock.mockReset();
  deleteMock.mockReset();
  listMock.mockResolvedValue([movies]);
  createMock.mockResolvedValue(undefined);
  renameMock.mockResolvedValue(undefined);
  updateMock.mockResolvedValue(undefined);
  deleteMock.mockResolvedValue(undefined);
});

it('creates an empty library and reloads the authoritative list', async () => {
  render(<ThemeProvider theme={theme}><LibrariesPage /></ThemeProvider>);
  const user = userEvent.setup();

  expect(await screen.findByRole('rowheader', { name: 'Movies' })).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Add library' }));
  await user.type(screen.getByRole('textbox', { name: 'Library name' }), 'Shows');
  await user.click(screen.getByRole('button', { name: 'Create library' }));

  expect(createMock).toHaveBeenCalledWith({
    name: 'Shows', collectionType: 'mixed', enabled: true, scanProfile: 'Lazy',
  });
  await waitFor(() => {
    expect(listMock).toHaveBeenCalledTimes(2);
  });
});

it('saves a named profile with CAS and omits advanced overrides', async () => {
  render(<ThemeProvider theme={theme}><LibrariesPage /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByRole('rowheader', { name: 'Movies' });
  await user.click(screen.getByRole('button', { name: 'Edit Movies' }));
  await user.click(screen.getByRole('combobox', { name: 'Scan profile' }));
  await user.click(screen.getByRole('option', { name: 'Hybrid' }));
  await user.click(screen.getByRole('button', { name: 'Save scan policy' }));

  expect(updateMock).toHaveBeenCalledWith({
    id: 'library-1', enabled: true, scanProfile: 'Hybrid', profileVersion: 3,
  });
  await waitFor(() => {
    expect(listMock).toHaveBeenCalledTimes(2);
  });
});

it('opens candidate management for active and dormant library preferences', async () => {
  const hybrid = {
    ...movies,
    id: 'library-2',
    name: 'Shows',
    scanProfile: 'Hybrid',
    expansionPolicy: 'background',
  } satisfies LibraryOption;
  listMock.mockResolvedValue([movies, hybrid]);
  render(<ThemeProvider theme={theme}><LibrariesPage /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByRole('rowheader', { name: 'Shows' });
  expect(screen.getByRole('button', { name: 'Manage background candidates for Movies' }))
    .toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Manage background candidates for Shows' }));
  expect(screen.getByRole('dialog')).toHaveTextContent('Candidates for Shows');
});
