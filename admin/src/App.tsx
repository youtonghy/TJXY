import { CoreAdmin, CustomRoutes, Resource } from 'ra-core';
import { BrowserRouter, Route, Routes } from 'react-router-dom';

import { AccessPage } from './access/AccessPage';
import { dataProvider } from './api/dataProvider';
import { authProvider } from './auth/authProvider';
import { LoginPage } from './auth/LoginPage';
import { AdminLayout } from './layout/AdminLayout';
import { LibraryEditPage } from './libraries/LibraryEditPage';
import { LibrariesPage } from './libraries/LibrariesPage';
import { MetadataSettingsPage } from './settings/MetadataSettingsPage';
import { GoogleDrivePage } from './storage/GoogleDrivePage';
import { OneDrivePage } from './storage/OneDrivePage';
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

export function App() {
  return (
    <BrowserRouter>
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
              loginPage={LoginPage}
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
                <Route element={<AccessPage />} path="/access" />
                <Route element={<TasksPage />} path="/tasks" />
                <Route element={<LibrariesPage />} path="/libraries" />
                <Route element={<LibraryEditPage />} path="/libraries/:id" />
                <Route element={<MetadataSettingsPage />} path="/settings/metadata" />
                <Route element={<GoogleDrivePage />} path="/storage/google-drive" />
                <Route element={<OneDrivePage />} path="/storage/onedrive" />
              </CustomRoutes>
            </CoreAdmin>
          )}
        />
      </Routes>
    </BrowserRouter>
  );
}
