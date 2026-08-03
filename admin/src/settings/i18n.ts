import { useCallback } from 'react';

import { useSystemLocale } from './SystemLocaleProvider';
import enUS from './locales/en-US';
import zhCN from './locales/zh-CN';
import type { SystemLocale } from './systemLanguageApi';

const catalogs: Record<SystemLocale, Record<string, string>> = { 'en-US': enUS, 'zh-CN': zhCN };

export function translate(locale: SystemLocale, key: string, chineseFallback?: string): string {
  if (chineseFallback !== undefined) return locale === 'zh-CN' ? chineseFallback : key;
  return catalogs[locale][key] ?? catalogs['en-US'][key] ?? key;
}

export function interpolate(value: string, values: Record<string, string>): string {
  return Object.entries(values).reduce(
    (result, [key, replacement]) => result.replaceAll(`{${key}}`, replacement),
    value,
  );
}

export function useTranslate() {
  const { locale } = useSystemLocale();
  return useCallback(
    (key: string, chineseFallback?: string) => translate(locale, key, chineseFallback),
    [locale],
  );
}
