/* eslint-disable react-refresh/only-export-components */
import { Alert, Button, CloseButton, Spinner } from '@heroui/react';
import {
  Component,
  Suspense,
  createContext,
  useContext,
  useLayoutEffect,
  useState,
  type ReactNode,
} from 'react';
import { useSystemLocale } from '../../settings/SystemLocaleProvider';
import { useTranslate } from '../../settings/i18n';
import { useClientColorMode, type ClientColorMode } from '../layout/clientTheme';
import { resolveClientTheme } from './registry';
import type { ClientThemeDefinition, ThemeOptions } from './types';

interface ActiveThemeContextValue {
  definition: ClientThemeDefinition;
  options: ThemeOptions;
  colorMode: ClientColorMode;
  toggleColorMode: () => void;
}

const ActiveThemeContext = createContext<ActiveThemeContextValue | null>(null);

export function ClientThemeRuntime({ children }: { children: ReactNode }) {
  const { isLoading, settingsLoadFailed, theme } = useSystemLocale();
  const tr = useTranslate();
  const resolved = resolveClientTheme(theme);
  const { colorMode, toggleColorMode } = useClientColorMode();
  const warningKey = settingsLoadFailed ? 'settings' : resolved.didFallback ? theme.id : '';
  const [dismissedWarning, setDismissedWarning] = useState('');
  useClientThemeAttribute(resolved.definition.id, resolved.options, colorMode);

  if (isLoading) {
    return <div className="flex min-h-screen items-center justify-center bg-background"><Spinner aria-label={tr('Loading appearance', '正在加载外观')} /></div>;
  }
  return (
    <ThemeLoadBoundary key={resolved.definition.id}>
      <Suspense fallback={<div className="flex min-h-screen items-center justify-center bg-background"><Spinner aria-label={tr('Loading theme', '正在加载主题')} /></div>}>
        <ActiveThemeContext.Provider value={{ definition: resolved.definition, options: resolved.options, colorMode, toggleColorMode }}>
          {warningKey && dismissedWarning !== warningKey && (
            <div className="fixed bottom-4 right-4 z-[80] w-[min(28rem,calc(100vw-2rem))]">
              <Alert className="max-w-xl" status="warning">
                <Alert.Content>
                  <Alert.Title>{tr('Appearance fallback active', '已启用备用外观')}</Alert.Title>
                  <Alert.Description>
                    {settingsLoadFailed
                      ? tr('Server appearance settings could not be loaded. TJXY is using the local fallback.', '无法加载服务器外观设置，TJXY 正在使用本地备用设置。')
                      : tr('The configured theme is unavailable. TJXY is using the classic theme.', '配置的主题不可用，TJXY 正在使用经典主题。')}
                  </Alert.Description>
                </Alert.Content>
                <CloseButton aria-label={tr('Dismiss warning', '关闭提示')} onPress={() => { setDismissedWarning(warningKey); }} />
              </Alert>
            </div>
          )}
          {children}
        </ActiveThemeContext.Provider>
      </Suspense>
    </ThemeLoadBoundary>
  );
}

export function useActiveClientTheme(): ActiveThemeContextValue {
  const value = useContext(ActiveThemeContext);
  if (value === null) throw new Error('Client theme runtime is missing.');
  return value;
}

function useClientThemeAttribute(
  themeId: string,
  options: ThemeOptions,
  colorMode: ClientColorMode,
) {
  useLayoutEffect(() => {
    const root = document.documentElement;
    const previousSurface = root.dataset.tjxySurface;
    const previousTheme = root.dataset.clientTheme;
    const previousAccent = root.dataset.clientAccent;
    const previousDensity = root.dataset.clientDensity;
    const previousColorTheme = root.dataset.theme;
    const previousDark = root.classList.contains('dark');
    const previousLight = root.classList.contains('light');
    const previousColorScheme = root.style.colorScheme;
    root.dataset.tjxySurface = 'client';
    root.dataset.clientTheme = themeId;
    root.dataset.clientAccent = typeof options.accent === 'string' ? options.accent : '';
    root.dataset.clientDensity = typeof options.density === 'string' ? options.density : '';
    root.dataset.theme = colorMode;
    root.classList.toggle('dark', colorMode === 'dark');
    root.classList.toggle('light', colorMode === 'light');
    root.style.colorScheme = colorMode;
    return () => {
      restoreDataAttribute(root, 'tjxySurface', previousSurface);
      restoreDataAttribute(root, 'clientTheme', previousTheme);
      restoreDataAttribute(root, 'clientAccent', previousAccent);
      restoreDataAttribute(root, 'clientDensity', previousDensity);
      restoreDataAttribute(root, 'theme', previousColorTheme);
      root.classList.toggle('dark', previousDark);
      root.classList.toggle('light', previousLight);
      root.style.colorScheme = previousColorScheme;
    };
  }, [colorMode, options.accent, options.density, themeId]);
}

function restoreDataAttribute(
  root: HTMLElement,
  key: 'tjxySurface' | 'clientTheme' | 'clientAccent' | 'clientDensity' | 'theme',
  value: string | undefined,
) {
  if (value === undefined) root.removeAttribute(`data-${dataAttributeName(key)}`);
  else root.dataset[key] = value;
}

function dataAttributeName(key: 'tjxySurface' | 'clientTheme' | 'clientAccent' | 'clientDensity' | 'theme') {
  return key.replace(/[A-Z]/gu, (letter) => `-${letter.toLowerCase()}`);
}

class ThemeLoadBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() { return { failed: true }; }

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <main className="flex min-h-screen items-center justify-center bg-background p-6">
        <Alert className="max-w-lg" status="danger">
          <Alert.Content>
            <Alert.Title>Theme could not be loaded</Alert.Title>
            <Alert.Description>Reload the application to use the built-in fallback theme.</Alert.Description>
          </Alert.Content>
          <Button onPress={() => { window.location.reload(); }} variant="secondary">Reload</Button>
        </Alert>
      </main>
    );
  }
}
