import { isDesktopShell } from './client/api/apiBase';
import { Spinner } from '@heroui/react';
import { CoreAdmin, CustomRoutes, Resource } from 'ra-core';
import { lazy, Suspense, type ComponentType } from 'react';
import { BrowserRouter, Navigate, Route, Routes, useLocation } from 'react-router-dom';

import { dataProvider } from './api/dataProvider';
import { authProvider } from './auth/authProvider';
import { AdminLoginRedirect } from './auth/AdminLoginRedirect';
import { AdminLayout } from './layout/AdminLayout';
import { SystemLocaleProvider } from './settings/SystemLocaleProvider';
import {
  AccessDeniedPage,
  ApplicationError,
  AuthenticationErrorPage,
  LoadingPage,
  NotFoundPage,
} from './ui/SystemPages';

const AccessPage = lazyComponent(() => import('./access/AccessPage'), 'AccessPage');
const AiSettingsPage = lazyComponent(() => import('./settings/AiSettingsPage'), 'AiSettingsPage');
const AnnouncementsPage = lazyComponent(() => import('./announcements/AnnouncementsPage'), 'AnnouncementsPage');
const ClientApp = lazyComponent(() => import('./client/ClientApp'), 'ClientApp');
const GoogleDrivePage = lazyComponent(() => import('./storage/GoogleDrivePage'), 'GoogleDrivePage');
const LibrariesPage = lazyComponent(() => import('./libraries/LibrariesPage'), 'LibrariesPage');
const LibraryEditPage = lazyComponent(() => import('./libraries/LibraryEditPage'), 'LibraryEditPage');
const LogsPage = lazyComponent(() => import('./logs/LogsPage'), 'LogsPage');
const MetadataSettingsPage = lazyComponent(() => import('./settings/MetadataSettingsPage'), 'MetadataSettingsPage');
const OneDrivePage = lazyComponent(() => import('./storage/OneDrivePage'), 'OneDrivePage');
const SetupApp = lazyComponent(() => import('./setup/SetupApp'), 'SetupApp');
const SystemSettingsPage = lazyComponent(() => import('./settings/SystemSettingsPage'), 'SystemSettingsPage');
const TasksPage = lazyComponent(() => import('./tasks/TasksPage'), 'TasksPage');
const ThemeSettingsPage = lazyComponent(() => import('./settings/ThemeSettingsPage'), 'ThemeSettingsPage');
const UserCreate = lazyComponent(() => import('./users/UserCreate'), 'UserCreate');
const UserEdit = lazyComponent(() => import('./users/UserEdit'), 'UserEdit');
const UserList = lazyComponent(() => import('./users/UserList'), 'UserList');
const UserShow = lazyComponent(() => import('./users/UserShow'), 'UserShow');

const DashboardPage = lazy(async () => {
  const module = await import('./dashboard/DashboardPage');
  return { default: module.DashboardPage };
});

export function App() {
  return (
    <BrowserRouter>
      <Suspense fallback={<RouteLoading />}><RouteBoundary /></Suspense>
    </BrowserRouter>
  );
}

function NavigateToApp() {
  return <Navigate replace to="/app/" />;
}

function RouteLoading() {
  return (
    <div className="flex min-h-screen items-center justify-center" role="status">
      <Spinner aria-label="Loading application" color="accent" />
    </div>
  );
}

function lazyComponent<TModule>(
  load: () => Promise<TModule>,
  key: keyof TModule,
) {
  return lazy(async () => {
    const module = await load();
    return { default: module[key] as ComponentType };
  });
}

function RouteBoundary() {
  const location = useLocation();
  if (location.pathname === '/setup' || location.pathname.startsWith('/setup/')) {
    return <Routes><Route element={<SetupApp />} path="/setup/*" /></Routes>;
  }
  return <SystemLocaleProvider><ApplicationRoutes /></SystemLocaleProvider>;
}

function ApplicationRoutes() {
  if (isDesktopShell()) {
    return (
      <Routes>
        <Route element={<ClientApp />} path="/app/*" />
        <Route element={<NavigateToApp />} path="*" />
      </Routes>
    );
  }
  return (
      <Routes>
        <Route element={<ClientApp />} path="/app/*" />
        <Route
          path="/admin/*"
          element={(
            <CoreAdmin
              accessDenied={AccessDeniedPage}
              authenticationError={AuthenticationErrorPage}
              authProvider={authProvider}
              basename="/admin"
              catchAll={NotFoundPage}
              dataProvider={dataProvider}
              disableTelemetry
              error={ApplicationError}
              layout={AdminLayout}
              loading={LoadingPage}
              loginPage={AdminLoginRedirect}
              title="TJXY Admin"
            >
              <Resource
                create={UserCreate}
                edit={UserEdit}
                list={UserList}
                name="users"
                show={UserShow}
              />
              <CustomRoutes>
                <Route
                  element={<Suspense fallback={<LoadingPage />}><DashboardPage /></Suspense>}
                  path="/"
                />
                <Route element={<AccessPage />} path="/access" />
                <Route element={<AnnouncementsPage />} path="/announcements" />
                <Route element={<TasksPage />} path="/tasks" />
                <Route element={<LogsPage />} path="/logs" />
                <Route element={<LibrariesPage />} path="/libraries" />
                <Route element={<LibraryEditPage />} path="/libraries/:id" />
                <Route element={<MetadataSettingsPage />} path="/settings/metadata" />
                <Route element={<AiSettingsPage />} path="/settings/ai" />
                <Route element={<SystemSettingsPage />} path="/settings/system" />
                <Route element={<ThemeSettingsPage />} path="/settings/theme" />
                <Route element={<GoogleDrivePage />} path="/storage/google-drive" />
                <Route element={<OneDrivePage />} path="/storage/onedrive" />
              </CustomRoutes>
            </CoreAdmin>
          )}
        />
      </Routes>
  );
}
