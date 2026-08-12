import { ApiError, apiRequest } from '../api/httpClient';
import {
  hasControlCharacters,
  invalidResponse,
  isNonNegativeInteger,
  isRecord,
  validDate,
  validText,
} from '../api/responseValidation';

const TMDB_SETTINGS_PATH = '/Admin/Metadata/Providers/Tmdb';
const THEAUDIODB_SETTINGS_PATH = '/Admin/Metadata/Providers/TheAudioDB';
const MUSICBRAINZ_SETTINGS_PATH = '/Admin/Metadata/Providers/MusicBrainz';
const LOCAL_METADATA_PATH = '/Admin/Metadata/Local';
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
export type MetadataSettingsSource = TmdbSettingsSource;

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

export interface MusicProviderSettings {
  provider: 'TheAudioDB' | 'MusicBrainz';
  configured: boolean;
  enabled: boolean;
  revision: number | null;
  source: MetadataSettingsSource;
  encryptionAvailable: boolean;
}

export interface TheAudioDbSettings extends MusicProviderSettings {
  provider: 'TheAudioDB';
}

export interface MusicBrainzSettings extends MusicProviderSettings {
  provider: 'MusicBrainz';
  userAgent: string;
}

export interface SaveTheAudioDbSettingsRequest {
  enabled: boolean;
  apiKey: string;
  revision: number | null;
}

export interface SaveMusicBrainzSettingsRequest {
  enabled: boolean;
  userAgent: string;
  revision: number | null;
}

export interface TestTheAudioDbConnectionRequest {
  apiKey?: string;
}

export interface TestMusicBrainzConnectionRequest {
  userAgent?: string;
}

export interface LocalMetadataMetric { count: number; bytes: number }
export interface LocalMetadataStorage {
  currentPath: string;
  pendingPath: string | null;
  historicalLocations: string[];
  source: 'Default' | 'Database' | 'Environment';
  locationEditable: boolean;
  restartRequired: boolean;
  checkedAt: string;
  statistics: Record<'total' | 'linked' | 'orphaned' | 'missing' | 'unregistered', LocalMetadataMetric>;
  cleanupInProgress: boolean;
}
export interface LocalMetadataCleanupResult {
  deleted: LocalMetadataMetric;
  skippedCount: number;
  failedCount: number;
  storage: LocalMetadataStorage;
}

export async function getLocalMetadataStorage(signal?: AbortSignal): Promise<LocalMetadataStorage> {
  return toLocalMetadataStorage(await apiRequest<unknown>(LOCAL_METADATA_PATH, signal === undefined ? {} : { signal }));
}

export async function saveLocalMetadataLocation(path: string): Promise<LocalMetadataStorage> {
  const normalized = path.trim();
  if (!validText(normalized, 4096)) throw new ApiError(400, 'validation', 'A valid metadata location is required.');
  return toLocalMetadataStorage(await apiRequest<unknown>(`${LOCAL_METADATA_PATH}/Location`, { method: 'PUT', body: JSON.stringify({ Path: normalized }) }));
}

export async function cleanupLocalMetadata(): Promise<LocalMetadataCleanupResult> {
  const value = await apiRequest<unknown>(`${LOCAL_METADATA_PATH}/Cleanup`, { method: 'POST' });
  if (!isRecord(value) || !hasExactKeys(value, ['Deleted', 'SkippedCount', 'FailedCount', 'Storage']) || !isNonNegativeInteger(value.SkippedCount) || !isNonNegativeInteger(value.FailedCount)) throw invalidResponse('local metadata cleanup result');
  return { deleted: toMetric(value.Deleted), skippedCount: value.SkippedCount, failedCount: value.FailedCount, storage: toLocalMetadataStorage(value.Storage) };
}

function toMetric(value: unknown): LocalMetadataMetric {
  if (!isRecord(value) || !hasExactKeys(value, ['Count', 'Bytes']) || !isNonNegativeInteger(value.Count) || !isNonNegativeInteger(value.Bytes)) throw invalidResponse('local metadata metric');
  return { count: value.Count, bytes: value.Bytes };
}

