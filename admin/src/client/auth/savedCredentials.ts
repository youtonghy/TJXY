const USERNAME_KEY = 'tjxy.web.savedUsername';
const PASSWORD_KEY = 'tjxy.web.savedPassword';
const REMEMBER_KEY = 'tjxy.web.rememberCredentials';

export interface SavedCredentials {
  username: string;
  remember: boolean;
}

export function loadSavedCredentials(): SavedCredentials {
  if (typeof window === 'undefined') return { username: '', remember: false };
  // Remove credentials saved by older clients. Remember-login persists only the
  // server-issued HttpOnly session cookie.
  window.localStorage.removeItem(USERNAME_KEY);
  window.localStorage.removeItem(PASSWORD_KEY);
  const remember = window.localStorage.getItem(REMEMBER_KEY) === '1';
  return { remember, username: '' };
}

export function clearSavedCredentials(): void {
  window.localStorage.setItem(REMEMBER_KEY, '0');
  window.localStorage.removeItem(USERNAME_KEY);
  window.localStorage.removeItem(PASSWORD_KEY);
}

export function persistRememberPreference(remember: boolean): void {
  if (remember) {
    window.localStorage.setItem(REMEMBER_KEY, '1');
    window.localStorage.removeItem(USERNAME_KEY);
    window.localStorage.removeItem(PASSWORD_KEY);
  }
  else clearSavedCredentials();
}
