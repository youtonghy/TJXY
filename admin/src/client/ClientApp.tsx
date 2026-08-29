import { Spinner, Toast } from '@heroui/react';
import { lazy, Suspense } from 'react';
import { Navigate, Outlet, Route, Routes, useLocation } from 'react-router-dom';
import { ClientAuthProvider, useClientAuth } from './auth/ClientAuthContext';
import { ClientLoginPage } from './auth/ClientLoginPage';
import { QrApprovalPage } from './auth/QrApprovalPage';
import { safeClientDestination } from './auth/clientDestination';
import { HomePage } from './catalog/HomePage';
import { ClientShell } from './layout/ClientShell';
import { useTranslate } from '../settings/i18n';
import { ClientThemeRuntime } from './themes/ThemeRuntime';

const AiChatPage = lazy(async () => ({ default: (await import('./ai/AiChatPage')).AiChatPage }));
const ItemPage = lazy(async () => ({ default: (await import('./catalog/ItemPage')).ItemPage }));
const LibrariesPage = lazy(async () => ({ default: (await import('./catalog/LibrariesPage')).LibrariesPage }));
const LibraryPage = lazy(async () => ({ default: (await import('./catalog/LibraryPage')).LibraryPage }));
const PlayerPage = lazy(async () => ({ default: (await import('./playback/PlayerPage')).PlayerPage }));
const ProfilePage = lazy(async () => ({ default: (await import('./profile/ProfilePage')).ProfilePage }));
const RankingsPage = lazy(async () => ({ default: (await import('./rankings/RankingsPage')).RankingsPage }));
const SearchPage = lazy(async () => ({ default: (await import('./catalog/SearchPage')).SearchPage }));

export function ClientApp() {
  return <ClientThemeRuntime><ClientAuthProvider><Suspense fallback={<ClientRouteLoading />}><Routes><Route element={<ClientLoginPage />} path="login" /><Route element={<ClientRouteGuard />}><Route element={<ClientShellLayout />}><Route index element={<HomePage />} /><Route element={<HomePage />} path="home" /><Route element={<LibrariesPage />} path="libraries" /><Route element={<LibraryPage />} path="libraries/:id" /><Route element={<SearchPage />} path="search" /><Route element={<RankingsPage />} path="rankings" /><Route element={<AiChatPage />} path="ai" /><Route element={<ProfilePage />} path="profile" /><Route element={<QrApprovalPage />} path="profile/authorize" /><Route element={<ItemPage />} path="items/:id" /><Route element={<PlayerPage />} path="play/:id" /></Route></Route><Route element={<Navigate replace to="/app/" />} path="*" /></Routes></Suspense><Toast.Provider placement="bottom end" /></ClientAuthProvider></ClientThemeRuntime>;
}

function ClientRouteLoading() { const tr = useTranslate(); return <div className="flex min-h-72 items-center justify-center" role="status"><Spinner aria-label={tr('Loading page', '正在加载页面')} color="accent" /></div>; }

function ClientShellLayout() { return <ClientShell><Outlet /></ClientShell>; }

function ClientRouteGuard() { const { user, isLoading } = useClientAuth(); const location = useLocation(); const tr = useTranslate(); if (isLoading) return <div className="flex min-h-screen items-center justify-center"><Spinner aria-label={tr('Loading account', '正在加载账户')} /></div>; if (!user) return <Navigate replace state={{ from: location.pathname + location.search }} to={`/login?redirect=${encodeURIComponent(safeClientDestination(location.pathname + location.search))}`} />; return <Outlet />; }
