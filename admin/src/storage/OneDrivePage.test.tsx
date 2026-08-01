import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { MockInstance } from 'vitest';

import { defaultTestAuthProvider, renderWithAdmin } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
import { OneDrivePage } from './OneDrivePage';
import type { GoogleDriveChoice, LibraryOption } from './googleDriveApi';
import {
  bindOneDrive,
  listLibraries,
  listOneDriveDirectories,
  startOneDriveOAuth,
} from './googleDriveApi';

vi.mock('./googleDriveApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('./googleDriveApi')>();
  return {
    ...original,
    bindOneDrive: vi.fn(),
    listLibraries: vi.fn(),
    listOneDriveDirectories: vi.fn(),
    startOneDriveOAuth: vi.fn(),
  };
});

const librariesMock = vi.mocked(listLibraries);
const startMock = vi.mocked(startOneDriveOAuth);
const directoriesMock = vi.mocked(listOneDriveDirectories);
const bindMock = vi.mocked(bindOneDrive);
const cursor = '028f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';
const secondCursor = '038f17ac-4e99-7ec5-b4fd-8f15ca9f4f13';
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

function renderOneDrive(authProvider = defaultTestAuthProvider) {
  return renderWithAdmin(
    <>
      <OneDrivePage />
      <AdminNotifications />
    </>,
    { authProvider, initialEntries: ['/admin/storage/onedrive'], strict: true },
  );
}

async function targetLibraryTrigger() {
  return await screen.findByRole('button', { name: /Target library/iu });
}

async function beginAuthorization(user: ReturnType<typeof userEvent.setup>) {
  await targetLibraryTrigger();
  await user.click(screen.getByRole('button', { name: 'Authorize OneDrive' }));
  await waitFor(() => {
    expect(startMock).toHaveBeenCalledWith('library-1', expect.any(AbortSignal));
  });
}

