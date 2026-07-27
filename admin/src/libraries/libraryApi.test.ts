import { apiRequest } from '../api/httpClient';
import {
  createLibrary,
  deleteLibrary,
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

it('creates an empty library without accepting a browser filesystem path', async () => {
  requestMock.mockResolvedValue(undefined);

  await createLibrary({
    name: 'Family Movies',
    collectionType: 'movies',
    enabled: true,
    scanProfile: 'Lazy',
  });

  expect(requestMock).toHaveBeenCalledWith(
    '/Library/VirtualFolders?name=Family+Movies&collectionType=movies&refreshLibrary=false',
    {
      method: 'POST',
      body: JSON.stringify({ LibraryOptions: { Enabled: true, ScanProfile: 'Lazy' } }),
    },
  );
  expect(JSON.stringify(requestMock.mock.calls)).not.toMatch(/paths|filesystem/i);
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
    scanProfile: 'Hybrid',
    profileVersion: 3,
    effectivePolicy: {
      objectSelectionScope: 'title_layer',
      metadataPolicy: 'full',
      expansionPolicy: 'background',
      probePolicy: 'on_playback',
    },
  });

  expect(requestMock).toHaveBeenCalledWith('/Library/VirtualFolders/LibraryOptions', {
    method: 'POST',
    body: JSON.stringify({
      Id: 'library-1',
      LibraryOptions: {
        Enabled: false,
        ScanProfile: 'Hybrid',
        ProfileVersion: 3,
        ObjectSelectionScope: 'title_layer',
        MetadataPolicy: 'full',
        ExpansionPolicy: 'background',
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
