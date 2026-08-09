const TOKEN_KEY = 'tjxy.web.token';
const DEVICE_KEY = 'tjxy.web.deviceId';

export function getAccessToken(): string | null {
  return sessionStorage.getItem(TOKEN_KEY);
}

export function setAccessToken(token: string): void {
  sessionStorage.setItem(TOKEN_KEY, token);
}

export function clearSession(): void {
  sessionStorage.removeItem(TOKEN_KEY);
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

export { DEVICE_KEY as SESSION_DEVICE_KEY, TOKEN_KEY as SESSION_TOKEN_KEY };