beforeEach(() => {
  librariesMock.mockReset();
  startMock.mockReset();
  directoriesMock.mockReset();
  bindMock.mockReset();
  librariesMock.mockResolvedValue([movies]);
  startMock.mockResolvedValue({ state: 'oauth-state', authorizationUrl: 'https://login.microsoftonline.com/auth' });
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

afterEach(() => { vi.restoreAllMocks(); });

it('authorizes, browses, reviews, and binds a OneDrive folder', async () => {
  renderOneDrive();
  const user = userEvent.setup();
  await beginAuthorization(user);
  expect(openMock).toHaveBeenCalledWith(
    'about:blank',
    'tjxy-onedrive-oauth',
    'popup',
  );
  expect(popupReplaceMock).toHaveBeenCalledWith('https://login.microsoftonline.com/auth');

  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  expect(await screen.findByRole('button', { name: 'Open Shows' })).toBeVisible();
  expect(directoriesMock).toHaveBeenCalledWith('oauth-state', {}, expect.any(AbortSignal));
  await user.click(screen.getByRole('button', { name: 'Open Shows' }));
  await waitFor(() => {
    expect(directoriesMock).toHaveBeenCalledWith(
      'oauth-state', { parentId: 'folder-1' }, expect.any(AbortSignal),
    );
  });
  await user.click(screen.getByRole('button', { name: 'Use this folder' }));
  expect(screen.getByRole('textbox', { name: 'Display name' })).toHaveValue('Movies');
  await user.click(screen.getByRole('button', { name: 'Add OneDrive' }));

  expect(bindMock).toHaveBeenCalledWith('oauth-state', {
    displayName: 'Movies', rootObjectId: 'folder-1',
  }, expect.any(AbortSignal));
  expect(await screen.findByText('OneDrive is connected')).toBeVisible();
  expect(screen.getByText('job-1')).toBeVisible();
});

it('shows explicit folder loading and keeps restart visible after authorization begins', async () => {
  let finishAuthorization: ((page: { items: []; nextPageToken: null }) => void) | undefined;
  directoriesMock.mockReset();
  directoriesMock.mockReturnValue(new Promise((resolve) => { finishAuthorization = resolve; }));
  renderOneDrive();
  const user = userEvent.setup();
  await beginAuthorization(user);
  expect(screen.getByRole('button', { name: 'Restart authorization' })).toBeVisible();

  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  expect(screen.getByRole('button', { name: 'Check authorization' })).toHaveAttribute('data-pending', 'true');
  expect(screen.getByRole('button', { name: 'Restart authorization' })).toBeDisabled();
  finishAuthorization?.({ items: [], nextPageToken: null });
  expect(await screen.findByText('This folder has no child folders.')).toBeVisible();
});

it('recovers a blocked popup and keeps callback verification retryable', async () => {
  openMock.mockReset();
  openMock.mockReturnValueOnce(null).mockReturnValue(fakePopup());
  directoriesMock.mockReset();
  directoriesMock.mockRejectedValueOnce({ category: 'conflict', message: 'private-callback-detail' });
  renderOneDrive();
  const user = userEvent.setup();
  await beginAuthorization(user);
  const blocked = await screen.findByRole('alert');
  await user.click(within(blocked).getByRole('button', { name: 'Retry' }));
  expect(startMock).toHaveBeenCalledOnce();

  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  expect(await screen.findByText('Microsoft authorization has not completed yet.')).toBeVisible();
  expect(screen.queryByText('private-callback-detail')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Check authorization' })).toBeEnabled();
});

it('continues from an empty page that still has a continuation', async () => {
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({ items: [], nextPageToken: cursor })
    .mockResolvedValueOnce({ items: [{ id: 'folder-1', name: 'Shows' }], nextPageToken: null });
  renderOneDrive();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  expect(await screen.findByText('No folders on this page.')).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Load more folders' }));

  expect(directoriesMock).toHaveBeenLastCalledWith(
    'oauth-state', { pageToken: cursor }, expect.any(AbortSignal),
  );
  expect(await screen.findByRole('button', { name: 'Open Shows' })).toBeVisible();
  expect(screen.queryByRole('button', { name: 'Load more folders' })).not.toBeInTheDocument();
});

it('deduplicates folder identifiers while appending a page', async () => {
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({ items: [{ id: 'folder-1', name: 'Shows' }], nextPageToken: cursor })
    .mockResolvedValueOnce({
      items: [{ id: 'folder-1', name: 'Duplicate Shows' }, { id: 'folder-2', name: 'Archive' }],
      nextPageToken: null,
    });
  renderOneDrive();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Load more folders' }));

  expect(screen.getByRole('button', { name: 'Open Shows' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Open Archive' })).toBeVisible();
  expect(screen.queryByRole('button', { name: 'Open Duplicate Shows' })).not.toBeInTheDocument();
});

it('retains the same cursor and visible folders after a load-more failure', async () => {
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({ items: [{ id: 'folder-1', name: 'Shows' }], nextPageToken: cursor })
    .mockRejectedValueOnce(new Error('private-provider-detail'))
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  renderOneDrive();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Load more folders' }));

  const browser = screen.getByRole('region', { name: 'OneDrive folders' });
  expect(await within(browser).findByRole('button', { name: 'Open Shows' })).toBeVisible();
  expect(within(browser).getByText('The folder list could not be loaded')).toBeVisible();
  expect(screen.queryByText('private-provider-detail')).not.toBeInTheDocument();
  await user.click(within(browser).getByRole('button', { name: 'Retry' }));
  await waitFor(() => { expect(directoriesMock).toHaveBeenCalledTimes(3); });
  expect(directoriesMock).toHaveBeenNthCalledWith(
    2, 'oauth-state', { pageToken: cursor }, expect.any(AbortSignal),
  );
  expect(directoriesMock).toHaveBeenNthCalledWith(
    3, 'oauth-state', { pageToken: cursor }, expect.any(AbortSignal),
  );
});

it('uses the opened folder cursor and resets it on breadcrumb navigation', async () => {
  directoriesMock.mockReset();
  directoriesMock
    .mockResolvedValueOnce({ items: [{ id: 'folder-1', name: 'Shows' }], nextPageToken: cursor })
    .mockResolvedValueOnce({ items: [], nextPageToken: secondCursor })
    .mockResolvedValueOnce({ items: [], nextPageToken: null })
    .mockResolvedValueOnce({ items: [], nextPageToken: null });
  renderOneDrive();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Open Shows' }));
  await user.click(await screen.findByRole('button', { name: 'Load more folders' }));
  expect(directoriesMock).toHaveBeenLastCalledWith('oauth-state', {
    parentId: 'folder-1', pageToken: secondCursor,
  }, expect.any(AbortSignal));

  await user.click(screen.getByRole('link', { name: 'OneDrive' }));
  await waitFor(() => {
    expect(directoriesMock).toHaveBeenLastCalledWith(
      'oauth-state', {}, expect.any(AbortSignal),
    );
  });
});

it('requires a fresh authorization after an unconfirmed binding result', async () => {
  bindMock.mockRejectedValue(new Error('private-bind-detail'));
  renderOneDrive();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await user.click(await screen.findByRole('button', { name: 'Use this folder' }));
  await user.click(screen.getByRole('button', { name: 'Add OneDrive' }));

  expect(await screen.findByText('The binding result could not be confirmed')).toBeVisible();
  expect(screen.getByText(/authorization cannot be reused/iu)).toBeVisible();
  expect(screen.queryByText('private-bind-detail')).not.toBeInTheDocument();
  const addButton = screen.getByRole('button', { name: 'Add OneDrive' });
  expect(addButton).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Back to folder' })).toBeDisabled();
  expect(screen.getByRole('textbox', { name: 'Display name' })).toBeDisabled();
  await user.click(addButton);
  expect(bindMock).toHaveBeenCalledOnce();
});

it('invalidates late provider responses when the page unmounts', async () => {
  let finishAuthorization: ((page: { items: GoogleDriveChoice[]; nextPageToken: null }) => void) | undefined;
  directoriesMock.mockReset();
  directoriesMock.mockReturnValue(new Promise((resolve) => { finishAuthorization = resolve; }));
  const view = renderOneDrive();
  const user = userEvent.setup();
  await beginAuthorization(user);
  await user.click(screen.getByRole('button', { name: 'Check authorization' }));
  await waitFor(() => { expect(directoriesMock).toHaveBeenCalledOnce(); });
  const signal = directoriesMock.mock.calls[0]?.[2];
  view.unmount();

  expect(signal?.aborted).toBe(true);
  finishAuthorization?.({ items: [{ id: 'late', name: 'Late folder' }], nextPageToken: null });
  await Promise.resolve();
  expect(screen.queryByText('Late folder')).not.toBeInTheDocument();
});
