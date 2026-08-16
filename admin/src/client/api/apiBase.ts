export const API_BASE_STORAGE_KEY = 'tjxy.api.baseUrl';
export const API_BASE_CHANGED_EVENT = 'tjxy-api-base-changed';

export function isDesktopShell(): boolean {
  return import.meta.env.VITE_TJXY_SHELL === 'desktop';
}

export function normalizeOrigin(value: string): string {
  const trimmed = value.trim().replace(/\/+$/, '');
  if (!trimmed) throw new Error('empty origin');
  const url = new URL(trimmed.includes('://') ? trimmed : `http://${trimmed}`);
  if (url.protocol !== 'http:' && url.protocol !== 'https:') throw new Error('invalid origin');
  return `${url.protocol}//${url.host}`;
}

export function getStoredApiBaseUrl(): string | null {
  if (typeof window === 'undefined') return null;
  const stored = window.localStorage.getItem(API_BASE_STORAGE_KEY);
  return stored && stored.length > 0 ? stored : null;
}

export function getApiBaseUrl(): string {
  if (!isDesktopShell()) {
    return typeof window !== 'undefined' ? window.location.origin : '';
  }
  return getStoredApiBaseUrl() ?? '';
}

export function setApiBaseUrl(origin: string): string {
  const normalized = normalizeOrigin(origin);
  const previous = window.localStorage.getItem(API_BASE_STORAGE_KEY);
  window.localStorage.setItem(API_BASE_STORAGE_KEY, normalized);
  if (previous !== normalized) {
    window.dispatchEvent(new CustomEvent(API_BASE_CHANGED_EVENT, { detail: normalized }));
  }
  return normalized;
}

export function clearApiBaseUrl(): void {
  const hadStoredOrigin = window.localStorage.getItem(API_BASE_STORAGE_KEY) !== null;
  window.localStorage.removeItem(API_BASE_STORAGE_KEY);
  if (hadStoredOrigin) window.dispatchEvent(new CustomEvent(API_BASE_CHANGED_EVENT));
}

export function resolveApiUrl(path: string, baseUrl = getApiBaseUrl()): string {
  if (!path.startsWith('/') || path.startsWith('//')) throw new Error('invalid path');
  if (!baseUrl) throw new Error('missing api base url');
  return new URL(path, `${baseUrl}/`).toString();
}

export function resolvePublicAssetUrl(value: string): string {
  if (!isDesktopShell() || !value.startsWith('/') || value.startsWith('//')) return value;
  const baseUrl = getStoredApiBaseUrl();
  return baseUrl ? new URL(value, `${baseUrl}/`).toString() : value;
}

export async function probeServer(origin: string, signal?: AbortSignal): Promise<string> {
  const base = normalizeOrigin(origin);
  const candidates = ['/System/Info/Public', '/health/ready', '/health/live'];
  let lastError: unknown;
  for (const path of candidates) {
    try {
      const response = await desktopAwareFetch(new URL(path, `${base}/`).toString(), { method: 'GET', signal });
      if (response.ok) return base;
      lastError = new Error(String(response.status));
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError instanceof Error ? lastError : new Error('unreachable');
}

export async function desktopAwareFetch(url: string, options: RequestInit = {}): Promise<Response> {
  if (!isDesktopShell()) return fetch(url, options);
  const { fetch: tauriFetch } = await import('@tauri-apps/plugin-http');
  const headers = serializeHeaders(options.headers);
  return tauriFetch(url, { ...options, headers });
}

function serializeHeaders(headers: HeadersInit | undefined): Record<string, string> | undefined {
  if (!headers) return undefined;
  if (headers instanceof Headers) return Object.fromEntries(headers.entries());
  if (Array.isArray(headers)) return Object.fromEntries(headers);
  return { ...headers };
}
