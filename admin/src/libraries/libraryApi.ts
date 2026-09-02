import { ApiError, apiRequest } from '../api/httpClient';

export type ScanProfile = 'Full' | 'Lazy' | 'Manual';
export type LibraryCollectionType = 'mixed' | 'movies' | 'tvshows' | 'music' | 'homevideos';
export type ObjectSelectionScope = 'all_synced_objects' | 'title_layer' | 'library_roots';
export type MetadataPolicy = 'full' | 'basic' | 'none';
export type MetadataSourceMode = 'automatic_scrape' | 'local_only';
export type LocalMetadataAccessMode = 'import' | 'direct' | 'import_metadata_only' | 'import_images_only';

export function localMetadataAccessMode(importMetadata: boolean, importImages: boolean): LocalMetadataAccessMode {
  if (importMetadata && importImages) return 'import';
  if (importMetadata) return 'import_metadata_only';
  if (importImages) return 'import_images_only';
  return 'direct';
}

export function localMetadataImportOptions(mode: LocalMetadataAccessMode): { importMetadata: boolean; importImages: boolean } {
  return {
    importMetadata: mode === 'import' || mode === 'import_metadata_only',
    importImages: mode === 'import' || mode === 'import_images_only',
  };
}
export type ExpansionPolicy = 'eager' | 'on_browse' | 'manual';
export type ProbePolicy = 'eager' | 'on_playback' | 'manual';

export interface CreateLibraryRequest {
  name: string;
  collectionType: LibraryCollectionType;
  enabled: boolean;
  scanProfile: ScanProfile;
  metadataSourceMode: MetadataSourceMode;
  localMetadataAccessMode: LocalMetadataAccessMode;
  path: string;
}

export interface EffectiveLibraryPolicy {
  objectSelectionScope: ObjectSelectionScope;
  metadataPolicy: MetadataPolicy;
  expansionPolicy: ExpansionPolicy;
  probePolicy: ProbePolicy;
}

export interface UpdateLibraryPolicyRequest {
  id: string;
  enabled: boolean;
  scanProfile: ScanProfile;
  profileVersion: number;
  metadataSourceMode: MetadataSourceMode;
  localMetadataAccessMode: LocalMetadataAccessMode;
  effectivePolicy?: EffectiveLibraryPolicy;
}

export interface LibraryOption {
  id: string;
  name: string;
  collectionType: string;
  locations: string[];
  unavailableLocations?: string[];
  enabled: boolean;
  scanProfile: ScanProfile;
  profileVersion: number;
  objectSelectionScope: ObjectSelectionScope;
  metadataPolicy: MetadataPolicy;
  metadataSourceMode: MetadataSourceMode;
  localMetadataAccessMode: LocalMetadataAccessMode;
  expansionPolicy: ExpansionPolicy;
  probePolicy: ProbePolicy;
}

export async function listLibraries(signal?: AbortSignal): Promise<LibraryOption[]> {
  const value = await apiRequest<unknown>(
    '/Library/VirtualFolders',
    signal === undefined ? {} : { signal },
  );
  if (!Array.isArray(value)) throw invalidResponse('library list');
  return value.map(toLibrary);
}

export async function createLibrary(request: CreateLibraryRequest): Promise<void> {
  const query = new URLSearchParams({
    name: requireText(request.name, 'A library name is required.'),
    collectionType: request.collectionType,
    refreshLibrary: 'false',
  });
  const location = { Path: requireText(request.path, 'A media path is required.') };
  await apiRequest(`/Library/VirtualFolders?${query.toString()}`, {
    method: 'POST',
    body: JSON.stringify({
      LibraryOptions: {
        Enabled: request.enabled,
        ScanProfile: request.scanProfile,
        MetadataSourceMode: request.metadataSourceMode,
        LocalMetadataAccessMode: request.localMetadataAccessMode,
      },
      ...location,
    }),
  });
}

export async function renameLibrary(currentName: string, newName: string): Promise<void> {
  const query = new URLSearchParams({
    name: requireText(currentName, 'The current library name is required.'),
    newName: requireText(newName, 'A new library name is required.'),
    refreshLibrary: 'false',
  });
  await apiRequest(`/Library/VirtualFolders/Name?${query.toString()}`, { method: 'POST' });
}

export async function updateLibraryPolicy(request: UpdateLibraryPolicyRequest): Promise<void> {
  if (!isPositiveVersion(request.profileVersion)) {
    throw new ApiError(400, 'validation', 'The current profile version is required.');
  }
  const options: Record<string, boolean | number | string> = {
    Enabled: request.enabled,
    ScanProfile: request.scanProfile,
    ProfileVersion: request.profileVersion,
    MetadataSourceMode: request.metadataSourceMode,
    LocalMetadataAccessMode: request.localMetadataAccessMode,
  };
  if (request.effectivePolicy !== undefined) {
    options.ObjectSelectionScope = request.effectivePolicy.objectSelectionScope;
    options.MetadataPolicy = request.effectivePolicy.metadataPolicy;
    options.ExpansionPolicy = request.effectivePolicy.expansionPolicy;
    options.ProbePolicy = request.effectivePolicy.probePolicy;
  }
  await apiRequest('/Library/VirtualFolders/LibraryOptions', {
    method: 'POST',
    body: JSON.stringify({
      Id: requireText(request.id, 'A library identifier is required.'),
      LibraryOptions: options,
    }),
  });
}

