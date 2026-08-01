import { useLayoutEffect, useState } from 'react';

export type ClientTheme = 'light' | 'dark';

const STORAGE_KEY = 'tjxy-color-theme';
const DARK_MEDIA_QUERY = '(prefers-color-scheme: dark)';

export function useClientTheme() {
  const [theme, setTheme] = useState<ClientTheme>(readInitialTheme);

  useLayoutEffect(() => {
    applyTheme(theme);
    writeStoredTheme(theme);
  }, [theme]);

  return {
    theme,
    toggleTheme: () => {
      setTheme((current) => current === 'dark' ? 'light' : 'dark');
    },
  };
}

function readInitialTheme(): ClientTheme {
  const stored = readStoredTheme();
  if (stored) return stored;
  return window.matchMedia(DARK_MEDIA_QUERY).matches ? 'dark' : 'light';
}

function readStoredTheme(): ClientTheme | undefined {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    return stored === 'dark' || stored === 'light' ? stored : undefined;
  } catch {
    return undefined;
  }
}

function writeStoredTheme(theme: ClientTheme) {
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // The selected theme still applies when browser storage is unavailable.
  }
}

function applyTheme(theme: ClientTheme) {
  const root = document.documentElement;
  root.dataset.theme = theme;
  root.classList.toggle('dark', theme === 'dark');
  root.classList.toggle('light', theme === 'light');
  root.style.colorScheme = theme;
}
