import { ApiError, apiRequest } from '../api/httpClient';
import { validUuid } from '../api/responseValidation';

export type { LibraryOption, ScanProfile } from '../libraries/libraryApi';
export { listLibraries } from '../libraries/libraryApi';

export type GoogleDriveScope = 'MyDrive' | 'SharedDrive';

export interface GoogleDriveChoice {
  id: string;
  name: string;
}

export interface GoogleOAuthStart {
  state: string;
  authorizationUrl: string;
}

export interface StorageChoicePage {
  items: GoogleDriveChoice[];
  nextPageToken: string | null;
}

export interface GoogleDirectoryRequest {
  scope: GoogleDriveScope;
  sharedDriveId?: string;
  parentId?: string;
  pageToken?: string;
}

export interface OneDriveDirectoryRequest {
  parentId?: string;
  pageToken?: string;
}

export interface GoogleDriveBindingRequest {
  scope: GoogleDriveScope;
  displayName: string;
  sharedDriveId?: string;
  rootObjectId: string;
}

export interface OneDriveBindingRequest {
  displayName: string;
  rootObjectId: string;
}

export interface StorageBindingResult {
  accountId: string;
  rootId: string;
  initialSyncJobId: string;
  restartRequired: boolean;
}

export async function startGoogleDriveOAuth(
  libraryId: string,
  signal?: AbortSignal,
): Promise<GoogleOAuthStart> {
  const targetLibraryId = requireText(libraryId, 'A target library is required.');
  const value = await apiRequest<unknown>('/Admin/Storage/OAuth/GoogleDrive/Start', {
    method: 'POST',
    body: JSON.stringify({ TargetLibraryId: targetLibraryId }),
    ...signalOption(signal),
  });
  if (!isRecord(value) || !validText(value.State) || !validAuthorizationUrl(value.AuthorizationUrl)) {
    throw invalidResponse('Google authorization');
  }
  return { state: value.State, authorizationUrl: value.AuthorizationUrl };
}

export async function listSharedDrives(
  state: string,
  pageToken?: string,
  signal?: AbortSignal,
): Promise<StorageChoicePage> {
  const path = oauthPath(state, 'SharedDrives');
  const query = new URLSearchParams();
  if (pageToken !== undefined) query.set('PageToken', requireText(pageToken, 'A page token is required.'));
  const value = await apiGet<unknown>(withQuery(path, query), signal);
  if (!isRecord(value) || !Array.isArray(value.Items)) throw invalidResponse('Shared Drive list');
  if (value.NextPageToken !== null && value.NextPageToken !== undefined && !validText(value.NextPageToken)) {
    throw invalidResponse('Shared Drive pagination');
  }
  return {
    items: value.Items.map(toChoice),
    nextPageToken: typeof value.NextPageToken === 'string' ? value.NextPageToken : null,
  };
}

export async function listGoogleDirectories(
  state: string,
  request: GoogleDirectoryRequest,
  signal?: AbortSignal,
): Promise<StorageChoicePage> {
  validateScope(request.scope, request.sharedDriveId);
  const query = new URLSearchParams({ Scope: request.scope });
  if (request.sharedDriveId !== undefined) {
    query.set('SharedDriveId', requireText(request.sharedDriveId, 'A Shared Drive is required.'));
  }
  if (request.parentId !== undefined) {
    query.set('ParentId', requireText(request.parentId, 'A parent folder is required.'));
  }
  if (request.pageToken !== undefined) {
    query.set('PageToken', requireUuid(request.pageToken, 'A valid page token is required.'));
  }
  const value = await apiGet<unknown>(withQuery(oauthPath(state, 'Directories'), query), signal);
  return toDirectoryPage(value, 'directory list');
}

export async function bindGoogleDrive(
  state: string,
  request: GoogleDriveBindingRequest,
  signal?: AbortSignal,
): Promise<StorageBindingResult> {
  validateScope(request.scope, request.sharedDriveId);
  const body: Record<string, string> = {
    Scope: request.scope,
    DisplayName: requireText(request.displayName, 'A display name is required.'),
    RootObjectId: requireText(request.rootObjectId, 'A root folder is required.'),
  };
  if (request.sharedDriveId !== undefined) {
    body.SharedDriveId = requireText(request.sharedDriveId, 'A Shared Drive is required.');
  }
  const value = await apiRequest<unknown>(oauthPath(state, 'Bind'), {
    method: 'POST',
    body: JSON.stringify(body),
    ...signalOption(signal),
  });
  if (
    !isRecord(value)
    || !validText(value.AccountId)
    || !validText(value.RootId)
    || !validText(value.InitialSyncJobId)
    || typeof value.RestartRequired !== 'boolean'
  ) {
    throw invalidResponse('storage binding');
  }
  return {
    accountId: value.AccountId,
    rootId: value.RootId,
    initialSyncJobId: value.InitialSyncJobId,
    restartRequired: value.RestartRequired,
  };
}

