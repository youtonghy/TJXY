import { clearSavedCredentials, loadSavedCredentials, persistCredentialsPreference, saveCredentials } from './savedCredentials';

beforeEach(() => {
  window.localStorage.clear();
});

it('remembers only the username when enabled', () => {
  window.localStorage.setItem('tjxy.web.savedPassword', 'legacy-secret');
  saveCredentials('alice');
  expect(loadSavedCredentials()).toEqual({ remember: true, username: 'alice' });
  expect(window.localStorage.getItem('tjxy.web.savedPassword')).toBeNull();
});

it('clears stored credentials when remember is turned off', () => {
  saveCredentials('alice');
  persistCredentialsPreference(false, 'alice');
  expect(loadSavedCredentials()).toEqual({ remember: false, username: '' });
  expect(window.localStorage.getItem('tjxy.web.savedPassword')).toBeNull();
});

it('defaults to not remembering credentials', () => {
  expect(loadSavedCredentials().remember).toBe(false);
  clearSavedCredentials();
  expect(loadSavedCredentials().remember).toBe(false);
});
