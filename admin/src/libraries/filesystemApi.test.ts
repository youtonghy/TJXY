import { apiRequest } from '../api/httpClient';
import { attachFilesystemFolder } from './filesystemApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);

beforeEach(() => {
  requestMock.mockReset();
});

it('attaches an absolute server path', async () => {
  requestMock.mockResolvedValue(undefined);

  await attachFilesystemFolder('library-1', '/mnt/media/Movies');

  expect(requestMock).toHaveBeenCalledWith('/Library/VirtualFolders/Paths', expect.objectContaining({
    method: 'POST',
  }));
  const body: unknown = JSON.parse(requestMock.mock.calls[0]?.[1]?.body as string);
  expect(body).toEqual({
    LibraryId: 'library-1',
    Path: '/mnt/media/Movies',
  });
});
