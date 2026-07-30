import { ApiError, apiRequest } from '../api/httpClient';
import {
  hasControlCharacters,
  invalidResponse,
  isRecord,
  validText,
} from '../api/responseValidation';

const SETTINGS_PATH = '/Admin/Metadata/Providers/Tmdb';
const SETTINGS_KEYS = [
  'Provider',
  'Configured',
  'Enabled',
  'Language',
  'Revision',
  'Source',
  'EncryptionAvailable',
] as const;

export type TmdbSettingsSource = 'None' | 'Environment' | 'Database';

export interface TmdbSettings {
  provider: 'Tmdb';
  configured: boolean;
  enabled: boolean;
  language: string;
  revision: number | null;
  source: TmdbSettingsSource;
  encryptionAvailable: boolean;
}

export interface SaveTmdbSettingsRequest {
  enabled: boolean;
  language: string;
  accessToken: string;
  revision: number | null;
}

export interface TestTmdbConnectionRequest {
  accessToken?: string;
  language?: string;
}

export async function getTmdbSettings(signal?: AbortSignal): Promise<TmdbSettings> {
  const value = await apiRequest<unknown>(
    SETTINGS_PATH,
    signal === undefined ? {} : { signal },
  );
  return toSettings(value);
}

export async function saveTmdbSettings(
  request: SaveTmdbSettingsRequest,
): Promise<TmdbSettings> {
  const body: Record<string, unknown> = {
    Enabled: request.enabled,
    Language: requireLanguage(request.language),
  };
  const accessToken = optionalAccessToken(request.accessToken);
  if (accessToken !== undefined) body.AccessToken = accessToken;
  if (request.revision !== null) body.Revision = requireRevision(request.revision);

  const value = await apiRequest<unknown>(SETTINGS_PATH, {
    method: 'PUT',
    body: JSON.stringify(body),
  });
  return toSettings(value);
}

export async function testTmdbConnection(
  request: TestTmdbConnectionRequest,
): Promise<void> {
  const body: Record<string, string> = {};
  const accessToken = optionalAccessToken(request.accessToken);
  if (accessToken !== undefined) body.AccessToken = accessToken;
  if (request.language !== undefined) body.Language = requireLanguage(request.language);

  const value = await apiRequest<unknown>(`${SETTINGS_PATH}/Test`, {
    method: 'POST',
    body: JSON.stringify(body),
  });
  if (
    !isRecord(value)
    || !hasExactKeys(value, ['Status'])
    || value.Status !== 'Success'
  ) {
    throw invalidResponse('TMDB connection-test result');
  }
}

export async function deleteTmdbSettings(): Promise<void> {
  await apiRequest(SETTINGS_PATH, { method: 'DELETE' });
}

function toSettings(value: unknown): TmdbSettings {
  if (
    !isRecord(value)
    || !hasExactKeys(value, SETTINGS_KEYS)
    || value.Provider !== 'Tmdb'
    || typeof value.Configured !== 'boolean'
    || typeof value.Enabled !== 'boolean'
    || !validText(value.Language, 32)
    || (value.Revision !== null && !validRevision(value.Revision))
    || !validSource(value.Source)
    || typeof value.EncryptionAvailable !== 'boolean'
  ) {
    throw invalidResponse('TMDB metadata settings');
  }
  return {
    provider: value.Provider,
    configured: value.Configured,
    enabled: value.Enabled,
    language: value.Language,
    revision: value.Revision,
    source: value.Source,
    encryptionAvailable: value.EncryptionAvailable,
  };
}

function requireLanguage(value: string): string {
  const language = value.trim();
  if (
    !validText(language, 32)
    || !/^[a-z]{2,3}(?:-[a-z0-9]{2,8})*$/iu.test(language)
  ) {
    throw new ApiError(400, 'validation', 'A valid metadata language is required.');
  }
  return language;
}

function optionalAccessToken(value: string | undefined): string | undefined {
  if (value === undefined || value.length === 0) return undefined;
  const accessToken = value.trim();
  if (
    accessToken.length === 0
    || accessToken.length > 8_192
    || hasControlCharacters(accessToken)
    || /\s/u.test(accessToken)
  ) {
    throw new ApiError(400, 'validation', 'A valid TMDB access token is required.');
  }
  return accessToken;
}

function requireRevision(value: number): number {
  if (!validRevision(value)) {
    throw new ApiError(400, 'validation', 'A valid settings revision is required.');
  }
  return value;
}

function validRevision(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 1;
}

function validSource(value: unknown): value is TmdbSettingsSource {
  return value === 'None' || value === 'Environment' || value === 'Database';
}

function hasExactKeys(
  value: Record<string, unknown>,
  keys: readonly string[],
): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length
    && actual.every((key, index) => key === expected[index]);
}
