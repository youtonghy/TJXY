import { ApiError, apiRequest } from '../api/httpClient';
import {
  hasControlCharacters,
  invalidResponse,
  isNonNegativeInteger,
  isRecord,
  validDate,
  validText,
  validUuid,
} from '../api/responseValidation';

export interface ApiKeyInfo {
  id: number;
  accessToken: string;
  deviceId: string | null;
  appName: string;
  appVersion: string | null;
  deviceName: string | null;
  userId: string;
  isActive: boolean;
  dateCreated: string;
  dateRevoked: string | null;
  dateLastActivity: string | null;
  userName: string;
}

export async function listApiKeys(signal?: AbortSignal): Promise<ApiKeyInfo[]> {
  const value = await apiRequest<unknown>('/Auth/Keys', signal === undefined ? {} : { signal });
  if (
    !isRecord(value)
    || !Array.isArray(value.Items)
    || value.StartIndex !== 0
    || !isNonNegativeInteger(value.TotalRecordCount)
    || value.TotalRecordCount !== value.Items.length
  ) throw invalidResponse('API key list');
  return value.Items.map(toApiKey);
}

export async function createApiKey(appName: string): Promise<void> {
  const query = new URLSearchParams({
    app: requireAppName(appName),
  });
  await apiRequest(`/Auth/Keys?${query.toString()}`, { method: 'POST' });
}

export async function deleteApiKey(rawToken: string): Promise<void> {
  if (!validTokenTransport(rawToken)) {
    throw new ApiError(400, 'validation', 'A valid API key is required.');
  }
  await apiRequest(`/Auth/Keys/${encodeURIComponent(rawToken)}`, { method: 'DELETE' });
}

function toApiKey(value: unknown): ApiKeyInfo {
  if (
    !isRecord(value)
    || !isPositiveInteger(value.Id)
    || !validCanonicalToken(value.AccessToken)
    || !validNullableText(value.DeviceId, 512)
    || !validText(value.AppName, 256)
    || !validNullableText(value.AppVersion, 512)
    || !validNullableText(value.DeviceName, 512)
    || !validUuid(value.UserId)
    || value.IsActive !== true
    || !validDate(value.DateCreated)
    || !validNullableDate(value.DateRevoked)
    || !validNullableDate(value.DateLastActivity)
    || !validText(value.UserName, 512)
  ) throw invalidResponse('API key');

  return {
    id: value.Id,
    accessToken: value.AccessToken,
    deviceId: value.DeviceId,
    appName: value.AppName,
    appVersion: value.AppVersion,
    deviceName: value.DeviceName,
    userId: value.UserId,
    isActive: value.IsActive,
    dateCreated: value.DateCreated,
    dateRevoked: value.DateRevoked,
    dateLastActivity: value.DateLastActivity,
    userName: value.UserName,
  };
}

function requireAppName(value: string): string {
  const appName = value.trim();
  if (!validText(appName, 256)) {
    throw new ApiError(400, 'validation', 'A valid application name is required.');
  }
  return appName;
}

function validCanonicalToken(value: unknown): value is string {
  return typeof value === 'string'
    && value.length === 64
    && /^[0-9a-f]{64}$/u.test(value);
}

function validTokenTransport(value: unknown): value is string {
  return typeof value === 'string'
    && value.length > 0
    && value.length <= 1_024
    && !hasControlCharacters(value);
}

function validNullableText(value: unknown, maxLength: number): value is string | null {
  return value === null || validText(value, maxLength);
}

function validNullableDate(value: unknown): value is string | null {
  return value === null || validDate(value);
}

function isPositiveInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 1;
}
