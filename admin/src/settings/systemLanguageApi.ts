import { apiRequest } from '../api/httpClient';

export type SystemLocale = 'zh-CN' | 'en-US';
export interface SystemLanguageSettings { locale: SystemLocale; revision: number; supportedLocales: SystemLocale[]; }

export async function getSystemLanguage(signal?: AbortSignal): Promise<SystemLanguageSettings> {
  const value = await apiRequest<{ Locale?: unknown; Revision?: unknown; SupportedLocales?: unknown }>('/System/Language', { auth: 'none', ...(signal ? { signal } : {}) });
  return parse(value);
}

export async function saveSystemLanguage(locale: SystemLocale, revision: number | null, setup = false): Promise<SystemLanguageSettings> {
  const value = await apiRequest<{ Locale?: unknown; Revision?: unknown; SupportedLocales?: unknown }>(setup ? '/System/Language' : '/Admin/System/Language', {
    method: 'PUT',
    body: JSON.stringify({ Locale: locale, ...(setup || revision === null ? {} : { Revision: revision }) }),
    auth: setup ? 'none' : 'token',
  });
  return parse(value);
}

function parse(value: { Locale?: unknown; Revision?: unknown; SupportedLocales?: unknown }): SystemLanguageSettings {
  const supportedLocales = Array.isArray(value.SupportedLocales) ? value.SupportedLocales.filter(isLocale) : [];
  const locale = isLocale(value.Locale) ? value.Locale : 'zh-CN';
  if (typeof value.Revision !== 'number' || supportedLocales.length === 0) throw new Error('Invalid system language response');
  return { locale, revision: value.Revision, supportedLocales };
}

function isLocale(value: unknown): value is SystemLocale { return value === 'zh-CN' || value === 'en-US'; }
