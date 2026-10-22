import { ApiError, apiRequest } from '../api/httpClient';
import { clientRequest } from '../client/api/clientApi';
import {
  invalidResponse,
  isNonNegativeInteger,
  isRecord,
  validDate,
  validMultilineText,
  validText,
  validUuid,
} from '../api/responseValidation';

export const CLIENT_PLATFORMS = ['windows', 'macos', 'linux', 'android'] as const;
export const CLIENT_ARCHES = ['x64', 'arm64', 'universal'] as const;
export type ClientPlatform = typeof CLIENT_PLATFORMS[number];
export type ClientArch = typeof CLIENT_ARCHES[number];

/** Installer extensions the server accepts per platform; mirrors `ClientPlatform::package_extensions`. */
export const CLIENT_PACKAGE_EXTENSIONS: Record<ClientPlatform, readonly string[]> = {
  windows: ['exe', 'msi'],
  macos: ['dmg', 'zip'],
  linux: ['appimage', 'deb', 'rpm'],
  android: ['apk'],
};

const MAX_NOTES_LENGTH = 8_000;
const SEMVER = /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$/u;
const RELEASE_KEYS = ['Id', 'Platform', 'Arch', 'Version', 'FileName', 'Size', 'Sha256', 'Notes', 'IsLatest', 'CreatedAt', 'DownloadPath'];

export interface ClientRelease {
  id: string;
  platform: ClientPlatform;
  arch: ClientArch;
  version: string;
  fileName: string;
  size: number;
  sha256: string;
  notes: string;
  isLatest: boolean;
  createdAt: string;
  downloadPath: string;
}

export interface ClientReleaseUpload {
  platform: ClientPlatform;
  arch: ClientArch;
  version: string;
  file: File;
  notes: string;
  markLatest: boolean;
}

export interface ClientUpdateCheck {
  updateAvailable: boolean;
  latest: ClientRelease | null;
}

export function isClientVersion(value: string): boolean {
  return value.length <= 64 && SEMVER.test(value);
}

export function packageExtension(platform: ClientPlatform, fileName: string): string | null {
  const extension = fileName.includes('.') ? fileName.slice(fileName.lastIndexOf('.') + 1).toLowerCase() : '';
  return CLIENT_PACKAGE_EXTENSIONS[platform].includes(extension) ? extension : null;
}

export async function listClientReleases(signal?: AbortSignal): Promise<ClientRelease[]> {
  const value = await apiRequest<unknown>('/Admin/ClientReleases', signal === undefined ? {} : { signal });
  if (!isRecord(value) || !exactKeys(value, ['Items']) || !Array.isArray(value.Items)) throw invalidResponse('client release list');
  return value.Items.map(toClientRelease);
}

/** Uploads the package as a raw streamed body, then attaches release notes when present. */
export async function uploadClientRelease(upload: ClientReleaseUpload): Promise<ClientRelease> {
  if (!isPlatform(upload.platform) || !isArch(upload.arch) || !isClientVersion(upload.version.trim()) || packageExtension(upload.platform, upload.file.name) === null || upload.file.size === 0 || !validMultilineText(upload.notes, MAX_NOTES_LENGTH, true)) {
    throw new ApiError(400, 'validation', 'Client release is invalid.');
  }
  const query = new URLSearchParams({ Platform: upload.platform, Arch: upload.arch, Version: upload.version.trim(), FileName: upload.file.name });
  if (upload.markLatest) query.set('Latest', 'true');
  const created = toClientRelease(await apiRequest<unknown>(`/Admin/ClientReleases?${query.toString()}`, {
    method: 'POST', body: upload.file, headers: { 'Content-Type': 'application/octet-stream' },
  }));
  return upload.notes.trim().length === 0 ? created : updateClientReleaseNotes(created.id, upload.notes);
}

export async function updateClientReleaseNotes(id: string, notes: string): Promise<ClientRelease> {
  requireUuid(id);
  if (!validMultilineText(notes, MAX_NOTES_LENGTH, true)) throw new ApiError(400, 'validation', 'Release notes are invalid.');
  return toClientRelease(await apiRequest<unknown>(`/Admin/ClientReleases/${id}`, { method: 'PUT', body: JSON.stringify({ Notes: notes }) }));
}

export async function markClientReleaseLatest(id: string): Promise<ClientRelease> {
  requireUuid(id);
  return toClientRelease(await apiRequest<unknown>(`/Admin/ClientReleases/${id}/Latest`, { method: 'POST' }));
}

export async function deleteClientRelease(id: string): Promise<void> {
  requireUuid(id);
  await apiRequest(`/Admin/ClientReleases/${id}`, { method: 'DELETE' });
}

export async function checkClientUpdate(platform: ClientPlatform, arch: ClientArch, currentVersion: string, signal?: AbortSignal): Promise<ClientUpdateCheck> {
  if (!isPlatform(platform) || !isArch(arch) || !isClientVersion(currentVersion)) throw new ApiError(400, 'validation', 'Client version is invalid.');
  const query = new URLSearchParams({ Platform: platform, Arch: arch, CurrentVersion: currentVersion });
  const value = await clientRequest<unknown>(`/ClientUpdates/Latest?${query.toString()}`, signal === undefined ? {} : { signal });
  if (!isRecord(value) || !exactKeys(value, ['UpdateAvailable', 'Latest']) || typeof value.UpdateAvailable !== 'boolean' || (value.UpdateAvailable && value.Latest === null)) throw invalidResponse('client update check');
  return { updateAvailable: value.UpdateAvailable, latest: value.Latest === null ? null : toClientRelease(value.Latest) };
}

export function isPlatform(value: unknown): value is ClientPlatform {
  return typeof value === 'string' && (CLIENT_PLATFORMS as readonly string[]).includes(value);
}

export function isArch(value: unknown): value is ClientArch {
  return typeof value === 'string' && (CLIENT_ARCHES as readonly string[]).includes(value);
}

function toClientRelease(value: unknown): ClientRelease {
  if (!isRecord(value) || !exactKeys(value, RELEASE_KEYS) || !validUuid(value.Id) || !isPlatform(value.Platform) || !isArch(value.Arch) || typeof value.Version !== 'string' || !isClientVersion(value.Version) || !validText(value.FileName, 200) || !isNonNegativeInteger(value.Size) || value.Size === 0 || typeof value.Sha256 !== 'string' || !/^[0-9a-f]{64}$/u.test(value.Sha256) || !validMultilineText(value.Notes, MAX_NOTES_LENGTH, true) || typeof value.IsLatest !== 'boolean' || !validDate(value.CreatedAt) || value.DownloadPath !== `/ClientUpdates/${value.Id}/Download`) {
    throw invalidResponse('client release');
  }
  return { id: value.Id, platform: value.Platform, arch: value.Arch, version: value.Version, fileName: value.FileName, size: value.Size, sha256: value.Sha256, notes: value.Notes, isLatest: value.IsLatest, createdAt: value.CreatedAt, downloadPath: value.DownloadPath };
}

function exactKeys(value: Record<string, unknown>, keys: string[]): boolean { const actual = Object.keys(value); return actual.length === keys.length && keys.every((key) => Object.hasOwn(value, key)); }
function requireUuid(value: string) { if (!validUuid(value)) throw new ApiError(400, 'validation', 'Client release ID is invalid.'); }
