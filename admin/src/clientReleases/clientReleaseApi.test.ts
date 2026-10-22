import { apiRequest } from '../api/httpClient';
import { clientRequest } from '../client/api/clientApi';
import { checkClientUpdate, listClientReleases, uploadClientRelease } from './clientReleaseApi';
import { guessTarget } from './releaseFormat';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});
vi.mock('../client/api/clientApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('../client/api/clientApi')>();
  return { ...original, clientRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);
const clientRequestMock = vi.mocked(clientRequest);
const id = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const row = {
  Id: id,
  Platform: 'windows',
  Arch: 'x64',
  Version: '1.2.0',
  FileName: 'TJXY-1.2.0-setup-x64.exe',
  Size: 42,
  Sha256: 'a'.repeat(64),
  Notes: '',
  IsLatest: true,
  CreatedAt: '2026-10-22T01:00:00Z',
  DownloadPath: `/ClientUpdates/${id}/Download`,
};

beforeEach(() => { requestMock.mockReset(); clientRequestMock.mockReset(); });

it('streams the package as the raw body and attaches notes afterwards', async () => {
  requestMock.mockResolvedValueOnce(row).mockResolvedValueOnce({ ...row, Notes: 'Faster' });
  const file = new File(['installer'], 'TJXY-1.2.0-setup-x64.exe');

  const release = await uploadClientRelease({ platform: 'windows', arch: 'x64', version: ' 1.2.0 ', file, notes: 'Faster', markLatest: true });

  expect(release.notes).toBe('Faster');
  expect(requestMock).toHaveBeenNthCalledWith(1, '/Admin/ClientReleases?Platform=windows&Arch=x64&Version=1.2.0&FileName=TJXY-1.2.0-setup-x64.exe&Latest=true', {
    method: 'POST', body: file, headers: { 'Content-Type': 'application/octet-stream' },
  });
  expect(requestMock).toHaveBeenNthCalledWith(2, `/Admin/ClientReleases/${id}`, { method: 'PUT', body: JSON.stringify({ Notes: 'Faster' }) });
});

it('rejects packages that do not match the platform before uploading', async () => {
  await expect(uploadClientRelease({ platform: 'android', arch: 'universal', version: '1.0.0', file: new File(['x'], 'TJXY.exe'), notes: '', markLatest: false })).rejects.toMatchObject({ category: 'validation' });
  await expect(uploadClientRelease({ platform: 'android', arch: 'universal', version: '1.0', file: new File(['x'], 'TJXY.apk'), notes: '', markLatest: false })).rejects.toMatchObject({ category: 'validation' });
  expect(requestMock).not.toHaveBeenCalled();
});

it('checks for updates and validates the response shape', async () => {
  clientRequestMock.mockResolvedValueOnce({ UpdateAvailable: true, Latest: row });
  await expect(checkClientUpdate('windows', 'x64', '1.0.0')).resolves.toMatchObject({ updateAvailable: true, latest: { version: '1.2.0', size: 42 } });
  expect(clientRequestMock).toHaveBeenCalledWith('/ClientUpdates/Latest?Platform=windows&Arch=x64&CurrentVersion=1.0.0', {});

  clientRequestMock.mockResolvedValueOnce({ UpdateAvailable: true, Latest: null });
  await expect(checkClientUpdate('windows', 'x64', '1.0.0')).rejects.toMatchObject({ category: 'invalid-response' });
  requestMock.mockResolvedValueOnce({ Items: [{ ...row, DownloadPath: 'https://elsewhere.example/file.exe' }] });
  await expect(listClientReleases()).rejects.toMatchObject({ category: 'invalid-response' });
});

it('guesses the release target from conventional artifact names', () => {
  expect(guessTarget('TJXY-1.2.0-setup-arm64.dmg')).toEqual({ platform: 'macos', arch: 'arm64', version: '1.2.0' });
  expect(guessTarget('TJXY-0.3.1.apk')).toEqual({ platform: 'android', arch: 'universal', version: '0.3.1' });
  expect(guessTarget('notes.txt')).toEqual({});
});
