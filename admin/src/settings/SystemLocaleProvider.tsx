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

import {
  getPublicSystemSettings,
  type PublicSiteThemeSettings,
  type SystemSettings,
} from './systemSettingsApi';
import type { SystemLocale } from './systemLanguageApi';

interface SystemLocaleContextValue {
  locale: SystemLocale;
  isLoading: boolean;
  siteTitle: string;
  siteSubtitle: string;
  logoUrl: string;
  iconUrl: string;
  theme: PublicSiteThemeSettings;
  settingsLoadFailed: boolean;
  setLocale: (locale: SystemLocale) => void;
}

const fallback: SystemLocaleContextValue = {
  locale: 'en-US',
  isLoading: true,
  siteTitle: 'TJXY',
  siteSubtitle: 'Your media library',
  logoUrl: '/brand/tjxy-mark.webp',
  iconUrl: '/brand/favicon.svg',
  theme: { id: 'classic', schemaVersion: 1, options: {}, revision: 0 },
  settingsLoadFailed: false,
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
    theme: { id: 'classic', schemaVersion: 1, options: {}, revision: 0 },
  }));
  const [isLoading, setIsLoading] = useState(true);
  const [settingsLoadFailed, setSettingsLoadFailed] = useState(false);

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
        setSettingsLoadFailed(false);
        window.localStorage.setItem('tjxy-system-locale', value.locale);
      })
      .catch(() => { if (!controller.signal.aborted) setSettingsLoadFailed(true); })
      .finally(() => { if (!controller.signal.aborted) setIsLoading(false); });
    return () => { controller.abort(); };
  }, []);

  useEffect(() => {
    const update = (event: Event) => {
      const detail = (event as CustomEvent<SystemSettings>).detail;
      setSettings((current) => ({
        ...current,
        locale: detail.locale,
        siteTitle: detail.siteTitle,
        siteSubtitle: detail.siteSubtitle,
        logoUrl: detail.logoUrl,
        iconUrl: detail.iconUrl,
      }));
    };
    window.addEventListener('tjxy-system-settings', update);
    return () => { window.removeEventListener('tjxy-system-settings', update); };
  }, []);

  useEffect(() => {
    const update = (event: Event) => {
      const theme = (event as CustomEvent<PublicSiteThemeSettings>).detail;
      setSettings((current) => ({ ...current, theme }));
    };
    window.addEventListener('tjxy-site-theme', update);
    return () => { window.removeEventListener('tjxy-site-theme', update); };
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
    theme: settings.theme,
    settingsLoadFailed,
    setLocale: selectLocale,
  }), [isLoading, selectLocale, settings, settingsLoadFailed]);
  return <SystemLocaleContext.Provider value={value}>{children}</SystemLocaleContext.Provider>;
}

export function useSystemLocale(): SystemLocaleContextValue { return useContext(SystemLocaleContext); }
