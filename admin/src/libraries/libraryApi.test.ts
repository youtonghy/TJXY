import { apiRequest } from '../api/httpClient';
import {
  createLibrary,
  deleteLibrary,
  listLibraries,
  renameLibrary,
  updateLibraryPolicy,
} from './libraryApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);

beforeEach(() => {
  requestMock.mockReset();
});

it('creates a library from an opaque filesystem selection and metadata source mode', async () => {
  requestMock.mockResolvedValue(undefined);

  await createLibrary({
    name: 'Family Movies',
    collectionType: 'movies',
    enabled: true,
    scanProfile: 'Lazy',
    metadataSourceMode: 'local_only',
    localMetadataAccessMode: 'direct',
    filesystemSelection: { rootId: 'root-1', relativePath: 'Movies' },
  });

  expect(requestMock).toHaveBeenCalledWith(
    '/Library/VirtualFolders?name=Family+Movies&collectionType=movies&refreshLibrary=false',
    {
      method: 'POST',
      body: JSON.stringify({
        LibraryOptions: {
          Enabled: true,
          ScanProfile: 'Lazy',
          MetadataSourceMode: 'local_only',
          LocalMetadataAccessMode: 'direct',
        },
        FilesystemSelection: { RootId: 'root-1', RelativePath: 'Movies' },
      }),
    },
  );
  expect(JSON.stringify(requestMock.mock.calls)).not.toContain('/Movies');
});

it('defaults a missing metadata source mode from an older server to automatic scrape', async () => {
  requestMock.mockResolvedValueOnce([{
    ItemId: 'library-1',
    Name: 'Movies',
    CollectionType: 'movies',
    Locations: [],
    LibraryOptions: {
      Enabled: true,
      ScanProfile: 'Lazy',
      ProfileVersion: 1,
      ObjectSelectionScope: 'title_layer',
      MetadataPolicy: 'basic',
      ExpansionPolicy: 'on_browse',
      ProbePolicy: 'on_playback',
    },
  }]);

  await expect(listLibraries()).resolves.toEqual([
    expect.objectContaining({ metadataSourceMode: 'automatic_scrape' }),
  ]);
});

it('parses unavailable storage locations while remaining compatible with older servers', async () => {
  requestMock.mockResolvedValueOnce([{
    ItemId: 'library-1',
    Name: 'Movies',
    CollectionType: 'movies',
    Locations: ['tjxy://storage-root/root-1'],
    UnavailableLocations: ['tjxy://storage-root/root-1'],
    LibraryOptions: {
      Enabled: true,
      ScanProfile: 'Lazy',
      ProfileVersion: 1,
      ObjectSelectionScope: 'title_layer',
      MetadataPolicy: 'basic',
      MetadataSourceMode: 'automatic_scrape',
      ExpansionPolicy: 'on_browse',
      ProbePolicy: 'on_playback',
    },
  }]);

  await expect(listLibraries()).resolves.toEqual([
    expect.objectContaining({
      unavailableLocations: ['tjxy://storage-root/root-1'],
    }),
  ]);
});

it('renames a library through the exact current-name command', async () => {
  requestMock.mockResolvedValue(undefined);

  await renameLibrary('Family Movies', 'Archive Movies');

  expect(requestMock).toHaveBeenCalledWith(
    '/Library/VirtualFolders/Name?name=Family+Movies&newName=Archive+Movies&refreshLibrary=false',
    { method: 'POST' },
  );
});

it('updates the complete effective policy with the current profile version', async () => {
  requestMock.mockResolvedValue(undefined);

  await updateLibraryPolicy({
    id: 'library-1',
    enabled: false,
    scanProfile: 'Full',
    profileVersion: 3,
    metadataSourceMode: 'automatic_scrape',
    localMetadataAccessMode: 'import',
    effectivePolicy: {
      objectSelectionScope: 'title_layer',
      metadataPolicy: 'full',
      expansionPolicy: 'eager',
      probePolicy: 'on_playback',
    },
  });

  expect(requestMock).toHaveBeenCalledWith('/Library/VirtualFolders/LibraryOptions', {
    method: 'POST',
    body: JSON.stringify({
      Id: 'library-1',
      LibraryOptions: {
        Enabled: false,
        ScanProfile: 'Full',
        ProfileVersion: 3,
        MetadataSourceMode: 'automatic_scrape',
        LocalMetadataAccessMode: 'import',
        ObjectSelectionScope: 'title_layer',
        MetadataPolicy: 'full',
        ExpansionPolicy: 'eager',
        ProbePolicy: 'on_playback',
      },
    }),
  });
});

it('deletes a library by its exact name', async () => {
  requestMock.mockResolvedValue(undefined);

  await deleteLibrary('Archive Movies');

  expect(requestMock).toHaveBeenCalledWith(
    '/Library/VirtualFolders?name=Archive+Movies&refreshLibrary=false',
    { method: 'DELETE' },
  );
});
