import { apiRequest } from '../api/httpClient';
import {
  bindGoogleDrive,
  bindOneDrive,
  listGoogleDirectories,
  listLibraries,
  listOneDriveDirectories,
  listSharedDrives,
  startGoogleDriveOAuth,
  startOneDriveOAuth,
} from './googleDriveApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);

beforeEach(() => {
  requestMock.mockReset();
});

it('loads the complete persisted library policy contract', async () => {
  requestMock.mockResolvedValue([
    {
      ItemId: 'library-1',
      Name: 'Movies',
      CollectionType: 'movies',
      Locations: ['tjxy://storage-root/root-1'],
      LibraryOptions: {
        Enabled: true,
        ScanProfile: 'Lazy',
        ProfileVersion: 3,
        ObjectSelectionScope: 'title_layer',
        MetadataPolicy: 'basic',
        MetadataSourceMode: 'automatic_scrape',
        ExpansionPolicy: 'on_browse',
        ProbePolicy: 'on_playback',
      },
    },
  ]);

  await expect(listLibraries()).resolves.toEqual([
    {
      id: 'library-1',
      name: 'Movies',
      collectionType: 'movies',
      locations: ['tjxy://storage-root/root-1'],
      enabled: true,
      scanProfile: 'Lazy',
      profileVersion: 3,
      objectSelectionScope: 'title_layer',
      metadataPolicy: 'basic',
      metadataSourceMode: 'automatic_scrape',
      expansionPolicy: 'on_browse',
      probePolicy: 'on_playback',
    },
  ]);
  expect(requestMock).toHaveBeenCalledWith('/Library/VirtualFolders', {});
});

it('starts OAuth with only the target library identifier', async () => {
  requestMock.mockResolvedValue({
    State: 'oauth-state',
    AuthorizationUrl: 'https://accounts.google.com/o/oauth2/v2/auth?state=oauth-state',
  });

  await expect(startGoogleDriveOAuth('library-1')).resolves.toEqual({
    state: 'oauth-state',
    authorizationUrl: 'https://accounts.google.com/o/oauth2/v2/auth?state=oauth-state',
  });
  expect(requestMock).toHaveBeenCalledWith('/Admin/Storage/OAuth/GoogleDrive/Start', {
    method: 'POST',
    body: JSON.stringify({ TargetLibraryId: 'library-1' }),
  });
  expect(JSON.stringify(requestMock.mock.calls)).not.toMatch(/secret|refresh/i);
});

it('starts OneDrive OAuth with only the target library identifier', async () => {
  requestMock.mockResolvedValue({
    State: 'oauth-state',
    AuthorizationUrl: 'https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize',
  });

  await expect(startOneDriveOAuth('library-1')).resolves.toEqual({
    state: 'oauth-state',
    authorizationUrl: 'https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize',
  });
  expect(requestMock).toHaveBeenCalledWith('/Admin/Storage/OAuth/OneDrive/Start', {
    method: 'POST',
    body: JSON.stringify({ TargetLibraryId: 'library-1' }),
  });
});

it('preserves Shared Drive pagination without exposing credentials', async () => {
  requestMock.mockResolvedValue({
    Items: [{ Id: 'drive-1', Name: 'Team Media' }],
    NextPageToken: 'page/two',
  });

  await expect(listSharedDrives('oauth-state', 'page/one')).resolves.toEqual({
    items: [{ id: 'drive-1', name: 'Team Media' }],
    nextPageToken: 'page/two',
  });
  expect(requestMock).toHaveBeenCalledWith(
    '/Admin/Storage/OAuth/GoogleDrive/oauth-state/SharedDrives?PageToken=page%2Fone',
  );
});

it('forwards cancellation signals to provider requests', async () => {
  requestMock.mockResolvedValue({ Items: [], NextPageToken: null });
  const controller = new AbortController();

  await listSharedDrives('oauth-state', undefined, controller.signal);

  expect(requestMock).toHaveBeenCalledWith(
    '/Admin/Storage/OAuth/GoogleDrive/oauth-state/SharedDrives',
    { signal: controller.signal },
  );
});

