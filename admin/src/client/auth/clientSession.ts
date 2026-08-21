import {
  clearSession,
  getAccessToken,
  getDeviceId,
  SESSION_DEVICE_KEY,
  SESSION_TOKEN_KEY,
  setAccessToken,
} from '../../auth/session';
import { BUILD_VERSION } from '../../api/buildVersion';

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
  const desktop = import.meta.env.VITE_TJXY_SHELL === 'desktop';
  const client = desktop ? 'TJXY Desktop' : 'TJXY Web';
  const device = desktop ? 'Desktop' : 'Browser';
  return `MediaBrowser Client="${client}", Device="${device}", DeviceId="${getClientDeviceId()}", Version="${BUILD_VERSION}"`;
}

export { SESSION_TOKEN_KEY as CLIENT_TOKEN_KEY, SESSION_DEVICE_KEY as CLIENT_DEVICE_KEY };
