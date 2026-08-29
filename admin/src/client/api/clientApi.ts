import { desktopAwareFetch, getApiBaseUrl, resolveApiUrl } from './apiBase';
import { clientIdentityHeader, clearClientToken, getClientToken } from '../auth/clientSession';

export type ClientErrorKind = 'network' | 'authentication' | 'authorization' | 'not-found' | 'validation' | 'unavailable' | 'invalid-response' | 'unexpected';

export class ClientApiError extends Error {
  constructor(public readonly status: number, public readonly kind: ClientErrorKind) {
    super(kind === 'authentication' ? 'Please sign in again.' : kind === 'authorization' ? 'You do not have access to this content.' : kind === 'not-found' ? 'This content is no longer available.' : 'The request could not be completed.');
    this.name = 'ClientApiError';
  }
}

export const CLIENT_AUTH_INVALIDATED_EVENT = 'tjxy-client-auth-invalidated';

export async function clientRequest<T>(path: string, options: RequestInit = {}): Promise<T> {
  const response = await clientFetch(path, options);
  if (!response.ok) throw new ClientApiError(response.status, errorKind(response.status));
  if (response.status === 204) return undefined as T;
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim().toLowerCase();
  if (!contentType?.includes('json')) throw new ClientApiError(response.status, 'invalid-response');
  try { return await response.json() as T; } catch { throw new ClientApiError(response.status, 'invalid-response'); }
}

export async function clientBlob(path: string, signal?: AbortSignal): Promise<Blob> {
  const response = await clientFetch(path, { signal });
  if (!response.ok) throw new ClientApiError(response.status, errorKind(response.status));
  return response.blob();
}

export async function clientFetch(path: string, options: RequestInit = {}): Promise<Response> {
  if (!path.startsWith('/') || path.startsWith('//')) throw new ClientApiError(0, 'validation');
  const headers = new Headers(options.headers);
  if (!headers.has('Accept')) headers.set('Accept', 'application/json');
  const token = getClientToken();
  if (token) headers.set('Authorization', `MediaBrowser Token="${token}"`);
  else headers.set('Authorization', clientIdentityHeader());
  if (options.body !== undefined && options.body !== null && !headers.has('Content-Type')) headers.set('Content-Type', 'application/json');
  let response: Response;
  try {
    const baseUrl = getApiBaseUrl();
    if (!baseUrl) throw new ClientApiError(0, 'validation');
    response = await desktopAwareFetch(resolveApiUrl(path, baseUrl), { ...options, credentials: 'include', headers });
  }
  catch (error) { if (error instanceof DOMException && error.name === 'AbortError') throw error; if (error instanceof ClientApiError) throw error; throw new ClientApiError(0, 'network'); }
  if (response.status === 401 && token) {
    clearClientToken();
    window.dispatchEvent(new Event(CLIENT_AUTH_INVALIDATED_EVENT));
  }
  return response;
}

function errorKind(status: number): ClientErrorKind {
  if (status === 401) return 'authentication';
  if (status === 403) return 'authorization';
  if (status === 404) return 'not-found';
  if (status === 400 || status === 422) return 'validation';
  if (status >= 500) return 'unavailable';
  return 'unexpected';
}
