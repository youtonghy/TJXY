import { CoreAdmin, CustomRoutes, Resource } from 'ra-core';
import { lazy, Suspense } from 'react';
import { BrowserRouter, Route, Routes, useLocation } from 'react-router-dom';

import { AccessPage } from './access/AccessPage';
import { AnnouncementsPage } from './announcements/AnnouncementsPage';
import { dataProvider } from './api/dataProvider';
import { authProvider } from './auth/authProvider';
import { AdminLoginRedirect } from './auth/AdminLoginRedirect';
import { AdminLayout } from './layout/AdminLayout';
import { LibraryEditPage } from './libraries/LibraryEditPage';
import { LibrariesPage } from './libraries/LibrariesPage';
import { MetadataSettingsPage } from './settings/MetadataSettingsPage';
import { AiSettingsPage } from './settings/AiSettingsPage';
import { SystemLocaleProvider } from './settings/SystemLocaleProvider';
import { SystemSettingsPage } from './settings/SystemSettingsPage';
import { GoogleDrivePage } from './storage/GoogleDrivePage';
import { OneDrivePage } from './storage/OneDrivePage';
import { SetupApp } from './setup/SetupApp';
import { TasksPage } from './tasks/TasksPage';
import {
  AccessDeniedPage,
  ApplicationError,
  AuthenticationErrorPage,
  LoadingPage,
  NotFoundPage,
} from './ui/SystemPages';
import { UserCreate } from './users/UserCreate';
import { UserEdit } from './users/UserEdit';
import { UserList } from './users/UserList';
import { UserShow } from './users/UserShow';
import { ClientApp } from './client/ClientApp';

const DashboardPage = lazy(async () => {
  const module = await import('./dashboard/DashboardPage');
  return { default: module.DashboardPage };
});

export function App() {
  return (
    <BrowserRouter><RouteBoundary /></BrowserRouter>
  );
}

function RouteBoundary() {
  const location = useLocation();
  if (location.pathname === '/setup' || location.pathname.startsWith('/setup/')) {
    return <Routes><Route element={<SetupApp />} path="/setup/*" /></Routes>;
  }
  return <SystemLocaleProvider><ApplicationRoutes /></SystemLocaleProvider>;
}

function ApplicationRoutes() {
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
                <Route element={<LibrariesPage />} path="/libraries" />
                <Route element={<LibraryEditPage />} path="/libraries/:id" />
                <Route element={<MetadataSettingsPage />} path="/settings/metadata" />
                <Route element={<AiSettingsPage />} path="/settings/ai" />
                <Route element={<SystemSettingsPage />} path="/settings/system" />
                <Route element={<GoogleDrivePage />} path="/storage/google-drive" />
                <Route element={<OneDrivePage />} path="/storage/onedrive" />
              </CustomRoutes>
            </CoreAdmin>
          )}
        />
      </Routes>
  );
}
