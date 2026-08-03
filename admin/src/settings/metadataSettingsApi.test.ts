import { apiRequest } from '../api/httpClient';
import {
  deleteMusicBrainzSettings,
  deleteTheAudioDbSettings,
  deleteTmdbSettings,
  getMusicBrainzSettings,
  getTheAudioDbSettings,
  getTmdbSettings,
  saveMusicBrainzSettings,
  saveTheAudioDbSettings,
  saveTmdbSettings,
  testMusicBrainzConnection,
  testTheAudioDbConnection,
  testTmdbConnection,
} from './metadataSettingsApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);

beforeEach(() => {
  requestMock.mockReset();
});

it('loads and strictly maps the redacted TMDB settings contract', async () => {
  requestMock.mockResolvedValue({
    Provider: 'Tmdb',
    Configured: true,
    Enabled: true,
    Language: 'zh-CN',
    Revision: 4,
    Source: 'Database',
    EncryptionAvailable: true,
  });
  const controller = new AbortController();

  await expect(getTmdbSettings(controller.signal)).resolves.toEqual({
    provider: 'Tmdb',
    configured: true,
    enabled: true,
    language: 'zh-CN',
    revision: 4,
    source: 'Database',
    encryptionAvailable: true,
  });
  expect(requestMock).toHaveBeenCalledWith('/Admin/Metadata/Providers/Tmdb', {
    signal: controller.signal,
  });
});

it('saves only the explicit draft fields and forwards the revision fence', async () => {
  requestMock.mockResolvedValue({
    Provider: 'Tmdb',
    Configured: true,
    Enabled: false,
    Language: 'en-AU',
    Revision: 3,
    Source: 'Database',
    EncryptionAvailable: true,
  });

  await saveTmdbSettings({
    enabled: false,
    language: 'en-AU',
    accessToken: 'private-draft',
    revision: 2,
  });

  expect(requestMock).toHaveBeenCalledWith('/Admin/Metadata/Providers/Tmdb', {
    method: 'PUT',
    body: JSON.stringify({
      Enabled: false,
      Language: 'en-AU',
      AccessToken: 'private-draft',
      Revision: 2,
    }),
  });
});

it('omits an empty replacement token and null revision from saves', async () => {
  requestMock.mockResolvedValue({
    Provider: 'Tmdb',
    Configured: false,
    Enabled: false,
    Language: 'zh-CN',
    Revision: null,
    Source: 'None',
    EncryptionAvailable: true,
  });

  await saveTmdbSettings({
    enabled: false,
    language: 'zh-CN',
    accessToken: '',
    revision: null,
  });

  expect(requestMock).toHaveBeenCalledWith('/Admin/Metadata/Providers/Tmdb', {
    method: 'PUT',
    body: JSON.stringify({ Enabled: false, Language: 'zh-CN' }),
  });
});

it('tests a draft without persisting it and can test the configured token', async () => {
  requestMock.mockResolvedValue({ Status: 'Success' });

  await expect(testTmdbConnection({
    accessToken: 'draft-token',
    language: 'de-DE',
  })).resolves.toBeUndefined();
  await expect(testTmdbConnection({})).resolves.toBeUndefined();

  expect(requestMock).toHaveBeenNthCalledWith(
    1,
    '/Admin/Metadata/Providers/Tmdb/Test',
    {
      method: 'POST',
      body: JSON.stringify({ AccessToken: 'draft-token', Language: 'de-DE' }),
    },
  );
  expect(requestMock).toHaveBeenNthCalledWith(
    2,
    '/Admin/Metadata/Providers/Tmdb/Test',
    { method: 'POST', body: JSON.stringify({}) },
  );
});

it('deletes the database override with no request body', async () => {
  requestMock.mockResolvedValue(undefined);

  await expect(deleteTmdbSettings()).resolves.toBeUndefined();

  expect(requestMock).toHaveBeenCalledWith('/Admin/Metadata/Providers/Tmdb', {
    method: 'DELETE',
  });
});

it('rejects malformed responses without echoing submitted secrets', async () => {
  requestMock.mockResolvedValue({
    Provider: 'Tmdb',
    Configured: true,
    Enabled: true,
    Language: 'zh-CN',
    Revision: 1,
    Source: 'database',
    EncryptionAvailable: true,
    AccessToken: 'server leak',
  });

  await expect(getTmdbSettings()).rejects.toMatchObject({
    category: 'invalid-response',
  });

  requestMock.mockResolvedValue({ Status: 'Rejected', Detail: 'private-draft' });
  const error = await testTmdbConnection({ accessToken: 'private-draft' })
    .then(() => null, (failure: unknown) => failure);
  expect(String(error)).not.toContain('private-draft');
});