export async function deleteLibrary(name: string): Promise<void> {
  const query = new URLSearchParams({
    name: requireText(name, 'A library name is required.'),
    refreshLibrary: 'false',
  });
  await apiRequest(`/Library/VirtualFolders?${query.toString()}`, { method: 'DELETE' });
}

function toLibrary(value: unknown): LibraryOption {
  const metadataSourceMode = isRecord(value)
    && isRecord(value.LibraryOptions)
    && value.LibraryOptions.MetadataSourceMode === undefined
    ? 'automatic_scrape'
    : isRecord(value)
      && isRecord(value.LibraryOptions)
      && isMetadataSourceMode(value.LibraryOptions.MetadataSourceMode)
      ? value.LibraryOptions.MetadataSourceMode
      : null;
  const localMetadataAccessMode = isRecord(value)
    && isRecord(value.LibraryOptions)
    && value.LibraryOptions.LocalMetadataAccessMode === undefined
    ? 'import'
    : isRecord(value)
      && isRecord(value.LibraryOptions)
      && isLocalMetadataAccessMode(value.LibraryOptions.LocalMetadataAccessMode)
      ? value.LibraryOptions.LocalMetadataAccessMode
      : null;
  if (
    !isRecord(value)
    || !validText(value.ItemId)
    || !validText(value.Name)
    || !validText(value.CollectionType)
    || !validLocations(value.Locations)
    || !isRecord(value.LibraryOptions)
    || typeof value.LibraryOptions.Enabled !== 'boolean'
    || !isScanProfile(value.LibraryOptions.ScanProfile)
    || !isPositiveVersion(value.LibraryOptions.ProfileVersion)
    || !isObjectSelectionScope(value.LibraryOptions.ObjectSelectionScope)
    || !isMetadataPolicy(value.LibraryOptions.MetadataPolicy)
    || metadataSourceMode === null
    || localMetadataAccessMode === null
    || !isExpansionPolicy(value.LibraryOptions.ExpansionPolicy)
    || !isProbePolicy(value.LibraryOptions.ProbePolicy)
  ) {
    throw invalidResponse('library');
  }
  return {
    id: value.ItemId,
    name: value.Name,
    collectionType: value.CollectionType,
    locations: value.Locations,
    unavailableLocations: optionalStringArray(value.UnavailableLocations),
    enabled: value.LibraryOptions.Enabled,
    scanProfile: value.LibraryOptions.ScanProfile,
    profileVersion: value.LibraryOptions.ProfileVersion,
    objectSelectionScope: value.LibraryOptions.ObjectSelectionScope,
    metadataPolicy: value.LibraryOptions.MetadataPolicy,
    metadataSourceMode,
    localMetadataAccessMode,
    expansionPolicy: value.LibraryOptions.ExpansionPolicy,
    probePolicy: value.LibraryOptions.ProbePolicy,
  };
}

function optionalStringArray(value: unknown): string[] {
  if (value === undefined) return [];
  if (!validLocations(value)) throw invalidResponse('library storage status');
  return value;
}

function isScanProfile(value: unknown): value is ScanProfile {
  return value === 'Full' || value === 'Lazy' || value === 'Manual';
}

function isPositiveVersion(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 1;
}

function isObjectSelectionScope(value: unknown): value is ObjectSelectionScope {
  return value === 'all_synced_objects' || value === 'title_layer' || value === 'library_roots';
}

function isMetadataPolicy(value: unknown): value is MetadataPolicy {
  return value === 'full' || value === 'basic' || value === 'none';
}

function isMetadataSourceMode(value: unknown): value is MetadataSourceMode {
  return value === 'automatic_scrape' || value === 'local_only';
}

function isLocalMetadataAccessMode(value: unknown): value is LocalMetadataAccessMode {
  return value === 'import'
    || value === 'direct'
    || value === 'import_metadata_only'
    || value === 'import_images_only';
}

function isExpansionPolicy(value: unknown): value is ExpansionPolicy {
  return value === 'eager' || value === 'on_browse' || value === 'manual';
}

function isProbePolicy(value: unknown): value is ProbePolicy {
  return value === 'eager' || value === 'on_playback' || value === 'manual';
}

function validLocations(value: unknown): value is string[] {
  return Array.isArray(value)
    && value.every((location) => validText(location) && location.startsWith('tjxy://storage-root/'));
}

function validText(value: unknown): value is string {
  return typeof value === 'string'
    && value.trim().length > 0
    && value.length <= 16_384
    && !Array.from(value).some((character) => {
      const codePoint = character.codePointAt(0) ?? 0;
      return codePoint < 0x20 || codePoint === 0x7f;
    });
}

function requireText(value: string, message: string): string {
  if (!validText(value)) throw new ApiError(400, 'validation', message);
  return value.trim();
}

function invalidResponse(subject: string): ApiError {
  return new ApiError(200, 'invalid-response', `The server returned an invalid ${subject}.`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
