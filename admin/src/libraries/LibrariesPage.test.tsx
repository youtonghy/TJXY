import { Toast } from '@heroui/react';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useLocation } from 'react-router-dom';

import { defaultTestAuthProvider, renderWithAdmin } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
import { LibrariesPage } from './LibrariesPage';
import type { LibraryOption } from './libraryApi';
import { createLibrary, listLibraries } from './libraryApi';

vi.mock('./libraryApi', () => ({
  createLibrary: vi.fn(),
  listLibraries: vi.fn(),
}));

const listMock = vi.mocked(listLibraries);
const createMock = vi.mocked(createLibrary);
const libraryId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const movies = {
  id: libraryId,
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

function renderLibraries(authProvider = defaultTestAuthProvider) {
  return renderWithAdmin(
    <>
      <LibrariesPage />
      <AdminNotifications />
      <CurrentRoute />
    </>,
    { authProvider, initialEntries: ['/admin/libraries'], strict: true },
  );
}

function CurrentRoute() {
  const location = useLocation();
  return <span data-testid="current-route">{location.pathname}</span>;
}

beforeEach(() => {
  listMock.mockReset();
  createMock.mockReset();
  listMock.mockResolvedValue([movies]);
  createMock.mockResolvedValue(undefined);
});

afterEach(() => { vi.restoreAllMocks(); });

it('renders a stable skeleton followed by readable desktop and mobile records', async () => {
  let finishLoad: ((records: LibraryOption[]) => void) | undefined;
  listMock.mockReturnValue(new Promise((resolve) => { finishLoad = resolve; }));
  renderLibraries();

  expect(screen.getByRole('status', { name: 'Loading libraries' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Reload libraries' })).toHaveAttribute('data-pending', 'true');
  finishLoad?.([movies]);

  const grid = await screen.findByRole('grid', { name: 'Libraries' });
  expect(grid).toHaveClass('table-fixed');
  expect(within(grid).getByText('Title layer / Basic metadata / On browse / On playback')).toBeVisible();
  expect(within(grid).getByText('Enabled')).toBeVisible();
  const edit = within(grid).getByRole('link', { name: 'Edit Movies' });
  expect(edit).toHaveAttribute('href', `/admin/libraries/${libraryId}`);

  const mobile = screen.getByRole('list', { name: 'Libraries mobile' });
  const record = within(mobile).getByRole('listitem', { name: 'Movies' });
  expect(record).toHaveTextContent('Scan profile');
  expect(record).toHaveTextContent('Storage roots');
});

it('distinguishes an initial error, retry success, and a proven empty collection', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  listMock
    .mockRejectedValueOnce(new Error('private-library-load-detail'))
    .mockResolvedValueOnce([]);
  renderLibraries();
  const user = userEvent.setup();

  expect(await screen.findByRole('heading', { name: 'Unable to load this content' })).toBeVisible();
  expect(screen.queryByText('private-library-load-detail')).not.toBeInTheDocument();
  expect(dangerToast).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Retry' }));
  expect(await screen.findByRole('heading', { name: 'No libraries are configured.' })).toBeVisible();
});

it('keeps existing records and shows an inline stale warning after refresh failure', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  listMock
    .mockResolvedValueOnce([movies])
    .mockRejectedValueOnce(new Error('private-library-refresh-detail'));
  renderLibraries();
  const user = userEvent.setup();
  const grid = await screen.findByRole('grid', { name: 'Libraries' });

  await user.click(screen.getByRole('button', { name: 'Reload libraries' }));

  expect(await screen.findByText('Showing the last available data')).toBeVisible();
  expect(within(grid).getByRole('rowheader')).toHaveTextContent('Movies');
  expect(screen.queryByText('private-library-refresh-detail')).not.toBeInTheDocument();
  expect(dangerToast).not.toHaveBeenCalled();
});

it('creates with approved defaults, closes the modal, and reloads the authoritative list', async () => {
  listMock.mockResolvedValueOnce([movies]).mockResolvedValueOnce([movies]);
  renderLibraries();
  const user = userEvent.setup();
  await screen.findByRole('grid', { name: 'Libraries' });

  await user.click(screen.getByRole('button', { name: 'Add library' }));
  await user.type(screen.getByRole('textbox', { name: 'Library name' }), 'Shows');
  await user.click(screen.getByRole('button', { name: 'Create library' }));

  expect(createMock).toHaveBeenCalledWith({
    name: 'Shows',
    collectionType: 'mixed',
    enabled: true,
    scanProfile: 'Lazy',
  });
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  await waitFor(() => {
    expect(screen.queryByRole('dialog', { name: 'Add library' })).not.toBeInTheDocument();
  });
});

it('preserves the create draft and reports only safe copy after failure', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('create-error');
  createMock.mockRejectedValue(new Error('private-create-detail'));
  renderLibraries();
  const user = userEvent.setup();
  await screen.findByRole('grid', { name: 'Libraries' });

  await user.click(screen.getByRole('button', { name: 'Add library' }));
  const name = screen.getByRole('textbox', { name: 'Library name' });
  await user.type(name, 'Shows');
  await user.click(screen.getByRole('button', { name: 'Create library' }));

  await waitFor(() => {
    expect(dangerToast).toHaveBeenCalledWith('The library could not be created.', expect.any(Object));
  });
  expect(name).toHaveValue('Shows');
  expect(screen.getByRole('dialog', { name: 'Add library' })).toBeVisible();
  expect(screen.queryByText('private-create-detail')).not.toBeInTheDocument();
});

it('routes a list authorization failure to access denied without a local error', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  const checkError = vi.fn().mockRejectedValue({
    logoutUser: false,
    message: false,
    redirectTo: '/admin/access-denied',
  });
  listMock.mockRejectedValue({ status: 403, message: 'private-list-auth-detail' });
  renderLibraries({ ...defaultTestAuthProvider, checkError });

  await waitFor(() => { expect(screen.getByTestId('current-route')).toHaveTextContent('/admin/access-denied'); });
  expect(dangerToast).not.toHaveBeenCalled();
  expect(screen.queryByText('private-list-auth-detail')).not.toBeInTheDocument();
});
