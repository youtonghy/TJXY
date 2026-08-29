import { clearSavedCredentials, loadSavedCredentials, persistRememberPreference } from './savedCredentials';

beforeEach(() => {
  window.localStorage.clear();
});

it('removes credentials saved by older clients', () => {
  window.localStorage.setItem('tjxy.web.rememberCredentials', '1');
  window.localStorage.setItem('tjxy.web.savedUsername', 'alice');
  window.localStorage.setItem('tjxy.web.savedPassword', 'legacy-secret');
  expect(loadSavedCredentials()).toEqual({ remember: true, username: '' });
  expect(window.localStorage.getItem('tjxy.web.savedUsername')).toBeNull();
  expect(window.localStorage.getItem('tjxy.web.savedPassword')).toBeNull();
});

it('clears stored credentials when remember is turned off', () => {
  persistRememberPreference(true);
  persistRememberPreference(false);
  expect(loadSavedCredentials()).toEqual({ remember: false, username: '' });
  expect(window.localStorage.getItem('tjxy.web.savedPassword')).toBeNull();
});

it('defaults to not remembering credentials', () => {
  expect(loadSavedCredentials().remember).toBe(false);
  clearSavedCredentials();
  expect(loadSavedCredentials().remember).toBe(false);
});

it('persists only the remember-login preference', () => {
  persistRememberPreference(true);
  expect(loadSavedCredentials().remember).toBe(true);
  expect(loadSavedCredentials().username).toBe('');
  expect(window.localStorage.getItem('tjxy.web.savedPassword')).toBeNull();
});
