import { Spinner } from '@heroui/react';
import { Navigate, Outlet, Route, Routes, useLocation } from 'react-router-dom';
import { ClientAuthProvider, useClientAuth } from './auth/ClientAuthContext';
import { ClientLoginPage } from './auth/ClientLoginPage';
import { safeClientDestination } from './auth/clientDestination';
import { HomePage } from './catalog/HomePage';
import { ItemPage } from './catalog/ItemPage';
import { LibraryPage } from './catalog/LibraryPage';
import { LibrariesPage } from './catalog/LibrariesPage';
import { SearchPage } from './catalog/SearchPage';
import { ClientShell } from './layout/ClientShell';
import { PlayerPage } from './playback/PlayerPage';
import { ProfilePage } from './profile/ProfilePage';
import { RankingsPage } from './rankings/RankingsPage';
import { useTranslate } from '../settings/i18n';
import { AiChatPage } from './ai/AiChatPage';
import { ClientThemeRuntime } from './themes/ThemeRuntime';

export function ClientApp() {
  return <ClientThemeRuntime><ClientAuthProvider><Routes><Route element={<ClientLoginPage />} path="login" /><Route element={<ClientRouteGuard />}><Route element={<ClientShellLayout />}><Route index element={<HomePage />} /><Route element={<HomePage />} path="home" /><Route element={<LibrariesPage />} path="libraries" /><Route element={<LibraryPage />} path="libraries/:id" /><Route element={<SearchPage />} path="search" /><Route element={<RankingsPage />} path="rankings" /><Route element={<AiChatPage />} path="ai" /><Route element={<ProfilePage />} path="profile" /><Route element={<ItemPage />} path="items/:id" /><Route element={<PlayerPage />} path="play/:id" /></Route></Route><Route element={<Navigate replace to="/app/" />} path="*" /></Routes></ClientAuthProvider></ClientThemeRuntime>;
}

function ClientShellLayout() { return <ClientShell><Outlet /></ClientShell>; }

function ClientRouteGuard() { const { user, isLoading } = useClientAuth(); const location = useLocation(); const tr = useTranslate(); if (isLoading) return <div className="flex min-h-screen items-center justify-center"><Spinner aria-label={tr('Loading account', '正在加载账户')} /></div>; if (!user) return <Navigate replace state={{ from: location.pathname + location.search }} to={`/app/login?redirect=${encodeURIComponent(safeClientDestination(location.pathname + location.search))}`} />; return <Outlet />; }
