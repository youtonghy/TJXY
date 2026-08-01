import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { MockInstance } from 'vitest';

import { defaultTestAuthProvider, renderWithAdmin } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
import { GoogleDrivePage } from './GoogleDrivePage';
import type { LibraryOption } from './googleDriveApi';
import {
  bindGoogleDrive,
  listGoogleDirectories,
  listLibraries,
  listSharedDrives,
  startGoogleDriveOAuth,
} from './googleDriveApi';

vi.mock('./googleDriveApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('./googleDriveApi')>();
  return {
    ...original,
    bindGoogleDrive: vi.fn(),
    listGoogleDirectories: vi.fn(),
    listLibraries: vi.fn(),
    listSharedDrives: vi.fn(),
    startGoogleDriveOAuth: vi.fn(),
  };
});

const librariesMock = vi.mocked(listLibraries);
const startMock = vi.mocked(startGoogleDriveOAuth);
const sharedDrivesMock = vi.mocked(listSharedDrives);
const directoriesMock = vi.mocked(listGoogleDirectories);
const bindMock = vi.mocked(bindGoogleDrive);
const cursor = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const secondCursor = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';
const movies = {
  id: 'library-1',
  name: 'Movies',
  collectionType: 'movies',
  locations: [],
  enabled: true,
  scanProfile: 'Lazy',
  profileVersion: 1,
  objectSelectionScope: 'title_layer',
  metadataPolicy: 'basic',
  metadataSourceMode: 'automatic_scrape',
  expansionPolicy: 'on_browse',
  probePolicy: 'on_playback',
} satisfies LibraryOption;
let openMock: MockInstance<typeof window.open>;
let popupReplaceMock = vi.fn();
let popupCloseMock = vi.fn();

function fakePopup(): Window {
  return {
    opener: window,
    location: { replace: popupReplaceMock },
    close: popupCloseMock,
  } as unknown as Window;
}

function renderGoogle(authProvider = defaultTestAuthProvider) {
  return renderWithAdmin(
    <>
      <GoogleDrivePage />
      <AdminNotifications />
    </>,
    { authProvider, initialEntries: ['/admin/storage/google-drive'], strict: true },
  );
}

async function targetLibraryTrigger() {
  return await screen.findByRole('button', { name: /Target library/iu });
}

async function beginAuthorization(user: ReturnType<typeof userEvent.setup>) {
  await targetLibraryTrigger();
  await user.click(screen.getByRole('button', { name: 'Authorize Google Drive' }));
  await waitFor(() => {
    expect(startMock).toHaveBeenCalledWith('library-1', expect.any(AbortSignal));
  });
}

beforeEach(() => {
  librariesMock.mockReset();
  startMock.mockReset();
  sharedDrivesMock.mockReset();
  directoriesMock.mockReset();
  bindMock.mockReset();
  librariesMock.mockResolvedValue([movies, { ...movies, id: 'library-2', name: 'Disabled', enabled: false }]);
  startMock.mockResolvedValue({ state: 'oauth-state', authorizationUrl: 'https://accounts.google.com/auth' });
  sharedDrivesMock.mockResolvedValue({ items: [], nextPageToken: null });
  directoriesMock
    .mockResolvedValueOnce({ items: [{ id: 'folder-1', name: 'Shows' }], nextPageToken: null })
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  bindMock.mockResolvedValue({
    accountId: 'account-1', rootId: 'root-1', initialSyncJobId: 'job-1', restartRequired: false,
  });
  popupReplaceMock = vi.fn();
  popupCloseMock = vi.fn();
  openMock = vi.spyOn(window, 'open').mockReturnValue(fakePopup());
});

afterEach(() => {
  vi.restoreAllMocks();
  sessionStorage.clear();
});

