import { Toast } from '@heroui/react';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes, useLocation, useNavigate } from 'react-router-dom';

import { defaultTestAuthProvider, renderWithAdmin } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
import { LibraryEditPage } from './LibraryEditPage';
import { attachFilesystemFolder } from './filesystemApi';
import type { LibraryOption } from './libraryApi';
import {
  deleteLibrary,
  listLibraries,
  renameLibrary,
  updateLibraryPolicy,
} from './libraryApi';

vi.mock('./libraryApi', () => ({
  deleteLibrary: vi.fn(),
  listLibraries: vi.fn(),
  renameLibrary: vi.fn(),
  updateLibraryPolicy: vi.fn(),
}));
vi.mock('./filesystemApi', () => ({ attachFilesystemFolder: vi.fn() }));

const listMock = vi.mocked(listLibraries);
const renameMock = vi.mocked(renameLibrary);
const updateMock = vi.mocked(updateLibraryPolicy);
const deleteMock = vi.mocked(deleteLibrary);
const attachMock = vi.mocked(attachFilesystemFolder);
const libraryId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const otherLibraryId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';
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
  metadataSourceMode: 'automatic_scrape',
  localMetadataAccessMode: 'import',
  expansionPolicy: 'on_browse',
  probePolicy: 'on_playback',
} satisfies LibraryOption;
const shows = {
  ...movies,
  id: otherLibraryId,
  name: 'Shows',
  collectionType: 'tvshows',
} satisfies LibraryOption;

function renderEdit(authProvider = defaultTestAuthProvider, id = libraryId) {
  return renderWithAdmin(
    <>
      <Routes>
        <Route element={<LibraryEditPage />} path="/admin/libraries/:id" />
        <Route element={<h1>Libraries route</h1>} path="/admin/libraries" />
      </Routes>
      <AdminNotifications />
      <CurrentRoute />
      <RouteSwitcher />
    </>,
    { authProvider, initialEntries: [`/admin/libraries/${id}`], strict: true },
  );
}

function CurrentRoute() {
  const location = useLocation();
  return <span data-testid="current-route">{location.pathname}</span>;
}

function RouteSwitcher() {
  const navigate = useNavigate();
  return (
    <button onClick={() => { void navigate(`/admin/libraries/${otherLibraryId}`); }} type="button">
      Open another library
    </button>
  );
}

async function loadedNameInput() {
  return await screen.findByRole('textbox', { name: 'Library name' });
}

async function selectOption(user: ReturnType<typeof userEvent.setup>, label: string, option: string) {
  await user.click(screen.getByRole('button', { name: new RegExp(label, 'iu') }));
  await user.click(await screen.findByRole('option', { name: option }));
}

beforeEach(() => {
  listMock.mockReset();
  renameMock.mockReset();
  updateMock.mockReset();
  deleteMock.mockReset();
  attachMock.mockReset();
  listMock.mockResolvedValue([movies]);
  renameMock.mockResolvedValue(undefined);
  updateMock.mockResolvedValue(undefined);
  deleteMock.mockResolvedValue(undefined);
  attachMock.mockResolvedValue(undefined);
});

afterEach(() => { vi.restoreAllMocks(); });

it('loads a direct deep link with ordered sections and a Back breadcrumb', async () => {
  let finishLoad: ((records: LibraryOption[]) => void) | undefined;
  listMock.mockReturnValue(new Promise((resolve) => { finishLoad = resolve; }));
  renderEdit();

  expect(screen.getByRole('status', { name: 'Loading library settings' })).toBeVisible();
  await waitFor(() => { expect(listMock).toHaveBeenCalledOnce(); });
  finishLoad?.([movies]);
  await loadedNameInput();

  expect(screen.getByRole('link', { name: 'Libraries' })).toHaveAttribute('href', '/admin/libraries');
  const sectionHeadings = screen.getAllByRole('heading', { level: 2 }).map((heading) => heading.textContent);
  expect(sectionHeadings).toEqual([
    'Identity',
    'Scanning policy',
    'Media folders',
    'Danger zone',
  ]);
  expect(screen.getByText('3', { selector: 'dd' })).toBeVisible();
});

