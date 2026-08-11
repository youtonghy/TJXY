/* eslint-disable react-refresh/only-export-components */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

import { getPublicSystemSettings, type SystemSettings } from './systemSettingsApi';
import type { SystemLocale } from './systemLanguageApi';

interface SystemLocaleContextValue {
  locale: SystemLocale;
  isLoading: boolean;
  siteTitle: string;
  siteSubtitle: string;
  logoUrl: string;
  iconUrl: string;
  setLocale: (locale: SystemLocale) => void;
}

const fallback: SystemLocaleContextValue = {
  locale: 'en-US',
  isLoading: true,
  siteTitle: 'TJXY',
  siteSubtitle: 'Your media library',
  logoUrl: '/brand/tjxy-mark.webp',
  iconUrl: '/brand/favicon.svg',
  setLocale: () => undefined,
};
const SystemLocaleContext = createContext<SystemLocaleContextValue>(fallback);

export function SystemLocaleProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<SystemSettings>(() => ({
    locale: window.localStorage.getItem('tjxy-system-locale') === 'en-US' ? 'en-US' : 'zh-CN',
    siteTitle: 'TJXY',
    siteSubtitle: 'Your media library',
    logoUrl: '/brand/tjxy-mark.webp',
    iconUrl: '/brand/favicon.svg',
    publicUrl: '',
    listenHost: '127.0.0.1',
    port: 8096,
    mediaBrowserRoots: [],
    invalidMediaBrowserRootIndexes: [],
    revision: 0,
    restartRequired: false,
    environmentOverrides: {
      siteTitle: false,
      publicUrl: false,
      listenAddress: false,
      mediaBrowserRoots: false,
    },
  }));
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    document.documentElement.lang = settings.locale;
    document.title = settings.siteTitle;
    for (const icon of document.querySelectorAll<HTMLLinkElement>('link[rel~="icon"]')) {
      icon.href = settings.iconUrl;
    }
  }, [settings.iconUrl, settings.locale, settings.siteTitle]);

  useEffect(() => {
    const controller = new AbortController();
    void getPublicSystemSettings(controller.signal)
      .then((value) => {
        setSettings(value);
        window.localStorage.setItem('tjxy-system-locale', value.locale);
      })
      .catch(() => undefined)
      .finally(() => { if (!controller.signal.aborted) setIsLoading(false); });
    return () => { controller.abort(); };
  }, []);

  useEffect(() => {
    const update = (event: Event) => {
      const detail = (event as CustomEvent<SystemSettings>).detail;
      setSettings(detail);
    };
    window.addEventListener('tjxy-system-settings', update);
    return () => { window.removeEventListener('tjxy-system-settings', update); };
  }, []);

  const selectLocale = useCallback((locale: SystemLocale) => {
    setSettings((current) => ({ ...current, locale }));
    window.localStorage.setItem('tjxy-system-locale', locale);
  }, []);
  const value = useMemo(() => ({
    locale: settings.locale,
    isLoading,
    siteTitle: settings.siteTitle,
    siteSubtitle: settings.siteSubtitle,
    logoUrl: settings.logoUrl,
    iconUrl: settings.iconUrl,
    setLocale: selectLocale,
  }), [isLoading, selectLocale, settings]);
  return <SystemLocaleContext.Provider value={value}>{children}</SystemLocaleContext.Provider>;
}

export function useSystemLocale(): SystemLocaleContextValue { return useContext(SystemLocaleContext); }
