import { apiRequest } from '../api/httpClient';
import { attachFilesystemFolder, listFilesystemDirectories, listFilesystemRoots } from './filesystemApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);

beforeEach(() => {
  requestMock.mockReset();
});

it('parses opaque roots and directory list entries', async () => {
  requestMock
    .mockResolvedValueOnce([{ Id: 'root-1', Name: 'Media' }])
    .mockResolvedValueOnce({ Items: [
      { Name: 'Movies', RelativePath: 'Movies', ModifiedAt: '2026-08-01T10:00:00Z' },
    ] });

  await expect(listFilesystemRoots()).resolves.toEqual([{ id: 'root-1', name: 'Media' }]);
  await expect(listFilesystemDirectories('root-1', '')).resolves.toEqual([
    { name: 'Movies', relativePath: 'Movies', modifiedAt: '2026-08-01T10:00:00Z' },
  ]);
  expect(requestMock.mock.calls[1]?.[0]).toContain('RootId=root-1&Path=');
});

it('attaches only an opaque root id and relative path', async () => {
  requestMock.mockResolvedValue(undefined);

  await attachFilesystemFolder('library-1', { rootId: 'root-1', relativePath: 'Movies' });

  expect(requestMock).toHaveBeenCalledWith('/Library/VirtualFolders/Paths', expect.objectContaining({
    method: 'POST',
  }));
  const body: unknown = JSON.parse(requestMock.mock.calls[0]?.[1]?.body as string);
  expect(body).toEqual({
    LibraryId: 'library-1',
    FilesystemSelection: { RootId: 'root-1', RelativePath: 'Movies' },
  });
});