it('presents scanning policy as one ordered form with a structured effective-policy summary', async () => {
  renderEdit();
  const user = userEvent.setup();
  await loadedNameInput();

  const policySection = screen.getByRole('region', { name: 'Scanning policy' });
  const automatic = within(policySection).getByRole('radio', { name: /Automatic scrape/iu });
  const localOnly = within(policySection).getByRole('radio', { name: /Local metadata only/iu });
  expect(automatic).toBeChecked();
  await user.click(localOnly);
  expect(localOnly).toBeChecked();
  expect(automatic).not.toBeChecked();
  expect(within(policySection).getByRole('switch', { name: 'Enabled' })).toBeVisible();
  expect(within(policySection).getByRole('switch', { name: 'Override effective policy' })).toBeVisible();
  const summary = within(policySection).getByLabelText('Effective policy summary');
  expect(within(summary).getByText('Object selection').nextElementSibling).toHaveTextContent('Title layer');
  expect(within(summary).getByText('Metadata').nextElementSibling).toHaveTextContent('Basic metadata');
  expect(within(summary).getByText('Expansion').nextElementSibling).toHaveTextContent('On browse');
  expect(within(summary).getByText('Media probe').nextElementSibling).toHaveTextContent('On playback');
});

it('shows a safe initial error with Retry and an explicit not-found state', async () => {
  listMock
    .mockRejectedValueOnce(new Error('private-library-detail'))
    .mockResolvedValueOnce([]);
  renderEdit();
  const user = userEvent.setup();

  expect(await screen.findByRole('heading', { name: 'Unable to load this content' })).toBeVisible();
  expect(screen.queryByText('private-library-detail')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Retry' }));
  expect(await screen.findByRole('heading', { name: 'Library not found' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Back to Libraries' })).toHaveAttribute('href', '/admin/libraries');
});

it('renames identity without reloading or replacing an unsaved policy draft', async () => {
  renderEdit();
  const user = userEvent.setup();
  const name = await loadedNameInput();
  await selectOption(user, 'Scan profile', 'Manual');
  await user.clear(name);
  await user.type(name, 'Shows');
  await user.click(screen.getByRole('button', { name: 'Rename' }));

  expect(renameMock).toHaveBeenCalledWith('Movies', 'Shows');
  expect(listMock).toHaveBeenCalledOnce();
  await waitFor(() => { expect(screen.getByRole('heading', { name: 'Shows' })).toBeVisible(); });
  expect(screen.getByRole('button', { name: /Scan profile/iu })).toHaveTextContent('Manual');
});

it('clears the previous entity while a different route parameter is loading', async () => {
  let finishOtherLoad: ((records: LibraryOption[]) => void) | undefined;
  listMock
    .mockResolvedValueOnce([movies])
    .mockReturnValueOnce(new Promise((resolve) => { finishOtherLoad = resolve; }));
  renderEdit();
  const user = userEvent.setup();
  expect(await loadedNameInput()).toHaveValue('Movies');

  await user.click(screen.getByRole('button', { name: 'Open another library' }));

  expect(screen.getByRole('status', { name: 'Loading library settings' })).toBeVisible();
  expect(screen.queryByRole('textbox', { name: 'Library name' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Delete library' })).not.toBeInTheDocument();
  finishOtherLoad?.([shows]);
  expect(await loadedNameInput()).toHaveValue('Shows');
});

it('saves a named profile with the loaded version and omits advanced overrides', async () => {
  listMock
    .mockResolvedValueOnce([movies])
    .mockResolvedValueOnce([{ ...movies, enabled: false, scanProfile: 'Manual', profileVersion: 4 }]);
  renderEdit();
  const user = userEvent.setup();
  await loadedNameInput();
  await user.click(screen.getByRole('switch', { name: 'Enabled' }));
  await selectOption(user, 'Scan profile', 'Manual');
  await user.click(screen.getByRole('button', { name: 'Save scan policy' }));

  expect(updateMock).toHaveBeenCalledWith({
    id: libraryId,
    enabled: false,
    scanProfile: 'Manual',
    profileVersion: 3,
    metadataSourceMode: 'automatic_scrape',
    localMetadataAccessMode: 'import',
  });
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  expect(await screen.findByText('4', { selector: 'dd' })).toBeVisible();
});

it('sends all advanced policy values as one versioned update', async () => {
  renderEdit();
  const user = userEvent.setup();
  await loadedNameInput();
  await user.click(screen.getByRole('switch', { name: 'Override effective policy' }));
  await selectOption(user, 'Object selection', 'Library roots');
  await selectOption(user, 'Metadata', 'Full metadata');
  await selectOption(user, 'Expansion', 'Eager');
  await selectOption(user, 'Media probe', 'Eager');
  await user.click(screen.getByRole('button', { name: 'Save scan policy' }));

  expect(updateMock).toHaveBeenCalledWith({
    id: libraryId,
    enabled: true,
    scanProfile: 'Lazy',
    profileVersion: 3,
    metadataSourceMode: 'automatic_scrape',
    localMetadataAccessMode: 'import',
    effectivePolicy: {
      objectSelectionScope: 'library_roots',
      metadataPolicy: 'full',
      expansionPolicy: 'eager',
      probePolicy: 'eager',
    },
  });
});

it('attaches an absolute server folder path', async () => {
  renderEdit();
  const user = userEvent.setup();
  await loadedNameInput();

  await user.type(screen.getByRole('textbox', { name: 'Absolute folder path' }), '/mnt/media/Movies');
  await user.click(screen.getByRole('button', { name: 'Add folder' }));

  await waitFor(() => {
    expect(attachMock).toHaveBeenCalledWith(libraryId, '/mnt/media/Movies');
  });
});

it('does not let a policy refresh roll back a concurrent successful rename', async () => {
  let finishPolicyReload: ((records: LibraryOption[]) => void) | undefined;
  listMock
    .mockResolvedValueOnce([movies])
    .mockReturnValueOnce(new Promise((resolve) => { finishPolicyReload = resolve; }));
  renderEdit();
  const user = userEvent.setup();
  const name = await loadedNameInput();

  await user.click(screen.getByRole('button', { name: 'Save scan policy' }));
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  await user.clear(name);
  await user.type(name, 'Renamed Movies');
  await user.click(screen.getByRole('button', { name: 'Rename' }));
  await waitFor(() => { expect(screen.getByRole('heading', { name: 'Renamed Movies' })).toBeVisible(); });

  finishPolicyReload?.([{ ...movies, profileVersion: 4 }]);
  await waitFor(() => { expect(screen.getByText('4', { selector: 'dd' })).toBeVisible(); });
  expect(name).toHaveValue('Renamed Movies');
  expect(screen.getByRole('heading', { name: 'Renamed Movies' })).toBeVisible();
});

it('keeps every draft field after 409 and replaces it only through Reload latest', async () => {
  const latest = {
    ...movies,
    name: 'Server Movies',
    enabled: true,
    scanProfile: 'Manual',
    profileVersion: 5,
  } satisfies LibraryOption;
  listMock.mockResolvedValueOnce([movies]).mockResolvedValueOnce([latest]);
  updateMock.mockRejectedValue({ status: 409, category: 'conflict', message: 'private-conflict-detail' });
  renderEdit();
  const user = userEvent.setup();
  const name = await loadedNameInput();
  await user.clear(name);
  await user.type(name, 'Local name');
  await user.click(screen.getByRole('switch', { name: 'Enabled' }));
  await selectOption(user, 'Scan profile', 'Full');
  await user.click(screen.getByRole('switch', { name: 'Override effective policy' }));
  await selectOption(user, 'Expansion', 'Eager');
  await user.click(screen.getByRole('button', { name: 'Save scan policy' }));

  expect(await screen.findByRole('alert')).toHaveTextContent('Your draft is intact');
  expect(name).toHaveValue('Local name');
  expect(screen.getByRole('switch', { name: 'Enabled' })).not.toBeChecked();
  expect(screen.getByRole('button', { name: /Scan profile/iu })).toHaveTextContent('Full');
  expect(screen.getByRole('button', { name: /Expansion/iu })).toHaveTextContent('Eager');
  expect(screen.getByRole('button', { name: 'Save scan policy' })).toBeDisabled();
  expect(updateMock).toHaveBeenCalledOnce();
  expect(screen.queryByText('private-conflict-detail')).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Reload latest' }));
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  await waitFor(() => { expect(name).toHaveValue('Server Movies'); });
  expect(screen.getByRole('button', { name: /Scan profile/iu })).toHaveTextContent('Manual');
  expect(screen.queryByText('Your draft is intact')).not.toBeInTheDocument();
});

it('retains a full draft and current record after an explicit refresh fails', async () => {
  listMock
    .mockResolvedValueOnce([movies])
    .mockRejectedValueOnce(new Error('private-refresh-detail'));
  renderEdit();
  const user = userEvent.setup();
  const name = await loadedNameInput();
  await user.clear(name);
  await user.type(name, 'Unsaved name');
  await selectOption(user, 'Scan profile', 'Manual');

  await user.click(screen.getByRole('button', { name: 'Reload library' }));

  expect(await screen.findByText('Showing the last available data')).toBeVisible();
  expect(name).toHaveValue('Unsaved name');
  expect(screen.getByRole('button', { name: /Scan profile/iu })).toHaveTextContent('Manual');
  expect(screen.queryByText('private-refresh-detail')).not.toBeInTheDocument();
});

it('keeps rename pending local to identity controls', async () => {
  let finishRename: (() => void) | undefined;
  renameMock.mockReturnValue(new Promise((resolve) => { finishRename = resolve; }));
  renderEdit();
  const user = userEvent.setup();
  const name = await loadedNameInput();
  await user.clear(name);
  await user.type(name, 'Shows');
  await user.click(screen.getByRole('button', { name: 'Rename' }));

  expect(screen.getByRole('button', { name: 'Rename' })).toHaveAttribute('data-pending', 'true');
  expect(screen.getByRole('button', { name: 'Save scan policy' })).toBeEnabled();
  expect(screen.getByRole('button', { name: 'Delete library' })).toBeDisabled();
  expect(deleteMock).not.toHaveBeenCalled();
  finishRename?.();
  await waitFor(() => { expect(screen.getByRole('button', { name: 'Rename' })).not.toHaveAttribute('data-pending'); });
});

it('keeps delete confirmation open on failure and redirects only after success', async () => {
  deleteMock.mockRejectedValueOnce(new Error('private-delete-detail')).mockResolvedValueOnce(undefined);
  renderEdit();
  const user = userEvent.setup();
  await loadedNameInput();

  await user.click(screen.getByRole('button', { name: 'Delete library' }));
  let dialog = screen.getByRole('dialog', { name: 'Delete Movies?' });
  await user.click(within(dialog).getByRole('button', { name: 'Delete library' }));
  expect(await within(dialog).findByText('Review the current state and try again.')).toBeVisible();
  expect(screen.queryByText('private-delete-detail')).not.toBeInTheDocument();
  await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));

  await user.click(screen.getByRole('button', { name: 'Delete library' }));
  dialog = screen.getByRole('dialog', { name: 'Delete Movies?' });
  await user.click(within(dialog).getByRole('button', { name: 'Delete library' }));

  expect(deleteMock).toHaveBeenLastCalledWith('Movies');
  expect(await screen.findByRole('heading', { name: 'Libraries route' })).toBeVisible();
  expect(screen.getByTestId('current-route')).toHaveTextContent('/admin/libraries');
});

it('routes policy authorization failure through logout without local feedback', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  const logout = vi.fn().mockResolvedValue(undefined);
  const checkError = vi.fn().mockRejectedValue({ message: false });
  updateMock.mockRejectedValue({ status: 401, message: 'private-policy-auth-detail' });
  renderEdit({ ...defaultTestAuthProvider, checkError, logout });
  const user = userEvent.setup();
  await loadedNameInput();

  await user.click(screen.getByRole('button', { name: 'Save scan policy' }));

  await waitFor(() => { expect(logout).toHaveBeenCalled(); });
  await waitFor(() => { expect(screen.getByTestId('current-route')).toHaveTextContent('/admin/login'); });
  expect(dangerToast).not.toHaveBeenCalled();
  expect(screen.queryByText('private-policy-auth-detail')).not.toBeInTheDocument();
});
