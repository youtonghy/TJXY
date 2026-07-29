import { LogoutOnMount, useAuthState } from 'ra-core';
import type { ReactNode } from 'react';

import type { ApiError } from '../api/httpClient';
import { AccessDeniedPage, LoadingPage } from '../ui/SystemPages';

export function AdminRouteGuard({ children }: { children: ReactNode }) {
  const { authenticated, error, isPending } = useAuthState<ApiError>({}, false);

  if (isPending) return <LoadingPage />;
  if (authenticated) return children;
  if (error?.status === 403) return <AccessDeniedPage />;
  return <LogoutOnMount />;
}
