const USERNAME_KEY = 'tjxy.web.savedUsername';
const PASSWORD_KEY = 'tjxy.web.savedPassword';
const REMEMBER_KEY = 'tjxy.web.rememberCredentials';

export interface SavedCredentials {
  username: string;
  remember: boolean;
}

export function loadSavedCredentials(): SavedCredentials {
  if (typeof window === 'undefined') return { username: '', remember: false };
  // Remove passwords saved by older clients. Passwords must stay in the browser's
  // transient form state and the authenticated session only.
  window.localStorage.removeItem(PASSWORD_KEY);
  const remember = window.localStorage.getItem(REMEMBER_KEY) === '1';
  return {
    remember,
    username: remember ? window.localStorage.getItem(USERNAME_KEY) ?? '' : '',
  };
}

export function saveCredentials(username: string): void {
  window.localStorage.setItem(REMEMBER_KEY, '1');
  window.localStorage.setItem(USERNAME_KEY, username);
  window.localStorage.removeItem(PASSWORD_KEY);
}

export function clearSavedCredentials(): void {
  window.localStorage.setItem(REMEMBER_KEY, '0');
  window.localStorage.removeItem(USERNAME_KEY);
  window.localStorage.removeItem(PASSWORD_KEY);
}

export function persistCredentialsPreference(remember: boolean, username: string): void {
  if (remember) saveCredentials(username);
  else clearSavedCredentials();
}
