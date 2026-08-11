import { Home, Library, Search, Sparkles, Trophy } from 'lucide-react';
import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { ClientAnnouncements } from '../../announcements/ClientAnnouncements';
import { useSystemLocale } from '../../settings/SystemLocaleProvider';
import { useTranslate } from '../../settings/i18n';
import { getAiModels } from '../ai/aiApi';
import { useClientAuth } from '../auth/ClientAuthContext';
import { useActiveClientTheme } from '../themes/ThemeRuntime';
import type { ThemeNavigationItem } from '../themes/types';

const navigationItems = [
  { id: 'home', to: '/app/', label: 'Home', chineseLabel: '首页', icon: Home },
  { id: 'libraries', to: '/app/libraries', label: 'Libraries', chineseLabel: '媒体库', icon: Library },
  { id: 'search', to: '/app/search', label: 'Search', chineseLabel: '搜索', icon: Search },
  { id: 'rankings', to: '/app/rankings', label: 'Rankings', chineseLabel: '排行榜', icon: Trophy },
  { id: 'ai', to: '/app/ai', label: 'AI assistant', chineseLabel: 'AI 助手', icon: Sparkles },
] as const;

export function ClientShell({ children }: { children: ReactNode }) {
  const { user, signOut } = useClientAuth();
  const { siteTitle, logoUrl } = useSystemLocale();
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const tr = useTranslate();
  const { definition, options, colorMode, toggleColorMode } = useActiveClientTheme();
  const [aiAvailable, setAiAvailable] = useState(false);

  useEffect(() => {
    let active = true;
    void getAiModels()
      .then((models) => { if (active) setAiAvailable(models.length > 0); })
      .catch(() => { if (active) setAiAvailable(false); });
    return () => { active = false; };
  }, []);

  const navigation = useMemo<ThemeNavigationItem[]>(() => navigationItems
    .filter(({ id }) => id !== 'ai' || aiAvailable)
    .map(({ id, to, label, chineseLabel, icon }) => ({
      id, to, icon, label: tr(label, chineseLabel),
    })), [aiAvailable, tr]);
  const Shell = definition.Shell;
  return (
    <Shell
      announcements={<ClientAnnouncements />}
      colorMode={colorMode}
      logoUrl={logoUrl}
      navigation={navigation}
      onNavigate={(destination) => { void navigate(destination); }}
      onSignOut={() => { void signOut(); }}
      onToggleColorMode={toggleColorMode}
      options={options}
      pathname={pathname}
      siteTitle={siteTitle}
      userName={user?.Name ?? tr('User', '用户')}
    >
      {children}
    </Shell>
  );
}