it('strictly maps redacted TheAudioDB settings and never accepts an API key', async () => {
  requestMock.mockResolvedValue({
    Provider: 'TheAudioDB',
    Configured: true,
    Enabled: true,
    Revision: 6,
    Source: 'Database',
    EncryptionAvailable: true,
  });

  await expect(getTheAudioDbSettings()).resolves.toEqual({
    provider: 'TheAudioDB',
    configured: true,
    enabled: true,
    revision: 6,
    source: 'Database',
    encryptionAvailable: true,
  });

  requestMock.mockResolvedValue({
    Provider: 'TheAudioDB',
    Configured: true,
    Enabled: true,
    Revision: 6,
    Source: 'Database',
    EncryptionAvailable: true,
    ApiKey: 'server-leak',
  });
  await expect(getTheAudioDbSettings()).rejects.toMatchObject({
    category: 'invalid-response',
  });
});

it('saves, tests, and removes a TheAudioDB database override', async () => {
  requestMock
    .mockResolvedValueOnce({
      Provider: 'TheAudioDB',
      Configured: true,
      Enabled: false,
      Revision: 7,
      Source: 'Database',
      EncryptionAvailable: true,
    })
    .mockResolvedValueOnce({ Status: 'Success' })
    .mockResolvedValueOnce(undefined);

  await saveTheAudioDbSettings({
    enabled: false,
    apiKey: 'private-audio-key',
    revision: 6,
  });
  await testTheAudioDbConnection({ apiKey: 'private-audio-key' });
  await deleteTheAudioDbSettings();

  expect(requestMock).toHaveBeenNthCalledWith(
    1,
    '/Admin/Metadata/Providers/TheAudioDB',
    {
      method: 'PUT',
      body: JSON.stringify({
        Enabled: false,
        ApiKey: 'private-audio-key',
        Revision: 6,
      }),
    },
  );
  expect(requestMock).toHaveBeenNthCalledWith(
    2,
    '/Admin/Metadata/Providers/TheAudioDB/Test',
    { method: 'POST', body: JSON.stringify({ ApiKey: 'private-audio-key' }) },
  );
  expect(requestMock).toHaveBeenNthCalledWith(
    3,
    '/Admin/Metadata/Providers/TheAudioDB',
    { method: 'DELETE' },
  );
});

it('maps and manages the MusicBrainz identifying User-Agent', async () => {
  const userAgent = 'TJXY/1.0 (admin@example.invalid)';
  requestMock
    .mockResolvedValueOnce({
      Provider: 'MusicBrainz',
      Configured: true,
      Enabled: true,
      UserAgent: userAgent,
      Revision: 2,
      Source: 'Database',
      EncryptionAvailable: true,
    })
    .mockResolvedValueOnce({
      Provider: 'MusicBrainz',
      Configured: true,
      Enabled: true,
      UserAgent: userAgent,
      Revision: 3,
      Source: 'Database',
      EncryptionAvailable: true,
    })
    .mockResolvedValueOnce({ Status: 'Success' })
    .mockResolvedValueOnce(undefined);

  await expect(getMusicBrainzSettings()).resolves.toMatchObject({
    provider: 'MusicBrainz',
    userAgent,
    source: 'Database',
  });
  await saveMusicBrainzSettings({ enabled: true, userAgent, revision: 2 });
  await testMusicBrainzConnection({ userAgent });
  await deleteMusicBrainzSettings();

  expect(requestMock).toHaveBeenNthCalledWith(
    2,
    '/Admin/Metadata/Providers/MusicBrainz',
    {
      method: 'PUT',
      body: JSON.stringify({ Enabled: true, UserAgent: userAgent, Revision: 2 }),
    },
  );
  expect(requestMock).toHaveBeenNthCalledWith(
    3,
    '/Admin/Metadata/Providers/MusicBrainz/Test',
    { method: 'POST', body: JSON.stringify({ UserAgent: userAgent }) },
  );
  expect(requestMock).toHaveBeenNthCalledWith(
    4,
    '/Admin/Metadata/Providers/MusicBrainz',
    { method: 'DELETE' },
  );
});
