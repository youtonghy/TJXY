import { setApiBaseUrl } from '../api/apiBase';
import { CLIENT_TOKEN_KEY, clearClientToken, getClientToken, setClientToken } from './clientSession';

afterEach(() => {
  localStorage.clear();
  vi.unstubAllEnvs();
});

it('uses the shared browser credential consumed by the administrator session', () => {
  setClientToken('client-token');
  expect(getClientToken()).toBe('client-token');
  expect(sessionStorage.getItem(CLIENT_TOKEN_KEY)).toBe('client-token');
  clearClientToken();
  expect(sessionStorage.getItem('tjxy.web.token')).toBeNull();
});

it('keeps a desktop token to the server that issued it', () => {
  vi.stubEnv('VITE_TJXY_SHELL', 'desktop');
  setApiBaseUrl('http://old.example:8096');
  setClientToken('old-server-token');

  setApiBaseUrl('http://new.example:8096');
  expect(getClientToken()).toBeNull();

  setApiBaseUrl('http://old.example:8096');
  expect(getClientToken()).toBe('old-server-token');
});

it('treats a token stored before origins were recorded as the current server\'s', () => {
  vi.stubEnv('VITE_TJXY_SHELL', 'desktop');
  setApiBaseUrl('http://current.example:8096');
  sessionStorage.setItem(CLIENT_TOKEN_KEY, 'legacy-token');
  expect(getClientToken()).toBe('legacy-token');
});