it('authorizes, browses, reviews, and binds a My Drive folder', async () => {
  renderGoogle();
  const user = userEvent.setup();
  expect(await targetLibraryTrigger()).toHaveTextContent('Movies');

  await beginAuthorization(user);
  expect(openMock).toHaveBeenCalledWith(
    'about:blank',
    'tjxy-google-oauth',
    'popup',
  );
  expect(popupReplaceMock).toHaveBeenCalledWith('https://accounts.google.com/auth');
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  expect(await screen.findByRole('button', { name: 'Open Shows' })).toBeVisible();
  expect(directoriesMock).toHaveBeenCalledWith(
    'oauth-state', { scope: 'MyDrive' }, expect.any(AbortSignal),
  );

  await user.click(screen.getByRole('button', { name: 'Open Shows' }));
  await waitFor(() => {
    expect(directoriesMock).toHaveBeenCalledWith(
      'oauth-state', { scope: 'MyDrive', parentId: 'folder-1' }, expect.any(AbortSignal),
    );
  });
  await user.click(screen.getByRole('button', { name: 'Use this folder' }));
  const displayName = screen.getByRole('textbox', { name: 'Display name' });
  await user.clear(displayName);
  await user.type(displayName, 'Drive Shows');
  await user.click(screen.getByRole('button', { name: 'Add Google Drive' }));

  expect(bindMock).toHaveBeenCalledWith('oauth-state', {
    scope: 'MyDrive', displayName: 'Drive Shows', rootObjectId: 'folder-1',
  }, expect.any(AbortSignal));
  expect(await screen.findByText('Google Drive is connected')).toBeVisible();
  expect(screen.getByText('job-1')).toBeVisible();
  expect(screen.getByText(/active and ready/iu)).toBeVisible();
});

it('keeps a blocked popup recoverable and restart clears only workflow state', async () => {
  openMock.mockReset();
  openMock.mockReturnValueOnce(null).mockReturnValue(fakePopup());
  sessionStorage.setItem('unrelated-session-value', 'keep-me');
  renderGoogle();
  const user = userEvent.setup();

  await beginAuthorization(user);
  const blockedAlert = await screen.findByRole('alert');
  expect(blockedAlert).toHaveTextContent('authorization window was blocked');
  expect(screen.getByRole('button', { name: 'Restart authorization' })).toBeVisible();
  await user.click(within(blockedAlert).getByRole('button', { name: 'Retry' }));
  await waitFor(() => { expect(screen.queryByText('authorization window was blocked')).not.toBeInTheDocument(); });
  expect(startMock).toHaveBeenCalledOnce();

  await user.click(screen.getByRole('button', { name: 'Restart authorization' }));
  expect(screen.getByRole('button', { name: 'Authorize Google Drive' })).toBeEnabled();
  expect(screen.queryByRole('button', { name: 'Check authorization' })).not.toBeInTheDocument();
  expect(sessionStorage.getItem('unrelated-session-value')).toBe('keep-me');
});

it('keeps authorization retryable before the provider callback is ready', async () => {
  sharedDrivesMock.mockRejectedValue({ category: 'conflict', message: 'private-callback-detail' });
  renderGoogle();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));

  expect(await screen.findByText('Google authorization has not completed yet.')).toBeVisible();
  expect(screen.queryByText('private-callback-detail')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Check authorization' })).toBeEnabled();
});

it('aborts the sibling verification request when either provider request fails', async () => {
  let finishDirectories: ((page: { items: []; nextPageToken: null }) => void) | undefined;
  sharedDrivesMock.mockRejectedValue(new Error('private-provider-detail'));
  directoriesMock.mockReset();
  directoriesMock.mockReturnValue(new Promise((resolve) => { finishDirectories = resolve; }));
  renderGoogle();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));

  expect(await screen.findByText('Google authorization could not be verified.')).toBeVisible();
  const directorySignal = directoriesMock.mock.calls[0]?.[2];
  expect(directorySignal?.aborted).toBe(true);
  expect(screen.queryByText('private-provider-detail')).not.toBeInTheDocument();
  finishDirectories?.({ items: [], nextPageToken: null });
  await Promise.resolve();
});

