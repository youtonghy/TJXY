import { ApiError, apiRequest } from '../api/httpClient';
import { createApiKey, deleteApiKey, listApiKeys } from './apiKeyApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);
const userId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f31';
const rawToken = '0123456789abcdef'.repeat(4);
const apiKey = {
  Id: 7,
  AccessToken: rawToken,
  DeviceId: null,
  AppName: 'Kodi Sync',
  AppVersion: null,
  DeviceName: null,
  UserId: userId,
  IsActive: true,
  DateCreated: '2026-07-26T12:00:00Z',
  DateRevoked: null,
  DateLastActivity: null,
  UserName: 'Admin',
};

beforeEach(() => {
  requestMock.mockReset();
});

it('loads every complete and nullable API key field with abort support', async () => {
  requestMock.mockResolvedValue({ Items: [apiKey], TotalRecordCount: 1, StartIndex: 0 });
  const controller = new AbortController();

  await expect(listApiKeys(controller.signal)).resolves.toEqual([{
    id: 7,
    accessToken: rawToken,
    deviceId: null,
    appName: 'Kodi Sync',
    appVersion: null,
    deviceName: null,
    userId,
    isActive: true,
    dateCreated: '2026-07-26T12:00:00Z',
    dateRevoked: null,
    dateLastActivity: null,
    userName: 'Admin',
  }]);
  expect(requestMock).toHaveBeenCalledWith('/Auth/Keys', { signal: controller.signal });
});

it('encodes app query values and raw-token path segments exactly', async () => {
  requestMock.mockResolvedValue(undefined);

  await createApiKey('Kodi / Sync');
  await deleteApiKey('raw/key');

  expect(requestMock).toHaveBeenNthCalledWith(1, '/Auth/Keys?app=Kodi+%2F+Sync', {
    method: 'POST',
  });
  expect(requestMock).toHaveBeenNthCalledWith(2, '/Auth/Keys/raw%2Fkey', {
    method: 'DELETE',
  });
});

it.each([
  null,
  { Items: {}, TotalRecordCount: 0, StartIndex: 0 },
  { Items: [apiKey], TotalRecordCount: 2, StartIndex: 0 },
  { Items: [{ ...apiKey, AccessToken: 'raw/key' }], TotalRecordCount: 1, StartIndex: 0 },
  { Items: [{ ...apiKey, DeviceId: undefined }], TotalRecordCount: 1, StartIndex: 0 },
  { Items: [{ ...apiKey, DateLastActivity: 0 }], TotalRecordCount: 1, StartIndex: 0 },
  { Items: [{ ...apiKey, IsActive: 'true' }], TotalRecordCount: 1, StartIndex: 0 },
])('rejects a malformed API key response %#', async (response) => {
  requestMock.mockResolvedValue(response);
  await expect(listApiKeys()).rejects.toMatchObject({ category: 'invalid-response' });
});

it('never includes a raw token in validation or request errors', async () => {
  const distinctiveToken = 'do-not-render-this-token';
  const invalid = `${distinctiveToken}\n`;
  const validationError = await deleteApiKey(invalid).catch((error: unknown) => error);
  expect(validationError).toBeInstanceOf(ApiError);
  expect(String(validationError)).not.toContain(distinctiveToken);

  requestMock.mockRejectedValue(new ApiError(503, 'unavailable', 'The server is unavailable.'));
  const requestError = await deleteApiKey(distinctiveToken).catch((error: unknown) => error);
  expect(String(requestError)).not.toContain(distinctiveToken);
});
