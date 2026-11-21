import { getApiBaseUrl } from '../client/api/apiBase';

const TOKEN_KEY = 'tjxy.web.token';
const TOKEN_ORIGIN_KEY = 'tjxy.web.tokenOrigin';
const DEVICE_KEY = 'tjxy.web.deviceId';

/**
 * Returns the token only while the API base is the server that issued it, so a
 * desktop client that switched servers never sends one server's credential to
 * another. A token stored before its origin was recorded belongs to the
 * current server.
 */
export function getAccessToken(): string | null {
  const token = sessionStorage.getItem(TOKEN_KEY);
  if (token === null) return null;
  const origin = sessionStorage.getItem(TOKEN_ORIGIN_KEY);
  return origin === null || origin === getApiBaseUrl() ? token : null;
}

export function setAccessToken(token: string): void {
  sessionStorage.setItem(TOKEN_KEY, token);
  sessionStorage.setItem(TOKEN_ORIGIN_KEY, getApiBaseUrl());
}

export function clearSession(): void {
  sessionStorage.removeItem(TOKEN_KEY);
  sessionStorage.removeItem(TOKEN_ORIGIN_KEY);
}

export function getDeviceId(): string {
  const existing = localStorage.getItem(DEVICE_KEY);
  if (existing !== null) {
    return existing;
  }
  const created = typeof crypto.randomUUID === 'function'
    ? crypto.randomUUID()
    : `tjxy-web-${String(Date.now())}`;
  localStorage.setItem(DEVICE_KEY, created);
  return created;
}

export { DEVICE_KEY as SESSION_DEVICE_KEY, TOKEN_KEY as SESSION_TOKEN_KEY, TOKEN_ORIGIN_KEY as SESSION_TOKEN_ORIGIN_KEY };
