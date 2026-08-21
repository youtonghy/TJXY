import { getAccessToken, getDeviceId } from '../auth/session';
import { desktopAwareFetch, getApiBaseUrl, isDesktopShell, resolveApiUrl } from '../client/api/apiBase';
import { BUILD_VERSION } from './buildVersion';

export type ApiErrorCategory =
  | 'network'
  | 'invalid-response'
  | 'validation'
  | 'authentication'
  | 'authorization'
  | 'not-found'
  | 'conflict'
  | 'unavailable'
  | 'unexpected';

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly category: ApiErrorCategory,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

export type RequestAuth = 'none' | 'identity' | 'token';

export interface ApiRequestOptions extends RequestInit {
  auth?: RequestAuth;
}

const ERROR_MESSAGES: Record<ApiErrorCategory, string> = {
  network: 'The server could not be reached.',
  'invalid-response': 'The server returned an invalid response.',
  validation: 'The request was not valid.',
  authentication: 'Your session is not valid.',
  authorization: 'You do not have permission to perform this action.',
  'not-found': 'The requested record was not found.',
  conflict: 'The request conflicts with the current server state.',
  unavailable: 'The server is temporarily unavailable.',
  unexpected: 'The server could not complete the request.',
};

export function mediaBrowserIdentityHeader(): string {
  return `MediaBrowser Client="TJXY Admin", Device="Browser", DeviceId="${getDeviceId()}", Version="${BUILD_VERSION}"`;
}

export async function apiRequest<T = undefined>(
  path: string,
  options: ApiRequestOptions = {},
): Promise<T> {
  validatePath(path);
  const { auth = 'token', headers: suppliedHeaders, ...requestOptions } = options;
  const headers = new Headers(suppliedHeaders);
  if (
    requestOptions.body !== undefined
    && requestOptions.body !== null
    && !headers.has('Content-Type')
  ) {
    headers.set('Content-Type', 'application/json');
  }
  if (auth === 'identity') {
    headers.set('Authorization', mediaBrowserIdentityHeader());
  } else if (auth === 'token') {
    headers.set('Authorization', mediaBrowserTokenHeader());
  }

  let response: Response;
  try {
    const origin = getApiBaseUrl() || (isDesktopShell() ? '' : window.location.origin);
    if (!origin) throw apiError(0, 'network');
    response = await desktopAwareFetch(resolveApiUrl(path, origin), {
      ...requestOptions,
      headers,
    });
  } catch (error) {
    if (error instanceof ApiError) throw error;
    throw apiError(0, 'network');
  }

  if (!response.ok) {
    throw apiError(response.status, categoryForStatus(response.status));
  }
  if (response.status === 204 || response.status === 205) {
    return undefined as T;
  }

  const contentType = response.headers.get('Content-Type')?.split(';', 1)[0]?.trim() ?? '';
  if (!isJsonContentType(contentType)) {
    throw apiError(response.status, 'invalid-response');
  }
  try {
    return await response.json() as T;
  } catch {
    throw apiError(response.status, 'invalid-response');
  }
}

function validatePath(path: string): void {
  if (!path.startsWith('/') || path.startsWith('//')) {
    throw apiError(0, 'validation');
  }
}

export function mediaBrowserTokenHeader(): string {
  const token = getAccessToken();
  if (
    token === null
    || token.length === 0
    || token.length > 1_024
    || Array.from(token).some((character) => {
      const codePoint = character.codePointAt(0) ?? 0;
      return codePoint <= 0x20 || codePoint === 0x7f || character === '"' || character === '\\';
    })
  ) {
    throw apiError(401, 'authentication');
  }
  return `MediaBrowser Token="${token}"`;
}

function isJsonContentType(contentType: string): boolean {
  return contentType.toLowerCase() === 'application/json'
    || /^application\/[a-z0-9!#$&^_.+-]+\+json$/iu.test(contentType);
}

function categoryForStatus(status: number): ApiErrorCategory {
  switch (status) {
    case 400:
    case 422:
      return 'validation';
    case 401:
      return 'authentication';
    case 403:
      return 'authorization';
    case 404:
      return 'not-found';
    case 409:
      return 'conflict';
    case 502:
    case 503:
    case 504:
      return 'unavailable';
    default:
      return 'unexpected';
  }
}

function apiError(status: number, category: ApiErrorCategory): ApiError {
  return new ApiError(status, category, ERROR_MESSAGES[category]);
}
