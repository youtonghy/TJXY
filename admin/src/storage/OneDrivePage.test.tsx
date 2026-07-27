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
    .mockResolvedValueOnce([{ id: 'folder-1', name: 'Shows' }])
    .mockResolvedValueOnce([]);
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
    expect(directoriesMock).toHaveBeenCalledWith('oauth-state', 'folder-1');
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
