const TOKEN_KEY = 'tjxy.admin.token';
const DEVICE_KEY = 'tjxy.admin.deviceId';

export function getAccessToken(): string | null {
  return sessionStorage.getItem(TOKEN_KEY);
}

export function setAccessToken(token: string): void {
  sessionStorage.setItem(TOKEN_KEY, token);
}

export function clearSession(): void {
  sessionStorage.removeItem(TOKEN_KEY);
  sessionStorage.removeItem(DEVICE_KEY);
}

export function getDeviceId(): string {
  const existing = sessionStorage.getItem(DEVICE_KEY);
  if (existing !== null) {
    return existing;
  }
  const created = crypto.randomUUID();
  sessionStorage.setItem(DEVICE_KEY, created);
  return created;
}
