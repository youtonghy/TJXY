import { useLayoutEffect, useState } from 'react';

export type ClientColorMode = 'light' | 'dark';
export type ClientTheme = ClientColorMode;

const STORAGE_KEY = 'tjxy-color-theme';
const DARK_MEDIA_QUERY = '(prefers-color-scheme: dark)';

export function useClientTheme() {
  const { colorMode, toggleColorMode } = useClientColorMode();

  useLayoutEffect(() => {
    const restore = applyColorMode(colorMode);
    return restore;
  }, [colorMode]);

  return {
    theme: colorMode,
    toggleTheme: toggleColorMode,
  };
}

export function useClientColorMode() {
  const [colorMode, setColorMode] = useState<ClientColorMode>(readInitialTheme);
  return {
    colorMode,
    toggleColorMode: () => {
      setColorMode((current) => {
        const next = current === 'dark' ? 'light' : 'dark';
        writeStoredTheme(next);
        return next;
      });
    },
  };
}

function readInitialTheme(): ClientColorMode {
  const stored = readStoredTheme();
  if (stored) return stored;
  return window.matchMedia(DARK_MEDIA_QUERY).matches ? 'dark' : 'light';
}

function readStoredTheme(): ClientColorMode | undefined {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    return stored === 'dark' || stored === 'light' ? stored : undefined;
  } catch {
    return undefined;
  }
}

function writeStoredTheme(theme: ClientColorMode) {
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // The selected theme still applies when browser storage is unavailable.
  }
}

function applyColorMode(theme: ClientColorMode) {
  const root = document.documentElement;
  const previousTheme = root.dataset.theme;
  const previousDark = root.classList.contains('dark');
  const previousLight = root.classList.contains('light');
  const previousColorScheme = root.style.colorScheme;
  root.dataset.theme = theme;
  root.classList.toggle('dark', theme === 'dark');
  root.classList.toggle('light', theme === 'light');
  root.style.colorScheme = theme;
  return () => {
    if (previousTheme === undefined) delete root.dataset.theme;
    else root.dataset.theme = previousTheme;
    root.classList.toggle('dark', previousDark);
    root.classList.toggle('light', previousLight);
    root.style.colorScheme = previousColorScheme;
  };
}
