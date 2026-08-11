import { apiRequest } from '../api/httpClient';
import { getThemeSettings, saveThemeSettings } from './themeSettingsApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);
const response = {
  ActiveThemeId: 'cinema',
  Configurations: [
    { ThemeId: 'classic', SchemaVersion: 1, Options: { contentWidth: 'standard' } },
    { ThemeId: 'cinema', SchemaVersion: 1, Options: { density: 'compact' } },
  ],
  Revision: 2,
};

beforeEach(() => { requestMock.mockReset(); });

it('loads the complete strict theme settings contract', async () => {
  requestMock.mockResolvedValue(response);
  await expect(getThemeSettings()).resolves.toEqual({
    activeThemeId: 'cinema', revision: 2,
    configurations: [
      { themeId: 'classic', schemaVersion: 1, options: { contentWidth: 'standard' } },
      { themeId: 'cinema', schemaVersion: 1, options: { density: 'compact' } },
    ],
  });
});

it('saves one selected configuration with its CAS revision', async () => {
  requestMock.mockResolvedValue(response);
  await saveThemeSettings({ themeId: 'cinema', schemaVersion: 1, options: { density: 'compact' } }, 1);
  expect(requestMock).toHaveBeenCalledWith('/Admin/System/Theme', {
    method: 'PUT',
    body: JSON.stringify({ ThemeId: 'cinema', SchemaVersion: 1, Options: { density: 'compact' }, Revision: 1 }),
  });
});

it('rejects response drift and invalid local input', async () => {
  requestMock.mockResolvedValue({ ...response, Unexpected: true });
  await expect(getThemeSettings()).rejects.toMatchObject({ category: 'invalid-response' });
  await expect(saveThemeSettings({ themeId: 'Invalid Theme', schemaVersion: 1, options: {} }, 1))
    .rejects.toMatchObject({ category: 'validation' });
});
