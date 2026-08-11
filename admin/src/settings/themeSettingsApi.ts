import { ApiError, apiRequest } from '../api/httpClient';
import { invalidResponse, isRecord } from '../api/responseValidation';

export interface ThemeConfiguration {
  themeId: string;
  schemaVersion: number;
  options: Record<string, unknown>;
}

export interface ThemeSettings {
  activeThemeId: string;
  configurations: ThemeConfiguration[];
  revision: number;
}

export async function getThemeSettings(signal?: AbortSignal): Promise<ThemeSettings> {
  const value = await apiRequest<unknown>('/Admin/System/Theme', signal === undefined ? {} : { signal });
  return parseThemeSettings(value);
}

export async function saveThemeSettings(
  configuration: ThemeConfiguration,
  revision: number,
): Promise<ThemeSettings> {
  requireConfiguration(configuration);
  if (!Number.isSafeInteger(revision) || revision < 0) {
    throw new ApiError(400, 'validation', 'A valid theme settings revision is required.');
  }
  const value = await apiRequest<unknown>('/Admin/System/Theme', {
    method: 'PUT',
    body: JSON.stringify({
      ThemeId: configuration.themeId,
      SchemaVersion: configuration.schemaVersion,
      Options: configuration.options,
      ...(revision > 0 ? { Revision: revision } : {}),
    }),
  });
  return parseThemeSettings(value);
}

function parseThemeSettings(value: unknown): ThemeSettings {
  if (
    !isRecord(value)
    || !exactKeys(value, ['ActiveThemeId', 'Configurations', 'Revision'])
    || !validThemeId(value.ActiveThemeId)
    || !Array.isArray(value.Configurations)
    || value.Configurations.length === 0
    || value.Configurations.length > 32
    || !nonNegativeInteger(value.Revision)
  ) throw invalidResponse('theme settings');
  const configurations = value.Configurations.map(parseConfiguration);
  if (!configurations.some(({ themeId }) => themeId === value.ActiveThemeId)) {
    throw invalidResponse('theme settings');
  }
  return { activeThemeId: value.ActiveThemeId, configurations, revision: value.Revision };
}

function parseConfiguration(value: unknown): ThemeConfiguration {
  if (
    !isRecord(value)
    || !exactKeys(value, ['ThemeId', 'SchemaVersion', 'Options'])
    || !validThemeId(value.ThemeId)
    || !positiveInteger(value.SchemaVersion)
    || value.SchemaVersion > 1_000
    || !isRecord(value.Options)
  ) throw invalidResponse('theme configuration');
  const configuration = {
    themeId: value.ThemeId,
    schemaVersion: value.SchemaVersion,
    options: value.Options,
  };
  requireConfiguration(configuration);
  return configuration;
}

function requireConfiguration(configuration: ThemeConfiguration) {
  if (
    !validThemeId(configuration.themeId)
    || !positiveInteger(configuration.schemaVersion)
    || configuration.schemaVersion > 1_000
    || !isRecord(configuration.options)
    || JSON.stringify(configuration.options).length > 16 * 1024
  ) throw new ApiError(400, 'validation', 'The site theme configuration is invalid.');
}

function validThemeId(value: unknown): value is string {
  return typeof value === 'string' && /^[a-z][a-z0-9-]{0,63}$/u.test(value);
}
function positiveInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value > 0;
}
function nonNegativeInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}
function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length
    && actual.every((key, index) => key === expected[index]);
}
