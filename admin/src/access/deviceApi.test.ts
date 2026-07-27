import { apiRequest } from '../api/httpClient';
import { deleteDevice, listDevices, updateDeviceName } from './deviceApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);
const userId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f31';

const device = {
  Name: 'Pixel',
  CustomName: 'Living room',
  Id: 'Phone-Aa',
  LastUserName: 'Admin',
  AppName: 'Findroid',
  AppVersion: '0.16.0',
  LastUserId: userId,
  DateLastActivity: '2026-07-26T12:00:00Z',
  Capabilities: {
    PlayableMediaTypes: ['Video', 'Audio'],
    SupportedCommands: ['Play'],
    SupportsMediaControl: true,
    SupportsPersistentIdentifier: true,
    DeviceProfile: { Name: 'Findroid' },
    AppStoreUrl: null,
    IconUrl: 'https://example.invalid/icon.png',
  },
  IconUrl: 'https://example.invalid/icon.png',
};

beforeEach(() => {
  requestMock.mockReset();
});

it('loads the complete device envelope and forwards an abort signal', async () => {
  requestMock.mockResolvedValue({ Items: [device], TotalRecordCount: 1, StartIndex: 0 });
  const controller = new AbortController();

  await expect(listDevices(controller.signal)).resolves.toEqual([{
    name: 'Pixel',
    customName: 'Living room',
    id: 'Phone-Aa',
    lastUserName: 'Admin',
    appName: 'Findroid',
    appVersion: '0.16.0',
    lastUserId: userId,
    dateLastActivity: '2026-07-26T12:00:00Z',
    capabilities: {
      playableMediaTypes: ['Video', 'Audio'],
      supportedCommands: ['Play'],
      supportsMediaControl: true,
      supportsPersistentIdentifier: true,
      deviceProfile: { Name: 'Findroid' },
      appStoreUrl: null,
      iconUrl: 'https://example.invalid/icon.png',
    },
    iconUrl: 'https://example.invalid/icon.png',
  }]);
  expect(requestMock).toHaveBeenCalledWith('/Devices', { signal: controller.signal });
});

it('normalizes omitted nullable device fields and preserves case-sensitive command ids', async () => {
  const withoutOptional = structuredClone(device);
  delete (withoutOptional as Partial<typeof device>).CustomName;
  delete (withoutOptional as Partial<typeof device>).IconUrl;
  requestMock.mockResolvedValueOnce({ Items: [withoutOptional], TotalRecordCount: 1, StartIndex: 0 });

  await expect(listDevices()).resolves.toEqual([
    expect.objectContaining({ id: 'Phone-Aa', customName: null, iconUrl: null }),
  ]);
  expect(requestMock).toHaveBeenNthCalledWith(1, '/Devices', {});

  requestMock.mockResolvedValue(undefined);
  await updateDeviceName('Phone-Aa', 'Living room');
  await deleteDevice('Phone-Aa');
  expect(requestMock).toHaveBeenNthCalledWith(2, '/Devices/Options?id=Phone-Aa', {
    method: 'POST',
    body: JSON.stringify({ DeviceId: 'Phone-Aa', CustomName: 'Living room' }),
  });
  expect(requestMock).toHaveBeenNthCalledWith(3, '/Devices?id=Phone-Aa', {
    method: 'DELETE',
  });
});

it.each([
  null,
  { Items: {}, TotalRecordCount: 0, StartIndex: 0 },
  { Items: [device], TotalRecordCount: 2, StartIndex: 0 },
  { Items: [{ ...device, Id: '' }], TotalRecordCount: 1, StartIndex: 0 },
  { Items: [{ ...device, Capabilities: { ...device.Capabilities, SupportsMediaControl: 'yes' } }], TotalRecordCount: 1, StartIndex: 0 },
])('rejects a malformed device response %#', async (response) => {
  requestMock.mockResolvedValue(response);
  await expect(listDevices()).rejects.toMatchObject({ category: 'invalid-response' });
});
