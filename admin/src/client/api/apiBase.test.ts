import { getApiBaseUrl, normalizeOrigin, resolveApiUrl, setApiBaseUrl } from './apiBase';

beforeEach(() => {
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

it('joins relative API paths onto a stored origin', () => {
  setApiBaseUrl('http://127.0.0.1:8096/');
  expect(getApiBaseUrl()).toBe('http://127.0.0.1:8096');
  expect(resolveApiUrl('/Users/Me')).toBe('http://127.0.0.1:8096/Users/Me');
});

it('falls back to the page origin when no override is stored', () => {
  expect(getApiBaseUrl()).toBe(window.location.origin);
});