function toLocalMetadataStorage(value: unknown): LocalMetadataStorage {
  const keys = ['CurrentPath', 'PendingPath', 'HistoricalLocations', 'Source', 'LocationEditable', 'RestartRequired', 'CheckedAt', 'Statistics', 'CleanupInProgress'] as const;
  if (!isRecord(value) || !hasExactKeys(value, keys) || !validText(value.CurrentPath, 4096) || (value.PendingPath !== null && !validText(value.PendingPath, 4096)) || !Array.isArray(value.HistoricalLocations) || !value.HistoricalLocations.every((path) => validText(path, 4096)) || !['Default', 'Database', 'Environment'].includes(value.Source as string) || typeof value.LocationEditable !== 'boolean' || typeof value.RestartRequired !== 'boolean' || !validDate(value.CheckedAt) || typeof value.CleanupInProgress !== 'boolean' || !isRecord(value.Statistics) || !hasExactKeys(value.Statistics, ['Total', 'Linked', 'Orphaned', 'Missing', 'Unregistered'])) throw invalidResponse('local metadata storage');
  return { currentPath: value.CurrentPath, pendingPath: value.PendingPath, historicalLocations: value.HistoricalLocations, source: value.Source as LocalMetadataStorage['source'], locationEditable: value.LocationEditable, restartRequired: value.RestartRequired, checkedAt: value.CheckedAt, statistics: { total: toMetric(value.Statistics.Total), linked: toMetric(value.Statistics.Linked), orphaned: toMetric(value.Statistics.Orphaned), missing: toMetric(value.Statistics.Missing), unregistered: toMetric(value.Statistics.Unregistered) }, cleanupInProgress: value.CleanupInProgress };
}

