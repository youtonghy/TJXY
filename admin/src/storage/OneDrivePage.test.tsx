import { ThemeProvider } from '@mui/material/styles';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { theme } from '../theme';
import { OneDrivePage } from './OneDrivePage';
import {
  bindOneDrive,
  listLibraries,
  listOneDriveDirectories,
  startOneDriveOAuth,
} from './googleDriveApi';

const notify = vi.fn();
vi.mock('react-admin', () => ({
  Title: ({ title }: { title: string }) => <title>{title}</title>,
  useNotify: () => notify,
}));
vi.mock('./googleDriveApi', () => ({
  bindOneDrive: vi.fn(),
  listLibraries: vi.fn(),
  listOneDriveDirectories: vi.fn(),
  startOneDriveOAuth: vi.fn(),
}));

const librariesMock = vi.mocked(listLibraries);
const startMock = vi.mocked(startOneDriveOAuth);
const directoriesMock = vi.mocked(listOneDriveDirectories);
const bindMock = vi.mocked(bindOneDrive);
let openMock: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  notify.mockReset();
  librariesMock.mockReset();
  startMock.mockReset();
  directoriesMock.mockReset();
  bindMock.mockReset();
  librariesMock.mockResolvedValue([{
    id: 'library-1',
    name: 'Movies',
    collectionType: 'movies',
    locations: [],
    enabled: true,
    scanProfile: 'Lazy',
    profileVersion: 1,
    objectSelectionScope: 'title_layer',
    metadataPolicy: 'basic',
    expansionPolicy: 'on_browse',
    probePolicy: 'on_playback',
  }]);
  startMock.mockResolvedValue({ state: 'oauth-state', authorizationUrl: 'https://login.microsoftonline.com/auth' });
  directoriesMock
    .mockResolvedValueOnce({ items: [{ id: 'folder-1', name: 'Shows' }], nextPageToken: null })
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  bindMock.mockResolvedValue({
    accountId: 'account-1', rootId: 'root-1', initialSyncJobId: 'job-1', restartRequired: false,
  });
  openMock = vi.spyOn(window, 'open').mockReturnValue({} as Window);
});

afterEach(() => {
  vi.restoreAllMocks();
});

it('authorizes, browses, and binds a OneDrive folder without client credentials', async () => {
  render(<ThemeProvider theme={theme}><OneDrivePage /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByRole('combobox', { name: 'Target library' });
  await user.click(screen.getByRole('button', { name: 'Authorize OneDrive' }));
  expect(startMock).toHaveBeenCalledWith('library-1');
  expect(openMock).toHaveBeenCalledWith(
    'https://login.microsoftonline.com/auth',
    'tjxy-onedrive-oauth',
    'noopener,noreferrer',
  );

  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  expect(await screen.findByRole('button', { name: 'Shows' })).toBeVisible();
  expect(directoriesMock).toHaveBeenCalledWith('oauth-state');

  await user.click(screen.getByRole('button', { name: 'Shows' }));
  await waitFor(() => {
    expect(directoriesMock).toHaveBeenCalledWith('oauth-state', { parentId: 'folder-1' });
  });
  await user.click(screen.getByRole('button', { name: 'Add OneDrive' }));

  expect(bindMock).toHaveBeenCalledWith('oauth-state', {
    displayName: 'Movies', rootObjectId: 'folder-1',
  });
  expect(await screen.findByText(/OneDrive is ready/)).toBeVisible();
});

it('keeps the wizard retryable before the callback is ready', async () => {
  directoriesMock.mockReset();
  directoriesMock.mockRejectedValue({ category: 'conflict' });
  render(<ThemeProvider theme={theme}><OneDrivePage /></ThemeProvider>);
  const user = userEvent.setup();
  await screen.findByRole('combobox', { name: 'Target library' });
  await waitFor(() => {
    expect(screen.getByRole('button', { name: 'Authorize OneDrive' })).toBeEnabled();
  });
  await user.click(screen.getByRole('button', { name: 'Authorize OneDrive' }));
  await waitFor(() => {
    expect(screen.getByRole('button', { name: 'Check authorization' })).toBeEnabled();
  });
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));

  await waitFor(() => {
    expect(notify).toHaveBeenCalledWith('Microsoft authorization has not completed yet.', { type: 'warning' });
  });
  expect(screen.getByRole('button', { name: 'Check authorization' })).toBeEnabled();
});

it('continues from an empty folder page and renders the next page', async () => {
  const cursor = '028f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({ items: [], nextPageToken: cursor })
    .mockResolvedValueOnce({
      items: [{ id: 'folder-1', name: 'Shows' }],
      nextPageToken: null,
    });
  render(<ThemeProvider theme={theme}><OneDrivePage /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByRole('combobox', { name: 'Target library' });
  await user.click(screen.getByRole('button', { name: 'Authorize OneDrive' }));
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  expect(await screen.findByText('No folders on this page.')).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Load more folders' }));

  expect(directoriesMock).toHaveBeenLastCalledWith('oauth-state', { pageToken: cursor });
  expect(await screen.findByRole('button', { name: 'Shows' })).toBeVisible();
  expect(screen.queryByRole('button', { name: 'Load more folders' })).not.toBeInTheDocument();
});

it('retains the same cursor and visible folders after a load-more failure', async () => {
  const cursor = '038f17ac-4e99-7ec5-b4fd-8f15ca9f4f13';
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({
      items: [{ id: 'folder-1', name: 'Shows' }],
      nextPageToken: cursor,
    })
    .mockRejectedValueOnce(new Error('Temporary failure'))
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  render(<ThemeProvider theme={theme}><OneDrivePage /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByRole('combobox', { name: 'Target library' });
  await user.click(screen.getByRole('button', { name: 'Authorize OneDrive' }));
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Load more folders' }));
  expect(await screen.findByRole('button', { name: 'Shows' })).toBeVisible();
  expect(notify).toHaveBeenCalledWith(expect.any(String), { type: 'error' });

  await user.click(screen.getByRole('button', { name: 'Load more folders' }));
  await waitFor(() => {
    expect(directoriesMock).toHaveBeenCalledTimes(3);
  });
  expect(directoriesMock).toHaveBeenNthCalledWith(2, 'oauth-state', { pageToken: cursor });
  expect(directoriesMock).toHaveBeenNthCalledWith(3, 'oauth-state', { pageToken: cursor });
});

it('uses the current folder cursor after navigation', async () => {
  const rootCursor = '048f17ac-4e99-7ec5-b4fd-8f15ca9f4f14';
  const folderCursor = '058f17ac-4e99-7ec5-b4fd-8f15ca9f4f15';
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({
      items: [{ id: 'folder-1', name: 'Shows' }],
      nextPageToken: rootCursor,
    })
    .mockResolvedValueOnce({ items: [], nextPageToken: folderCursor })
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  render(<ThemeProvider theme={theme}><OneDrivePage /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByRole('combobox', { name: 'Target library' });
  await user.click(screen.getByRole('button', { name: 'Authorize OneDrive' }));
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Shows' }));
  await user.click(await screen.findByRole('button', { name: 'Load more folders' }));

  expect(directoriesMock).toHaveBeenLastCalledWith('oauth-state', {
    parentId: 'folder-1', pageToken: folderCursor,
  });
});