it('appends directory pages in first-seen order and removes duplicate identifiers', async () => {
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({ items: [{ id: 'folder-1', name: 'Shows' }], nextPageToken: cursor })
    .mockResolvedValueOnce({
      items: [{ id: 'folder-1', name: 'Shows duplicate' }, { id: 'folder-2', name: 'Archive' }],
      nextPageToken: null,
    });
  renderGoogle();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Load more folders' }));

  expect(directoriesMock).toHaveBeenLastCalledWith(
    'oauth-state', { scope: 'MyDrive', pageToken: cursor }, expect.any(AbortSignal),
  );
  expect(screen.getByRole('button', { name: 'Open Shows' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Open Archive' })).toBeVisible();
  expect(screen.queryByRole('button', { name: 'Open Shows duplicate' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Load more folders' })).not.toBeInTheDocument();
});

it('preserves the directory cursor and rows when load more fails, then retries safely', async () => {
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({ items: [{ id: 'folder-1', name: 'Shows' }], nextPageToken: cursor })
    .mockRejectedValueOnce(new Error('private-provider-detail'))
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  renderGoogle();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Load more folders' }));

  const browser = screen.getByRole('region', { name: 'Google Drive folders' });
  expect(await within(browser).findByText('The folder list could not be loaded')).toBeVisible();
  expect(within(browser).getByRole('button', { name: 'Open Shows' })).toBeVisible();
  expect(screen.queryByText('private-provider-detail')).not.toBeInTheDocument();
  await user.click(within(browser).getByRole('button', { name: 'Retry' }));
  await waitFor(() => { expect(directoriesMock).toHaveBeenCalledTimes(3); });
  expect(directoriesMock).toHaveBeenNthCalledWith(
    2, 'oauth-state', { scope: 'MyDrive', pageToken: cursor }, expect.any(AbortSignal),
  );
  expect(directoriesMock).toHaveBeenNthCalledWith(
    3, 'oauth-state', { scope: 'MyDrive', pageToken: cursor }, expect.any(AbortSignal),
  );
});

it('resets folder pagination when navigating through a breadcrumb', async () => {
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({ items: [{ id: 'folder-1', name: 'Shows' }], nextPageToken: cursor })
    .mockResolvedValueOnce({ items: [{ id: 'season-1', name: 'Season 1' }], nextPageToken: secondCursor })
    .mockResolvedValueOnce({ items: [], nextPageToken: null })
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  renderGoogle();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Open Shows' }));
  await user.click(await screen.findByRole('button', { name: 'Load more folders' }));
  expect(directoriesMock).toHaveBeenLastCalledWith('oauth-state', {
    scope: 'MyDrive', parentId: 'folder-1', pageToken: secondCursor,
  }, expect.any(AbortSignal));

  await user.click(screen.getByRole('link', { name: 'My Drive' }));
  await waitFor(() => {
    expect(directoriesMock).toHaveBeenLastCalledWith(
      'oauth-state', { scope: 'MyDrive' }, expect.any(AbortSignal),
    );
  });
});

it('switches scope and paginates Shared Drives independently from folders', async () => {
  sharedDrivesMock
    .mockResolvedValueOnce({ items: [{ id: 'drive-1', name: 'First Drive' }], nextPageToken: cursor })
    .mockResolvedValueOnce({
      items: [{ id: 'drive-1', name: 'Duplicate Drive' }, { id: 'drive-2', name: 'Second Drive' }],
      nextPageToken: null,
    });
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({ items: [], nextPageToken: null })
    .mockResolvedValueOnce({ items: [{ id: 'folder-1', name: 'Team Shows' }], nextPageToken: null })
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  renderGoogle();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(screen.getByRole('radio', { name: 'Shared Drive' }));
  expect(await screen.findByRole('button', { name: 'Open Team Shows' })).toBeVisible();

  await user.click(screen.getByRole('button', { name: 'Load more Shared Drives' }));
  expect(sharedDrivesMock).toHaveBeenLastCalledWith(
    'oauth-state', cursor, expect.any(AbortSignal),
  );
  await user.click(screen.getByRole('button', { name: /Shared Drive/iu, expanded: false }));
  await user.click(await screen.findByRole('option', { name: 'Second Drive' }));
  await waitFor(() => {
    expect(directoriesMock).toHaveBeenLastCalledWith('oauth-state', {
      scope: 'SharedDrive', sharedDriveId: 'drive-2', parentId: 'drive-2',
    }, expect.any(AbortSignal));
  });
});

it('continues a temporarily empty Shared Drive page before switching scope', async () => {
  sharedDrivesMock
    .mockResolvedValueOnce({ items: [], nextPageToken: cursor })
    .mockResolvedValueOnce({ items: [{ id: 'drive-1', name: 'Team Drive' }], nextPageToken: null });
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({ items: [], nextPageToken: null })
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  renderGoogle();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(screen.getByRole('radio', { name: 'Shared Drive' }));

  expect(await screen.findByText('No Shared Drives are available')).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Load more Shared Drives' }));
  expect(sharedDrivesMock).toHaveBeenLastCalledWith(
    'oauth-state', cursor, expect.any(AbortSignal),
  );
  await user.click(screen.getByRole('radio', { name: 'Shared Drive' }));
  await waitFor(() => {
    expect(directoriesMock).toHaveBeenLastCalledWith('oauth-state', {
      scope: 'SharedDrive', sharedDriveId: 'drive-1', parentId: 'drive-1',
    }, expect.any(AbortSignal));
  });
});

it('preserves the selected Shared Drive across My Drive and clears stale pagination errors', async () => {
  sharedDrivesMock
    .mockResolvedValueOnce({
      items: [{ id: 'drive-1', name: 'First Drive' }, { id: 'drive-2', name: 'Second Drive' }],
      nextPageToken: cursor,
    })
    .mockRejectedValueOnce(new Error('private-shared-drive-detail'));
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({ items: [], nextPageToken: null })
    .mockResolvedValueOnce({ items: [], nextPageToken: null })
    .mockResolvedValueOnce({ items: [], nextPageToken: null })
    .mockResolvedValueOnce({ items: [], nextPageToken: null })
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  renderGoogle();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('radio', { name: 'Shared Drive' }));
  await user.click(screen.getByRole('button', { name: /Shared Drive/iu, expanded: false }));
  await user.click(await screen.findByRole('option', { name: 'Second Drive' }));
  await user.click(screen.getByRole('button', { name: 'Load more Shared Drives' }));

  expect(await screen.findByText('Shared Drives could not be loaded')).toBeVisible();
  expect(screen.queryByText('private-shared-drive-detail')).not.toBeInTheDocument();
  await user.click(screen.getByRole('radio', { name: 'My Drive' }));
  await waitFor(() => {
    expect(screen.queryByText('Shared Drives could not be loaded')).not.toBeInTheDocument();
  });
  await user.click(screen.getByRole('radio', { name: 'Shared Drive' }));

  await waitFor(() => {
    expect(directoriesMock).toHaveBeenLastCalledWith('oauth-state', {
      scope: 'SharedDrive', sharedDriveId: 'drive-2', parentId: 'drive-2',
    }, expect.any(AbortSignal));
  });
});

it('requires a fresh authorization after an unconfirmed binding result', async () => {
  bindMock.mockRejectedValue(new Error('private-bind-detail'));
  renderGoogle();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Use this folder' }));
  await user.click(screen.getByRole('button', { name: 'Add Google Drive' }));

  expect(await screen.findByText('The binding result could not be confirmed')).toBeVisible();
  expect(screen.getByText(/authorization cannot be reused/iu)).toBeVisible();
  expect(screen.queryByText('private-bind-detail')).not.toBeInTheDocument();
  const addButton = screen.getByRole('button', { name: 'Add Google Drive' });
  expect(addButton).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Back to folder' })).toBeDisabled();
  expect(screen.getByRole('textbox', { name: 'Display name' })).toBeDisabled();
  await user.click(addButton);
  expect(bindMock).toHaveBeenCalledOnce();
  await user.click(screen.getByRole('button', { name: 'Restart authorization' }));
  expect(screen.getByRole('button', { name: 'Authorize Google Drive' })).toBeEnabled();
});

it('aborts an in-flight provider request when the page unmounts', async () => {
  let finishAuthorization: ((page: { items: []; nextPageToken: null }) => void) | undefined;
  directoriesMock.mockReset();
  directoriesMock.mockReturnValue(new Promise((resolve) => { finishAuthorization = resolve; }));
  const view = renderGoogle();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await waitFor(() => { expect(directoriesMock).toHaveBeenCalledOnce(); });
  const signal = directoriesMock.mock.calls[0]?.[2];

  view.unmount();
  expect(signal?.aborted).toBe(true);
  finishAuthorization?.({ items: [], nextPageToken: null });
  await Promise.resolve();
});

it('renders disabled library choices and restart-required binding details', async () => {
  bindMock.mockResolvedValue({
    accountId: 'account-1', rootId: 'root-1', initialSyncJobId: 'job-1', restartRequired: true,
  });
  renderGoogle();
  const user = userEvent.setup();
  await user.click(await targetLibraryTrigger());
  expect(await screen.findByRole('option', { name: /Disabled/iu })).toHaveAttribute('aria-disabled', 'true');
  await user.keyboard('{Escape}');
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Use this folder' }));
  await user.click(screen.getByRole('button', { name: 'Add Google Drive' }));

  expect(await screen.findByText(/Restart the server/iu)).toBeVisible();
  expect(screen.getByText('job-1')).toBeVisible();
});

it('routes an authorization failure before showing provider-local feedback', async () => {
  const checkError = vi.fn().mockRejectedValue({
    logoutUser: false,
    message: false,
    redirectTo: '/admin/access-denied',
  });
  librariesMock.mockRejectedValue({ status: 403, message: 'private-storage-auth-detail' });
  renderGoogle({ ...defaultTestAuthProvider, checkError });

  await waitFor(() => { expect(checkError).toHaveBeenCalled(); });
  await waitFor(() => { expect(screen.queryByRole('heading', { name: 'Google Drive' })).not.toBeInTheDocument(); });
  expect(screen.queryByText('Target libraries could not be loaded')).not.toBeInTheDocument();
  expect(screen.queryByText('private-storage-auth-detail')).not.toBeInTheDocument();
});
