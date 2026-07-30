const TOKEN_KEY = 'tjxy.web.token';
const DEVICE_KEY = 'tjxy.web.deviceId';

export function getClientToken(): string | null {
  return window.sessionStorage.getItem(TOKEN_KEY);
}

export function setClientToken(token: string): void {
  window.sessionStorage.setItem(TOKEN_KEY, token);
}

export function clearClientToken(): void {
  window.sessionStorage.removeItem(TOKEN_KEY);
}

export function getClientDeviceId(): string {
  const existing = window.localStorage.getItem(DEVICE_KEY);
  if (existing) return existing;
  const id = typeof crypto.randomUUID === 'function' ? crypto.randomUUID() : `tjxy-web-${String(Date.now())}`;
  window.localStorage.setItem(DEVICE_KEY, id);
  return id;
}

export function clientIdentityHeader(): string {
  return `MediaBrowser Client="TJXY Web", Device="Browser", DeviceId="${getClientDeviceId()}", Version="0.1.0"`;
}

export { TOKEN_KEY as CLIENT_TOKEN_KEY, DEVICE_KEY as CLIENT_DEVICE_KEY };
