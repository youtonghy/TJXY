import { Toast } from '@heroui/react';
import type { ReactNode } from 'react';

import { AdminNotifications } from '../ui/AdminNotifications';
import { AdminRouteGuard } from './AdminRouteGuard';
import { AdminShell } from './AdminShell';

export function AdminLayout({ children }: { children: ReactNode }) {
  return (
    <AdminRouteGuard>
      <AdminShell>{children}</AdminShell>
      <AdminNotifications />
      <Toast.Provider placement="bottom end" />
    </AdminRouteGuard>
  );
}
