import { apiRequest } from '../api/httpClient';
import { detachLibraryFolder, listFolderContents, listLibraryFolders } from './libraryFoldersApi';

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

it('detaches a folder through the virtual folders path contract', async () => {
  requestMock.mockResolvedValue(undefined);
  await detachLibraryFolder('Movies', 'root-1');
  expect(requestMock).toHaveBeenCalledWith(
    '/Library/VirtualFolders/Paths?name=Movies&path=tjxy%3A%2F%2Fstorage-root%2Froot-1&refreshLibrary=false',
    { method: 'DELETE' },
  );
});

it.each([{ name: '', root: 'root-1' }, { name: 'Movies', root: '' }])(
  'rejects detach requests without a complete binding reference',
  async ({ name, root }) => {
    await expect(detachLibraryFolder(name, root)).rejects.toMatchObject({ category: 'validation' });
    expect(requestMock).not.toHaveBeenCalled();
  },
);

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