export async function getTmdbSettings(signal?: AbortSignal): Promise<TmdbSettings> {
  const value = await apiRequest<unknown>(
    TMDB_SETTINGS_PATH,
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

  const value = await apiRequest<unknown>(TMDB_SETTINGS_PATH, {
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

  const value = await apiRequest<unknown>(`${TMDB_SETTINGS_PATH}/Test`, {
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
  await apiRequest(TMDB_SETTINGS_PATH, { method: 'DELETE' });
}

export async function getTheAudioDbSettings(
  signal?: AbortSignal,
): Promise<TheAudioDbSettings> {
  const value = await apiRequest<unknown>(
    THEAUDIODB_SETTINGS_PATH,
    signal === undefined ? {} : { signal },
  );
  return toTheAudioDbSettings(value);
}

export async function saveTheAudioDbSettings(
  request: SaveTheAudioDbSettingsRequest,
): Promise<TheAudioDbSettings> {
  const body: Record<string, unknown> = { Enabled: request.enabled };
  const apiKey = optionalApiKey(request.apiKey);
  if (apiKey !== undefined) body.ApiKey = apiKey;
  if (request.revision !== null) body.Revision = requireRevision(request.revision);
  const value = await apiRequest<unknown>(THEAUDIODB_SETTINGS_PATH, {
    method: 'PUT',
    body: JSON.stringify(body),
  });
  return toTheAudioDbSettings(value);
}

export async function testTheAudioDbConnection(
  request: TestTheAudioDbConnectionRequest,
): Promise<void> {
  const body: Record<string, string> = {};
  const apiKey = optionalApiKey(request.apiKey);
  if (apiKey !== undefined) body.ApiKey = apiKey;
  await testConnection(THEAUDIODB_SETTINGS_PATH, body, 'TheAudioDB');
}

export async function deleteTheAudioDbSettings(): Promise<void> {
  await apiRequest(THEAUDIODB_SETTINGS_PATH, { method: 'DELETE' });
}

export async function getMusicBrainzSettings(
  signal?: AbortSignal,
): Promise<MusicBrainzSettings> {
  const value = await apiRequest<unknown>(
    MUSICBRAINZ_SETTINGS_PATH,
    signal === undefined ? {} : { signal },
  );
  return toMusicBrainzSettings(value);
}

export async function saveMusicBrainzSettings(
  request: SaveMusicBrainzSettingsRequest,
): Promise<MusicBrainzSettings> {
  const body: Record<string, unknown> = { Enabled: request.enabled };
  const userAgent = optionalUserAgent(request.userAgent);
  if (userAgent !== undefined) body.UserAgent = userAgent;
  if (request.revision !== null) body.Revision = requireRevision(request.revision);
  const value = await apiRequest<unknown>(MUSICBRAINZ_SETTINGS_PATH, {
    method: 'PUT',
    body: JSON.stringify(body),
  });
  return toMusicBrainzSettings(value);
}

export async function testMusicBrainzConnection(
  request: TestMusicBrainzConnectionRequest,
): Promise<void> {
  const body: Record<string, string> = {};
  const userAgent = optionalUserAgent(request.userAgent);
  if (userAgent !== undefined) body.UserAgent = userAgent;
  await testConnection(MUSICBRAINZ_SETTINGS_PATH, body, 'MusicBrainz');
}

export async function deleteMusicBrainzSettings(): Promise<void> {
  await apiRequest(MUSICBRAINZ_SETTINGS_PATH, { method: 'DELETE' });
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

const MUSIC_SECRET_SETTINGS_KEYS = [
  'Provider',
  'Configured',
  'Enabled',
  'Revision',
  'Source',
  'EncryptionAvailable',
] as const;

function toTheAudioDbSettings(value: unknown): TheAudioDbSettings {
  const common = toMusicProviderSettings(
    value,
    'TheAudioDB',
    MUSIC_SECRET_SETTINGS_KEYS,
  );
  return { ...common, provider: 'TheAudioDB' };
}

function toMusicBrainzSettings(value: unknown): MusicBrainzSettings {
  const keys = [...MUSIC_SECRET_SETTINGS_KEYS, 'UserAgent'] as const;
  const common = toMusicProviderSettings(value, 'MusicBrainz', keys);
  if (!isRecord(value) || typeof value.UserAgent !== 'string' || value.UserAgent.length > 512) {
    throw invalidResponse('MusicBrainz metadata settings');
  }
  return { ...common, provider: 'MusicBrainz', userAgent: value.UserAgent };
}

function toMusicProviderSettings(
  value: unknown,
  provider: MusicProviderSettings['provider'],
  keys: readonly string[],
): MusicProviderSettings {
  if (
    !isRecord(value)
    || !hasExactKeys(value, keys)
    || value.Provider !== provider
    || typeof value.Configured !== 'boolean'
    || typeof value.Enabled !== 'boolean'
    || (value.Revision !== null && !validRevision(value.Revision))
    || !validSource(value.Source)
    || typeof value.EncryptionAvailable !== 'boolean'
  ) {
    throw invalidResponse(`${provider} metadata settings`);
  }
  return {
    provider,
    configured: value.Configured,
    enabled: value.Enabled,
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

function optionalApiKey(value: string | undefined): string | undefined {
  if (value === undefined || value.length === 0) return undefined;
  const apiKey = value.trim();
  if (
    apiKey.length === 0
    || apiKey.length > 256
    || !/^[a-z0-9_-]+$/iu.test(apiKey)
  ) {
    throw new ApiError(400, 'validation', 'A valid TheAudioDB API key is required.');
  }
  return apiKey;
}

function optionalUserAgent(value: string | undefined): string | undefined {
  if (value === undefined || value.length === 0) return undefined;
  if (
    value.trim() !== value
    || !validText(value, 512)
  ) {
    throw new ApiError(400, 'validation', 'A valid MusicBrainz User-Agent is required.');
  }
  return value;
}

async function testConnection(
  path: string,
  body: Record<string, string>,
  provider: string,
): Promise<void> {
  const value = await apiRequest<unknown>(`${path}/Test`, {
    method: 'POST',
    body: JSON.stringify(body),
  });
  if (!isRecord(value) || !hasExactKeys(value, ['Status']) || value.Status !== 'Success') {
    throw invalidResponse(`${provider} connection-test result`);
  }
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
