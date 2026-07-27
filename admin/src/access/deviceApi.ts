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

export interface DeviceCapabilities {
  playableMediaTypes: string[];
  supportedCommands: string[];
  supportsMediaControl: boolean;
  supportsPersistentIdentifier: boolean;
  deviceProfile: Record<string, unknown> | null;
  appStoreUrl: string | null;
  iconUrl: string | null;
}

export interface DeviceInfo {
  name: string;
  customName: string | null;
  id: string;
  lastUserName: string;
  appName: string;
  appVersion: string;
  lastUserId: string;
  dateLastActivity: string;
  capabilities: DeviceCapabilities;
  iconUrl: string | null;
}

export async function listDevices(signal?: AbortSignal): Promise<DeviceInfo[]> {
  const value = await apiRequest<unknown>('/Devices', signal === undefined ? {} : { signal });
  if (
    !isRecord(value)
    || !Array.isArray(value.Items)
    || value.StartIndex !== 0
    || !isNonNegativeInteger(value.TotalRecordCount)
    || value.TotalRecordCount !== value.Items.length
  ) throw invalidResponse('device list');
  return value.Items.map(toDevice);
}

export async function updateDeviceName(
  deviceId: string,
  customName: string | null,
): Promise<void> {
  const id = requireDeviceId(deviceId);
  if (!validCustomName(customName)) {
    throw new ApiError(400, 'validation', 'A valid custom device name is required.');
  }
  const query = new URLSearchParams({ id });
  await apiRequest(`/Devices/Options?${query.toString()}`, {
    method: 'POST',
    body: JSON.stringify({ DeviceId: id, CustomName: customName }),
  });
}

export async function deleteDevice(deviceId: string): Promise<void> {
  const query = new URLSearchParams({ id: requireDeviceId(deviceId) });
  await apiRequest(`/Devices?${query.toString()}`, { method: 'DELETE' });
}

function toDevice(value: unknown): DeviceInfo {
  if (
    !isRecord(value)
    || !validText(value.Name, 512)
    || !validOptionalText(value.CustomName, 256, true)
    || !validDeviceId(value.Id)
    || !validText(value.LastUserName, 512)
    || !validText(value.AppName, 512)
    || !validText(value.AppVersion, 512)
    || !validUuid(value.LastUserId)
    || !validDate(value.DateLastActivity)
    || !isRecord(value.Capabilities)
    || !validStringArray(value.Capabilities.PlayableMediaTypes)
    || !validStringArray(value.Capabilities.SupportedCommands)
    || typeof value.Capabilities.SupportsMediaControl !== 'boolean'
    || typeof value.Capabilities.SupportsPersistentIdentifier !== 'boolean'
    || !Object.hasOwn(value.Capabilities, 'DeviceProfile')
    || !(value.Capabilities.DeviceProfile === null || isRecord(value.Capabilities.DeviceProfile))
    || !Object.hasOwn(value.Capabilities, 'AppStoreUrl')
    || !validOptionalText(value.Capabilities.AppStoreUrl, 16_384)
    || !Object.hasOwn(value.Capabilities, 'IconUrl')
    || !validOptionalText(value.Capabilities.IconUrl, 16_384)
    || !validOptionalText(value.IconUrl, 16_384)
  ) throw invalidResponse('device');

  return {
    name: value.Name,
    customName: value.CustomName ?? null,
    id: value.Id,
    lastUserName: value.LastUserName,
    appName: value.AppName,
    appVersion: value.AppVersion,
    lastUserId: value.LastUserId,
    dateLastActivity: value.DateLastActivity,
    capabilities: {
      playableMediaTypes: value.Capabilities.PlayableMediaTypes,
      supportedCommands: value.Capabilities.SupportedCommands,
      supportsMediaControl: value.Capabilities.SupportsMediaControl,
      supportsPersistentIdentifier: value.Capabilities.SupportsPersistentIdentifier,
      deviceProfile: value.Capabilities.DeviceProfile ?? null,
      appStoreUrl: value.Capabilities.AppStoreUrl ?? null,
      iconUrl: value.Capabilities.IconUrl ?? null,
    },
    iconUrl: value.IconUrl ?? null,
  };
}

function requireDeviceId(value: string): string {
  if (!validDeviceId(value)) {
    throw new ApiError(400, 'validation', 'A valid device identifier is required.');
  }
  return value;
}

function validDeviceId(value: unknown): value is string {
  return validText(value, 512);
}

function validCustomName(value: unknown): value is string | null {
  return value === null
    || (typeof value === 'string' && Array.from(value).length <= 256 && !hasControlCharacters(value));
}

function validOptionalText(
  value: unknown,
  maxLength: number,
  allowEmpty = false,
): value is string | null | undefined {
  return value === null
    || value === undefined
    || validText(value, maxLength, allowEmpty);
}

function validStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => validText(item, 512));
}
