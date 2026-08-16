import {
  API_BASE_CHANGED_EVENT,
  clearApiBaseUrl,
  getApiBaseUrl,
  normalizeOrigin,
  resolveApiUrl,
  setApiBaseUrl,
} from './apiBase';

beforeEach(() => {
  vi.unstubAllEnvs();
  window.localStorage.clear();
});

it('normalizes origins without a trailing slash', () => {
  expect(normalizeOrigin('http://192.168.1.10:8096/')).toBe('http://192.168.1.10:8096');
  expect(normalizeOrigin('192.168.1.10:8096')).toBe('http://192.168.1.10:8096');
});

it('rejects protocol-relative and empty paths', () => {
  expect(() => resolveApiUrl('//evil.example', 'http://127.0.0.1:8096')).toThrow('invalid path');
  expect(() => resolveApiUrl('Items', 'http://127.0.0.1:8096')).toThrow('invalid path');
});

it('ignores stored server overrides in the web application', () => {
  setApiBaseUrl('http://127.0.0.1:8096/');
  expect(getApiBaseUrl()).toBe(window.location.origin);
  expect(resolveApiUrl('/Users/Me')).toBe(`${window.location.origin}/Users/Me`);
});

it('uses the stored server origin in the desktop application', () => {
  vi.stubEnv('VITE_TJXY_SHELL', 'desktop');
  setApiBaseUrl('http://127.0.0.1:8096/');
  expect(getApiBaseUrl()).toBe('http://127.0.0.1:8096');
  expect(resolveApiUrl('/Users/Me')).toBe('http://127.0.0.1:8096/Users/Me');
});

it('falls back to the page origin when no override is stored', () => {
  expect(getApiBaseUrl()).toBe(window.location.origin);
});

it('notifies mounted providers when the server changes', () => {
  const listener = vi.fn();
  window.addEventListener(API_BASE_CHANGED_EVENT, listener);
  setApiBaseUrl('http://127.0.0.1:8096');
  clearApiBaseUrl();
  expect(listener).toHaveBeenCalledTimes(2);
  window.removeEventListener(API_BASE_CHANGED_EVENT, listener);
});
