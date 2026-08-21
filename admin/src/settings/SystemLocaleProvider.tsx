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
import {
  API_BASE_CHANGED_EVENT,
  getStoredApiBaseUrl,
  isDesktopShell,
  resolvePublicAssetUrl,
} from '../client/api/apiBase';

const SYSTEM_LOCALE_KEY = 'tjxy-system-locale';
const DEVICE_LOCALE_KEY = 'tjxy-device-locale';

function storedLocale(key: string): SystemLocale | undefined {
  const value = window.localStorage.getItem(key);
  return value === 'en-US' || value === 'zh-CN' ? value : undefined;
}

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
    locale: storedLocale(DEVICE_LOCALE_KEY) ?? storedLocale(SYSTEM_LOCALE_KEY) ?? 'zh-CN',
    siteTitle: 'TJXY',
    siteSubtitle: 'Your media library',
    logoUrl: '/brand/tjxy-mark.webp',
    iconUrl: '/brand/favicon.svg',
    publicUrl: '',
    listenHost: '127.0.0.1',
    port: 8096,
    revision: 0,
    restartRequired: false,
    environmentOverrides: {
      siteTitle: false,
      publicUrl: false,
      listenAddress: false,
    },
    theme: { id: 'classic', schemaVersion: 1, options: {}, revision: 0 },
  }));
  const [isLoading, setIsLoading] = useState(true);
  const [settingsLoadFailed, setSettingsLoadFailed] = useState(false);
  const logoUrl = resolvePublicAssetUrl(settings.logoUrl);
  const iconUrl = resolvePublicAssetUrl(settings.iconUrl);

  useEffect(() => {
    document.documentElement.lang = settings.locale;
    document.title = settings.siteTitle;
    for (const icon of document.querySelectorAll<HTMLLinkElement>('link[rel~="icon"]')) {
      icon.href = iconUrl;
    }
  }, [iconUrl, settings.locale, settings.siteTitle]);

  useEffect(() => {
    let controller: AbortController | undefined;
    const loadSettings = () => {
      controller?.abort();
      if (isDesktopShell() && !getStoredApiBaseUrl()) {
        setSettingsLoadFailed(false);
        setIsLoading(false);
        return;
      }
      controller = new AbortController();
      const request = controller;
      setIsLoading(true);
      void getPublicSystemSettings(request.signal)
        .then((value) => {
          const locale = storedLocale(DEVICE_LOCALE_KEY) ?? value.locale;
          setSettings({ ...value, locale });
          setSettingsLoadFailed(false);
          window.localStorage.setItem(SYSTEM_LOCALE_KEY, locale);
        })
        .catch(() => { if (!request.signal.aborted) setSettingsLoadFailed(true); })
        .finally(() => { if (!request.signal.aborted) setIsLoading(false); });
    };
    loadSettings();
    window.addEventListener(API_BASE_CHANGED_EVENT, loadSettings);
    return () => {
      controller?.abort();
      window.removeEventListener(API_BASE_CHANGED_EVENT, loadSettings);
    };
  }, []);

  useEffect(() => {
    const update = (event: Event) => {
      const detail = (event as CustomEvent<SystemSettings>).detail;
      setSettings((current) => ({
        ...current,
        locale: storedLocale(DEVICE_LOCALE_KEY) ?? detail.locale,
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
    window.localStorage.setItem(DEVICE_LOCALE_KEY, locale);
    window.localStorage.setItem(SYSTEM_LOCALE_KEY, locale);
  }, []);
  const value = useMemo(() => ({
    locale: settings.locale,
    isLoading,
    siteTitle: settings.siteTitle,
    siteSubtitle: settings.siteSubtitle,
    logoUrl,
    iconUrl,
    theme: settings.theme,
    settingsLoadFailed,
    setLocale: selectLocale,
  }), [iconUrl, isLoading, logoUrl, selectLocale, settings, settingsLoadFailed]);
  return <SystemLocaleContext.Provider value={value}>{children}</SystemLocaleContext.Provider>;
}

export function useSystemLocale(): SystemLocaleContextValue { return useContext(SystemLocaleContext); }
