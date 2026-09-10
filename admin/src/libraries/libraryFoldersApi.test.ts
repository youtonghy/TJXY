import { apiRequest } from '../api/httpClient';
import { listFolderContents, listLibraryFolders } from './libraryFoldersApi';

vi.mock('../api/httpClient', async (importOriginal) => ({
  ...await importOriginal<typeof import('../api/httpClient')>(), apiRequest: vi.fn(),
}));
const requestMock = vi.mocked(apiRequest);
beforeEach(() => { requestMock.mockReset(); });

it('preserves physical paths while keeping cloud paths explicitly unavailable', async () => {
  requestMock.mockResolvedValue([
    { Id: 'local', Name: 'Movies', Path: '/mnt/电影', Provider: 'filesystem' },
    { Id: 'remote', Name: 'Cloud', Path: null, Provider: 'onedrive' },
  ]);
  await expect(listLibraryFolders('library')).resolves.toEqual([
    { id: 'local', name: 'Movies', path: '/mnt/电影', provider: 'filesystem' },
    { id: 'remote', name: 'Cloud', path: null, provider: 'onedrive' },
  ]);
});

it('encodes a selected path as query data and forwards cancellation', async () => {
  requestMock.mockResolvedValue({ Indexed: false, Items: [
    { Name: 'notes.txt', Path: 'A&B/notes.txt', IsDirectory: false, Size: 5, ModifiedAt: null },
  ] });
  const controller = new AbortController();
  await expect(listFolderContents('library', 'root', 'A&B', controller.signal)).resolves.toEqual({ indexed: false, items: [
    { name: 'notes.txt', path: 'A&B/notes.txt', isDirectory: false, size: 5, modifiedAt: null },
  ] });
  expect(requestMock).toHaveBeenCalledWith('/Admin/Libraries/library/Folders/root/Contents?Path=A%26B', { signal: controller.signal });
});

it.each([
  { Indexed: false },
  { Indexed: true, Items: [{ Name: 'bad', Path: 'bad', IsDirectory: false, Size: -1, ModifiedAt: null }] },
  { Indexed: false, Items: [{ Name: 'bad', Path: 'bad', IsDirectory: false, Size: 1, ModifiedAt: 'invalid' }] },
])('rejects malformed content metadata', async (value) => {
  requestMock.mockResolvedValue(value);
  await expect(listFolderContents('library', 'root', '')).rejects.toMatchObject({ category: 'invalid-response' });
});
