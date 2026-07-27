import { PeopleOutlined as PeopleOutlineIcon } from '@mui/icons-material';
import { Admin, Authenticated, CustomRoutes, Resource } from 'react-admin';
import { BrowserRouter, Route, Routes } from 'react-router-dom';

import { dataProvider } from './api/dataProvider';
import { AccessPage } from './access/AccessPage';
import { authProvider } from './auth/authProvider';
import { LoginPage } from './auth/LoginPage';
import { AdminLayout } from './layout/AdminLayout';
import { LibrariesPage } from './libraries/LibrariesPage';
import { theme } from './theme';
import { GoogleDrivePage } from './storage/GoogleDrivePage';
import { OneDrivePage } from './storage/OneDrivePage';
import { TasksPage } from './tasks/TasksPage';
import { UserCreate } from './users/UserCreate';
import { UserEdit } from './users/UserEdit';
import { UserList } from './users/UserList';
import { UserShow } from './users/UserShow';

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route
          path="/admin/*"
          element={(
            <Admin
              basename="/admin"
              authProvider={authProvider}
              dataProvider={dataProvider}
              layout={AdminLayout}
              loginPage={LoginPage}
              theme={theme}
              requireAuth
              title="TJXY Admin"
            >
              <Resource
                name="users"
                list={UserList}
                create={UserCreate}
                edit={UserEdit}
                show={UserShow}
                icon={PeopleOutlineIcon}
                options={{ label: 'Users' }}
              />
              <CustomRoutes>
                <Route
                  path="/access"
                  element={<Authenticated><AccessPage /></Authenticated>}
                />
                <Route
                  path="/tasks"
                  element={<Authenticated><TasksPage /></Authenticated>}
                />
                <Route
                  path="/libraries"
                  element={<Authenticated><LibrariesPage /></Authenticated>}
                />
                <Route
                  path="/storage/google-drive"
                  element={<Authenticated><GoogleDrivePage /></Authenticated>}
                />
                <Route
                  path="/storage/onedrive"
                  element={<Authenticated><OneDrivePage /></Authenticated>}
                />
              </CustomRoutes>
            </Admin>
          )}
        />
      </Routes>
    </BrowserRouter>
  );
}