it('builds scoped directory queries and filters server records', async () => {
  const next = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
  requestMock.mockResolvedValue({
    Items: [{ Id: 'folder-1', Name: 'Shows' }],
    NextPageToken: next,
  });

  await expect(listGoogleDirectories('oauth-state', {
    scope: 'SharedDrive',
    sharedDriveId: 'drive-1',
    parentId: 'parent/1',
    pageToken: next,
  })).resolves.toEqual({
    items: [{ id: 'folder-1', name: 'Shows' }],
    nextPageToken: next,
  });
  expect(requestMock).toHaveBeenCalledWith(
    `/Admin/Storage/OAuth/GoogleDrive/oauth-state/Directories?Scope=SharedDrive&SharedDriveId=drive-1&ParentId=parent%2F1&PageToken=${next}`,
  );
});

it('binds the selected root without accepting browser-supplied identity or credentials', async () => {
  requestMock.mockResolvedValue({
    AccountId: 'account-1', RootId: 'root-1', InitialSyncJobId: 'job-1', RestartRequired: false,
  });

  await bindGoogleDrive('oauth-state', {
    scope: 'MyDrive',
    displayName: 'Movies on Drive',
    rootObjectId: 'folder-1',
  });

  expect(requestMock).toHaveBeenCalledWith(
    '/Admin/Storage/OAuth/GoogleDrive/oauth-state/Bind',
    {
      method: 'POST',
      body: JSON.stringify({
        Scope: 'MyDrive', DisplayName: 'Movies on Drive', RootObjectId: 'folder-1',
      }),
    },
  );
  expect(JSON.stringify(requestMock.mock.calls)).not.toMatch(/AccountIdentity|ClientSecret|RefreshToken/);
});

it('browses and binds OneDrive without browser-supplied credential fields', async () => {
  const next = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
  requestMock
    .mockResolvedValueOnce({
      Items: [{ Id: 'folder-1', Name: 'Shows' }],
      NextPageToken: next,
    })
    .mockResolvedValueOnce({
      AccountId: 'account-1', RootId: 'root-1', InitialSyncJobId: 'job-1', RestartRequired: false,
    });

  await expect(listOneDriveDirectories('oauth-state', {
    parentId: 'parent/1',
    pageToken: next,
  })).resolves.toEqual({
    items: [{ id: 'folder-1', name: 'Shows' }],
    nextPageToken: next,
  });
  await bindOneDrive('oauth-state', { displayName: 'OneDrive Shows', rootObjectId: 'folder-1' });

  expect(requestMock).toHaveBeenNthCalledWith(
    1,
    `/Admin/Storage/OAuth/OneDrive/oauth-state/Directories?ParentId=parent%2F1&PageToken=${next}`,
  );
  expect(requestMock).toHaveBeenNthCalledWith(
    2,
    '/Admin/Storage/OAuth/OneDrive/oauth-state/Bind',
    {
      method: 'POST',
      body: JSON.stringify({ DisplayName: 'OneDrive Shows', RootObjectId: 'folder-1' }),
    },
  );
  expect(JSON.stringify(requestMock.mock.calls)).not.toMatch(/AccountIdentity|ClientSecret|RefreshToken/);
});

it('rejects invalid scope combinations before making a request', async () => {
  await expect(listGoogleDirectories('oauth-state', {
    scope: 'MyDrive', sharedDriveId: 'must-not-be-present',
  })).rejects.toMatchObject({ category: 'validation' });
  expect(requestMock).not.toHaveBeenCalled();
});

it('rejects malformed successful responses', async () => {
  requestMock.mockResolvedValue({ Items: [{ Id: '', Name: 'Broken' }] });

  await expect(listSharedDrives('oauth-state')).rejects.toMatchObject({
    category: 'invalid-response',
  });
});

it('rejects missing or malformed directory pagination cursors', async () => {
  for (const nextPageToken of [undefined, 'provider-token', 'bad\ncursor', 42, {}]) {
    requestMock.mockResolvedValueOnce({ Items: [], NextPageToken: nextPageToken });
    await expect(listOneDriveDirectories('oauth-state')).rejects.toMatchObject({
      category: 'invalid-response',
    });
  }
});
