import {
  clearSession,
  getAccessToken,
  getDeviceId,
  SESSION_DEVICE_KEY,
  SESSION_TOKEN_KEY,
  setAccessToken,
} from '../../auth/session';

export function getClientToken(): string | null {
  return getAccessToken();
}

export function setClientToken(token: string): void {
  setAccessToken(token);
}

export function clearClientToken(): void {
  clearSession();
}

export function getClientDeviceId(): string {
  return getDeviceId();
}

export function clientIdentityHeader(): string {
  return `MediaBrowser Client="TJXY Web", Device="Browser", DeviceId="${getClientDeviceId()}", Version="0.1.0"`;
}

export { SESSION_TOKEN_KEY as CLIENT_TOKEN_KEY, SESSION_DEVICE_KEY as CLIENT_DEVICE_KEY };
