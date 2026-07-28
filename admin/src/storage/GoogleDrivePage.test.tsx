import { ThemeProvider } from '@mui/material/styles';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { theme } from '../theme';
import { GoogleDrivePage } from './GoogleDrivePage';
import {
  bindGoogleDrive,
  listGoogleDirectories,
  listLibraries,
  listSharedDrives,
  startGoogleDriveOAuth,
} from './googleDriveApi';

const notify = vi.fn();
vi.mock('react-admin', () => ({
  Title: ({ title }: { title: string }) => <title>{title}</title>,
  useNotify: () => notify,
}));
vi.mock('./googleDriveApi', () => ({
  bindGoogleDrive: vi.fn(),
  listGoogleDirectories: vi.fn(),
  listLibraries: vi.fn(),
  listSharedDrives: vi.fn(),
  startGoogleDriveOAuth: vi.fn(),
}));

const librariesMock = vi.mocked(listLibraries);
const startMock = vi.mocked(startGoogleDriveOAuth);
const sharedDrivesMock = vi.mocked(listSharedDrives);
const directoriesMock = vi.mocked(listGoogleDirectories);
const bindMock = vi.mocked(bindGoogleDrive);
let openMock: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  notify.mockReset();
  librariesMock.mockReset();
  startMock.mockReset();
  sharedDrivesMock.mockReset();
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
  startMock.mockResolvedValue({ state: 'oauth-state', authorizationUrl: 'https://accounts.google.com/auth' });
  sharedDrivesMock.mockResolvedValue({ items: [], nextPageToken: null });
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

it('authorizes, browses My Drive, and binds the current folder without credentials', async () => {
  render(<ThemeProvider theme={theme}><GoogleDrivePage /></ThemeProvider>);
  const user = userEvent.setup();

  expect(await screen.findByRole('combobox', { name: 'Target library' })).toHaveTextContent('Movies');
  await user.click(screen.getByRole('button', { name: 'Authorize Google Drive' }));
  expect(startMock).toHaveBeenCalledWith('library-1');
  expect(openMock).toHaveBeenCalledWith(
    'https://accounts.google.com/auth',
    'tjxy-google-oauth',
    'noopener,noreferrer',
  );

  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  expect(await screen.findByRole('button', { name: 'Open Shows' })).toBeVisible();
  expect(directoriesMock).toHaveBeenCalledWith('oauth-state', { scope: 'MyDrive' });

  await user.click(screen.getByRole('button', { name: 'Open Shows' }));
  await waitFor(() => {
    expect(directoriesMock).toHaveBeenCalledWith('oauth-state', {
      scope: 'MyDrive', parentId: 'folder-1',
    });
  });
  await user.clear(screen.getByRole('textbox', { name: 'Display name' }));
  await user.type(screen.getByRole('textbox', { name: 'Display name' }), 'Drive Shows');
  await user.click(screen.getByRole('button', { name: 'Bind this folder' }));

  expect(bindMock).toHaveBeenCalledWith('oauth-state', {
    scope: 'MyDrive', displayName: 'Drive Shows', rootObjectId: 'folder-1',
  });
  expect(await screen.findByText('Ready')).toBeVisible();
  expect(screen.getByText(/Initial sync job: job-1/)).toBeVisible();
});

it('keeps the wizard retryable when the callback is not ready', async () => {
  sharedDrivesMock.mockRejectedValue({ category: 'conflict' });
  render(<ThemeProvider theme={theme}><GoogleDrivePage /></ThemeProvider>);
  const user = userEvent.setup();
  await screen.findByRole('combobox', { name: 'Target library' });
  await user.click(screen.getByRole('button', { name: 'Authorize Google Drive' }));
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));

  expect(notify).toHaveBeenCalledWith('Google authorization has not completed yet.', { type: 'warning' });
  expect(screen.getByRole('button', { name: 'Check authorization' })).toBeEnabled();
});

it('appends directory pages in first-seen order and removes duplicate identifiers', async () => {
  const cursor = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({
      items: [{ id: 'folder-1', name: 'Shows' }],
      nextPageToken: cursor,
    })
    .mockResolvedValueOnce({
      items: [
        { id: 'folder-1', name: 'Shows duplicate' },
        { id: 'folder-2', name: 'Archive' },
      ],
      nextPageToken: null,
    });
  render(<ThemeProvider theme={theme}><GoogleDrivePage /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByRole('combobox', { name: 'Target library' });
  await user.click(screen.getByRole('button', { name: 'Authorize Google Drive' }));
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Load more folders' }));

  expect(directoriesMock).toHaveBeenLastCalledWith('oauth-state', {
    scope: 'MyDrive', pageToken: cursor,
  });
  expect(screen.getByRole('button', { name: 'Open Shows' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Open Archive' })).toBeVisible();
  expect(screen.queryByRole('button', { name: 'Open Shows duplicate' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Load more folders' })).not.toBeInTheDocument();
});

it('preserves folders and the cursor when loading another page fails', async () => {
  const cursor = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({
      items: [{ id: 'folder-1', name: 'Shows' }],
      nextPageToken: cursor,
    })
    .mockRejectedValueOnce(new Error('Temporary failure'))
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  render(<ThemeProvider theme={theme}><GoogleDrivePage /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByRole('combobox', { name: 'Target library' });
  await user.click(screen.getByRole('button', { name: 'Authorize Google Drive' }));
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Load more folders' }));

  expect(await screen.findByRole('button', { name: 'Open Shows' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Load more folders' })).toBeEnabled();
  await user.click(screen.getByRole('button', { name: 'Load more folders' }));
  await waitFor(() => {
    expect(directoriesMock).toHaveBeenCalledTimes(3);
  });
  expect(directoriesMock).toHaveBeenNthCalledWith(2, 'oauth-state', {
    scope: 'MyDrive', pageToken: cursor,
  });
  expect(directoriesMock).toHaveBeenNthCalledWith(3, 'oauth-state', {
    scope: 'MyDrive', pageToken: cursor,
  });
});

it('uses the opened folder cursor instead of the previous root cursor', async () => {
  const rootCursor = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
  const folderCursor = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({
      items: [{ id: 'folder-1', name: 'Shows' }],
      nextPageToken: rootCursor,
    })
    .mockResolvedValueOnce({
      items: [{ id: 'child-1', name: 'Season 1' }],
      nextPageToken: folderCursor,
    })
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  render(<ThemeProvider theme={theme}><GoogleDrivePage /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByRole('combobox', { name: 'Target library' });
  await user.click(screen.getByRole('button', { name: 'Authorize Google Drive' }));
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Open Shows' }));
  await user.click(await screen.findByRole('button', { name: 'Load more folders' }));

  expect(directoriesMock).toHaveBeenLastCalledWith('oauth-state', {
    scope: 'MyDrive', parentId: 'folder-1', pageToken: folderCursor,
  });
});
