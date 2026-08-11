import type { ComponentType, LazyExoticComponent, ReactNode } from 'react';
import type { ClientColorMode } from '../layout/clientTheme';

export type ThemeOptions = Record<string, string | number | boolean>;

export interface ThemeOptionChoice {
  value: string;
  labelKey: string;
}

export type ThemeOptionField =
  | {
    key: string;
    kind: 'select';
    labelKey: string;
    descriptionKey: string;
    choices: readonly ThemeOptionChoice[];
  }
  | {
    key: string;
    kind: 'boolean';
    labelKey: string;
    descriptionKey: string;
  };

export interface ThemeNavigationItem {
  id: 'home' | 'libraries' | 'search' | 'rankings' | 'ai';
  to: string;
  label: string;
  icon: ComponentType<{ className?: string; 'aria-hidden'?: boolean }>;
}

export interface ThemeShellProps {
  children: ReactNode;
  navigation: readonly ThemeNavigationItem[];
  pathname: string;
  siteTitle: string;
  logoUrl: string;
  userName: string;
  announcements: ReactNode;
  colorMode: ClientColorMode;
  onToggleColorMode: () => void;
  onNavigate: (destination: string) => void;
  onSignOut: () => void;
  options: ThemeOptions;
}

export interface ThemeLoginFrameProps {
  children: ReactNode;
  actions: ReactNode;
  siteTitle: string;
  siteSubtitle: string;
  logoUrl: string;
  options: ThemeOptions;
}

export interface ThemePreviewProps {
  options: ThemeOptions;
}

export interface ClientThemeDefinition {
  id: string;
  labelKey: string;
  descriptionKey: string;
  schemaVersion: number;
  defaultOptions: ThemeOptions;
  optionFields: readonly ThemeOptionField[];
  normalizeOptions: (schemaVersion: number, options: Record<string, unknown>) => ThemeOptions;
  Shell: LazyExoticComponent<ComponentType<ThemeShellProps>>;
  LoginFrame: LazyExoticComponent<ComponentType<ThemeLoginFrameProps>>;
  Preview: ComponentType<ThemePreviewProps>;
}
