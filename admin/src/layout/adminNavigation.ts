import {
  Cloud,
  FolderKanban,
  ListTodo,
  ShieldCheck,
  UsersRound,
} from 'lucide-react';
import type { ComponentType } from 'react';

export interface AdminNavigationGroup {
  label: 'Manage' | 'Operations' | 'Storage';
  items: readonly {
    label: string;
    to: string;
    icon: ComponentType<{ size?: number; 'aria-hidden'?: boolean }>;
  }[];
}

export const adminNavigation: readonly AdminNavigationGroup[] = [
  {
    label: 'Manage',
    items: [
      { label: 'Users', to: '/admin/users', icon: UsersRound },
      { label: 'Access', to: '/admin/access', icon: ShieldCheck },
      { label: 'Libraries', to: '/admin/libraries', icon: FolderKanban },
    ],
  },
  {
    label: 'Operations',
    items: [
      { label: 'Tasks', to: '/admin/tasks', icon: ListTodo },
    ],
  },
  {
    label: 'Storage',
    items: [
      { label: 'Google Drive', to: '/admin/storage/google-drive', icon: Cloud },
      { label: 'OneDrive', to: '/admin/storage/onedrive', icon: Cloud },
    ],
  },
];
