/* eslint-disable react-refresh/only-export-components */
import { lazy } from 'react';
import type { PublicSiteThemeSettings } from '../../settings/systemSettingsApi';
import type {
  ClientThemeDefinition,
  ThemeOptions,
  ThemePreviewProps,
} from './types';

const ClassicShell = lazy(async () => {
  const module = await import('./classic/ClassicTheme');
  return { default: module.ClassicThemeShell };
});
const ClassicLoginFrame = lazy(async () => {
  const module = await import('./classic/ClassicTheme');
  return { default: module.ClassicLoginFrame };
});
const CinemaShell = lazy(async () => {
  const module = await import('./cinema/CinemaTheme');
  return { default: module.CinemaThemeShell };
});
const CinemaLoginFrame = lazy(async () => {
  const module = await import('./cinema/CinemaTheme');
  return { default: module.CinemaLoginFrame };
});

const classicDefaults = { contentWidth: 'standard' } satisfies ThemeOptions;
const cinemaDefaults = {
  density: 'comfortable',
  contentWidth: 'wide',
  accent: 'crimson',
} satisfies ThemeOptions;

export const clientThemes: readonly ClientThemeDefinition[] = [
  {
    id: 'classic',
    labelKey: 'admin.theme.classicName',
    descriptionKey: 'admin.theme.classicDescription',
    schemaVersion: 1,
    defaultOptions: classicDefaults,
    optionFields: [{
      key: 'contentWidth', kind: 'select', labelKey: 'admin.theme.contentWidth',
      descriptionKey: 'admin.theme.contentWidthDescription', choices: [
        { value: 'standard', labelKey: 'admin.theme.widthStandard' },
        { value: 'wide', labelKey: 'admin.theme.widthWide' },
      ],
    }],
    normalizeOptions: (schemaVersion, options) => schemaVersion === 1
      ? { contentWidth: oneOf(options.contentWidth, ['standard', 'wide'], 'standard') }
      : { ...classicDefaults },
    Shell: ClassicShell,
    LoginFrame: ClassicLoginFrame,
    Preview: ClassicPreview,
  },
  {
    id: 'cinema',
    labelKey: 'admin.theme.cinemaName',
    descriptionKey: 'admin.theme.cinemaDescription',
    schemaVersion: 1,
    defaultOptions: cinemaDefaults,
    optionFields: [
      {
        key: 'density', kind: 'select', labelKey: 'admin.theme.density',
        descriptionKey: 'admin.theme.densityDescription', choices: [
          { value: 'comfortable', labelKey: 'admin.theme.densityComfortable' },
          { value: 'compact', labelKey: 'admin.theme.densityCompact' },
        ],
      },
      {
        key: 'contentWidth', kind: 'select', labelKey: 'admin.theme.contentWidth',
        descriptionKey: 'admin.theme.contentWidthDescription', choices: [
          { value: 'standard', labelKey: 'admin.theme.widthStandard' },
          { value: 'wide', labelKey: 'admin.theme.widthWide' },
        ],
      },
      {
        key: 'accent', kind: 'select', labelKey: 'admin.theme.accent',
        descriptionKey: 'admin.theme.accentDescription', choices: [
          { value: 'crimson', labelKey: 'admin.theme.accentCrimson' },
          { value: 'gold', labelKey: 'admin.theme.accentGold' },
          { value: 'teal', labelKey: 'admin.theme.accentTeal' },
        ],
      },
    ],
    normalizeOptions: (schemaVersion, options) => schemaVersion === 1 ? {
      density: oneOf(options.density, ['comfortable', 'compact'], 'comfortable'),
      contentWidth: oneOf(options.contentWidth, ['standard', 'wide'], 'wide'),
      accent: oneOf(options.accent, ['crimson', 'gold', 'teal'], 'crimson'),
    } : { ...cinemaDefaults },
    Shell: CinemaShell,
    LoginFrame: CinemaLoginFrame,
    Preview: CinemaPreview,
  },
];

export const defaultClientTheme = clientThemes[0] ?? missingDefaultTheme();

export function findClientTheme(id: string): ClientThemeDefinition | undefined {
  return clientThemes.find((theme) => theme.id === id);
}

export function resolveClientTheme(settings: PublicSiteThemeSettings): {
  definition: ClientThemeDefinition;
  options: ThemeOptions;
  didFallback: boolean;
} {
  const definition = findClientTheme(settings.id) ?? defaultClientTheme;
  return {
    definition,
    options: definition.normalizeOptions(settings.schemaVersion, settings.options),
    didFallback: definition.id !== settings.id,
  };
}

function oneOf<Value extends string>(
  value: unknown,
  choices: readonly Value[],
  fallback: Value,
): Value {
  return typeof value === 'string' && choices.some((choice) => choice === value)
    ? value as Value
    : fallback;
}

function missingDefaultTheme(): never {
  throw new Error('The client theme registry requires a default theme.');
}

function ClassicPreview({ options }: ThemePreviewProps) {
  const width = options.contentWidth === 'wide' ? '92%' : '76%';
  return <div className="theme-preview theme-preview--classic"><div className="theme-preview__topbar" /><div className="theme-preview__content" style={{ width }}><span /><span /><span /></div></div>;
}

function CinemaPreview({ options }: ThemePreviewProps) {
  return <div className={`theme-preview theme-preview--cinema theme-preview--${String(options.accent)}`}><div className="theme-preview__rail" /><div className="theme-preview__stage"><div className="theme-preview__hero" /><div className="theme-preview__posters"><span /><span /><span /><span /></div></div></div>;
}
