import { apiRequest } from '../api/httpClient';
import type { SystemLocale } from './systemLanguageApi';

export interface EnvironmentOverrides {
  siteTitle: boolean;
  publicUrl: boolean;
  listenAddress: boolean;
}

export interface PublicSiteThemeSettings {
  id: string;
  schemaVersion: number;
  options: Record<string, unknown>;
  revision: number;
}

export interface StreamAddressSetting {
  name: string;
  url: string;
}

export const MAX_STREAM_ADDRESSES = 10;

export interface SystemSettings {
  locale: SystemLocale;
  siteTitle: string;
  siteSubtitle: string;
  logoUrl: string;
  iconUrl: string;
  publicUrl: string;
  /** Alternate origins of this server that clients may stream from. */
  streamAddresses: StreamAddressSetting[];
  listenHost: string;
  port: number;
  passkeyEnabled: boolean;
  revision: number;
  restartRequired: boolean;
  environmentOverrides: EnvironmentOverrides;
  theme: PublicSiteThemeSettings;
}

export type SaveSystemSettings = Omit<
  SystemSettings,
  'restartRequired' | 'environmentOverrides' | 'theme'
>;
type SettingsResponse = Record<string, unknown>;

export async function getPublicSystemSettings(signal?: AbortSignal): Promise<SystemSettings> {
  const value = await apiRequest<SettingsResponse>('/System/Settings', {
    auth: 'none',
    ...(signal ? { signal } : {}),
  });
  return parse(value, false);
}

export async function getSystemSettings(signal?: AbortSignal): Promise<SystemSettings> {
  const value = await apiRequest<SettingsResponse>('/Admin/System/Settings', {
    ...(signal ? { signal } : {}),
  });
  return parse(value, true);
}

export async function saveSystemSettings(settings: SaveSystemSettings): Promise<SystemSettings> {
  const value = await apiRequest<SettingsResponse>('/Admin/System/Settings', {
    method: 'PUT',
    body: JSON.stringify({
      Locale: settings.locale,
      SiteTitle: settings.siteTitle,
      SiteSubtitle: settings.siteSubtitle,
      LogoUrl: settings.logoUrl,
      IconUrl: settings.iconUrl,
      PublicUrl: settings.publicUrl.trim() || null,
      StreamAddresses: settings.streamAddresses
        .map((address) => ({ Name: address.name.trim(), Url: address.url.trim() }))
        .filter((address) => address.Name || address.Url),
      ListenHost: settings.listenHost,
      Port: settings.port,
      PasskeyEnabled: settings.passkeyEnabled,
      ...(settings.revision > 0 ? { Revision: settings.revision } : {}),
    }),
  });
  return parse(value, true);
}

export async function uploadBrandAsset(kind: 'logo' | 'icon', file: File): Promise<{ url: string }> {
  const value = await apiRequest<{ Url?: unknown }>(`/Admin/System/Branding/${kind}`, {
    method: 'PUT',
    body: file,
    headers: { 'Content-Type': file.type },
  });
  if (typeof value.Url !== 'string' || !value.Url.startsWith('/Branding/Assets/')) {
    throw new Error('Invalid brand asset response');
  }
  return { url: value.Url };
}

export async function restartSystem(): Promise<void> {
  await apiRequest('/Admin/System/Restart', { method: 'POST' });
}

function parse(value: SettingsResponse, admin: boolean): SystemSettings {
  const locale = value.Locale === 'en-US' || value.Locale === 'zh-CN' ? value.Locale : null;
  const siteTitle = stringValue(value.SiteTitle);
  const siteSubtitle = stringValue(value.SiteSubtitle);
  const logoUrl = stringValue(value.LogoUrl);
  const iconUrl = stringValue(value.IconUrl);
  const revision = numberValue(value.Revision);
  if (locale === null || siteTitle === null || siteSubtitle === null || logoUrl === null || iconUrl === null || revision === null) {
    throw new Error('Invalid system settings response');
  }
  const overrides = value.EnvironmentOverrides;
  const theme = parseTheme(value.Theme);
  return {
    locale,
    siteTitle,
    siteSubtitle,
    logoUrl,
    iconUrl,
    publicUrl: typeof value.PublicUrl === 'string' ? value.PublicUrl : '',
    streamAddresses: admin ? parseStreamAddresses(value.StreamAddresses) : [],
    listenHost: admin ? stringValue(value.ListenHost) ?? '127.0.0.1' : '127.0.0.1',
    port: admin ? numberValue(value.Port) ?? 8096 : 8096,
    passkeyEnabled: value.PasskeyEnabled === true,
    revision,
    restartRequired: value.RestartRequired === true,
    environmentOverrides: isOverrides(overrides)
      ? overrides
      : {
        siteTitle: false,
        publicUrl: false,
        listenAddress: false,
      },
    theme,
  };
}

function parseTheme(value: unknown): PublicSiteThemeSettings {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return defaultTheme();
  const candidate = value as Record<string, unknown>;
  if (
    typeof candidate.Id !== 'string'
    || !/^[a-z][a-z0-9-]{0,63}$/u.test(candidate.Id)
    || !Number.isSafeInteger(candidate.SchemaVersion)
    || typeof candidate.SchemaVersion !== 'number'
    || candidate.SchemaVersion <= 0
    || typeof candidate.Options !== 'object'
    || candidate.Options === null
    || Array.isArray(candidate.Options)
    || !Number.isSafeInteger(candidate.Revision)
    || typeof candidate.Revision !== 'number'
    || candidate.Revision < 0
  ) throw new Error('Invalid site theme settings response');
  return {
    id: candidate.Id,
    schemaVersion: candidate.SchemaVersion,
    options: candidate.Options as Record<string, unknown>,
    revision: candidate.Revision,
  };
}

function parseStreamAddresses(value: unknown): StreamAddressSetting[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) throw new Error('Invalid stream addresses response');
  return value.map((entry: unknown) => {
    const candidate = (typeof entry === 'object' && entry !== null ? entry : {}) as Record<string, unknown>;
    if (typeof candidate.Name !== 'string' || typeof candidate.Url !== 'string') {
      throw new Error('Invalid stream addresses response');
    }
    return { name: candidate.Name, url: candidate.Url };
  });
}

function defaultTheme(): PublicSiteThemeSettings {
  return { id: 'classic', schemaVersion: 1, options: {}, revision: 0 };
}

function stringValue(value: unknown): string | null { return typeof value === 'string' ? value : null; }
function numberValue(value: unknown): number | null { return typeof value === 'number' && Number.isFinite(value) ? value : null; }
function isOverrides(value: unknown): value is EnvironmentOverrides {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return typeof candidate.siteTitle === 'boolean'
    && typeof candidate.publicUrl === 'boolean'
    && typeof candidate.listenAddress === 'boolean';
}