export async function startOneDriveOAuth(
  libraryId: string,
  signal?: AbortSignal,
): Promise<GoogleOAuthStart> {
  const targetLibraryId = requireText(libraryId, 'A target library is required.');
  const value = await apiRequest<unknown>('/Admin/Storage/OAuth/OneDrive/Start', {
    method: 'POST',
    body: JSON.stringify({ TargetLibraryId: targetLibraryId }),
    ...signalOption(signal),
  });
  if (!isRecord(value) || !validText(value.State) || !validAuthorizationUrl(value.AuthorizationUrl)) {
    throw invalidResponse('Microsoft authorization');
  }
  return { state: value.State, authorizationUrl: value.AuthorizationUrl };
}

export async function listOneDriveDirectories(
  state: string,
  request: OneDriveDirectoryRequest = {},
  signal?: AbortSignal,
): Promise<StorageChoicePage> {
  const query = new URLSearchParams();
  if (request.parentId !== undefined) {
    query.set('ParentId', requireText(request.parentId, 'A parent folder is required.'));
  }
  if (request.pageToken !== undefined) {
    query.set('PageToken', requireUuid(request.pageToken, 'A valid page token is required.'));
  }
  const value = await apiGet<unknown>(withQuery(oneDriveOAuthPath(state, 'Directories'), query), signal);
  return toDirectoryPage(value, 'OneDrive directory list');
}

export async function bindOneDrive(
  state: string,
  request: OneDriveBindingRequest,
  signal?: AbortSignal,
): Promise<StorageBindingResult> {
  const value = await apiRequest<unknown>(oneDriveOAuthPath(state, 'Bind'), {
    method: 'POST',
    body: JSON.stringify({
      DisplayName: requireText(request.displayName, 'A display name is required.'),
      RootObjectId: requireText(request.rootObjectId, 'A root folder is required.'),
    }),
    ...signalOption(signal),
  });
  if (
    !isRecord(value)
    || !validText(value.AccountId)
    || !validText(value.RootId)
    || !validText(value.InitialSyncJobId)
    || typeof value.RestartRequired !== 'boolean'
  ) {
    throw invalidResponse('storage binding');
  }
  return {
    accountId: value.AccountId,
    rootId: value.RootId,
    initialSyncJobId: value.InitialSyncJobId,
    restartRequired: value.RestartRequired,
  };
}

function apiGet<T>(path: string, signal?: AbortSignal): Promise<T> {
  return signal === undefined ? apiRequest<T>(path) : apiRequest<T>(path, { signal });
}

function signalOption(signal?: AbortSignal): Partial<Pick<RequestInit, 'signal'>> {
  return signal === undefined ? {} : { signal };
}

function validateScope(scope: GoogleDriveScope, sharedDriveId?: string): void {
  if (
    (scope === 'MyDrive' && sharedDriveId !== undefined)
    || (scope === 'SharedDrive' && !validText(sharedDriveId))
  ) {
    throw new ApiError(400, 'validation', 'The Drive scope selection is invalid.');
  }
}

function oauthPath(state: string, suffix: string): string {
  return `/Admin/Storage/OAuth/GoogleDrive/${encodeURIComponent(requireText(state, 'OAuth state is required.'))}/${suffix}`;
}

function oneDriveOAuthPath(state: string, suffix: string): string {
  return `/Admin/Storage/OAuth/OneDrive/${encodeURIComponent(requireText(state, 'OAuth state is required.'))}/${suffix}`;
}

function withQuery(path: string, query: URLSearchParams): string {
  const encoded = query.toString();
  return encoded.length === 0 ? path : `${path}?${encoded}`;
}

function toChoice(value: unknown): GoogleDriveChoice {
  if (!isRecord(value) || !validText(value.Id) || !validText(value.Name)) {
    throw invalidResponse('Google Drive record');
  }
  return { id: value.Id, name: value.Name };
}

function toDirectoryPage(value: unknown, subject: string): StorageChoicePage {
  if (!isRecord(value) || !Array.isArray(value.Items)) throw invalidResponse(subject);
  const nextPageToken = value.NextPageToken;
  if (nextPageToken !== null && !validUuid(nextPageToken)) {
    throw invalidResponse(`${subject} pagination`);
  }
  return {
    items: value.Items.map(toChoice),
    nextPageToken,
  };
}

function requireText(value: string, message: string): string {
  if (!validText(value)) throw new ApiError(400, 'validation', message);
  return value.trim();
}

function requireUuid(value: string, message: string): string {
  if (!validUuid(value)) throw new ApiError(400, 'validation', message);
  return value;
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

function validAuthorizationUrl(value: unknown): value is string {
  if (!validText(value)) return false;
  try {
    const url = new URL(value);
    return url.protocol === 'https:'
      || (url.protocol === 'http:' && ['127.0.0.1', '[::1]', 'localhost'].includes(url.hostname));
  } catch {
    return false;
  }
}

function invalidResponse(subject: string): ApiError {
  return new ApiError(200, 'invalid-response', `The server returned an invalid ${subject}.`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
