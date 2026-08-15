import {
  Cloud,
  DatabaseZap,
  FolderKanban,
  ListTodo,
  Logs,
  LayoutDashboard,
  Megaphone,
  Palette,
  ShieldCheck,
  Settings2,
  Sparkles,
  UsersRound,
} from 'lucide-react';
import type { ComponentType } from 'react';

export interface AdminNavigationGroup {
  labelKey: string;
  items: readonly {
    labelKey: string;
    to: string;
    icon: ComponentType<{ size?: number; 'aria-hidden'?: boolean }>;
  }[];
}

export const adminNavigation: readonly AdminNavigationGroup[] = [
  {
    labelKey: 'admin.navigation.manage',
    items: [
      { labelKey: 'admin.navigation.dashboard', to: '/admin', icon: LayoutDashboard },
      { labelKey: 'admin.navigation.users', to: '/admin/users', icon: UsersRound },
      { labelKey: 'admin.navigation.access', to: '/admin/access', icon: ShieldCheck },
      { labelKey: 'admin.navigation.libraries', to: '/admin/libraries', icon: FolderKanban },
      { labelKey: 'admin.navigation.announcements', to: '/admin/announcements', icon: Megaphone },
    ],
  },
  {
    labelKey: 'admin.navigation.operations',
    items: [
      { labelKey: 'admin.navigation.tasks', to: '/admin/tasks', icon: ListTodo },
      { labelKey: 'admin.navigation.logs', to: '/admin/logs', icon: Logs },
    ],
  },
  {
    labelKey: 'admin.navigation.storage',
    items: [
      { labelKey: 'admin.navigation.googleDrive', to: '/admin/storage/google-drive', icon: Cloud },
      { labelKey: 'admin.navigation.oneDrive', to: '/admin/storage/onedrive', icon: Cloud },
    ],
  },
  {
    labelKey: 'admin.navigation.system',
    items: [
      { labelKey: 'admin.navigation.metadata', to: '/admin/settings/metadata', icon: DatabaseZap },
      { labelKey: 'admin.navigation.ai', to: '/admin/settings/ai', icon: Sparkles },
      { labelKey: 'admin.navigation.theme', to: '/admin/settings/theme', icon: Palette },
      { labelKey: 'admin.navigation.systemSettings', to: '/admin/settings/system', icon: Settings2 },
    ],
  },
];
