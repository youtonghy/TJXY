import { PeopleOutlined as PeopleOutlineIcon } from '@mui/icons-material';
import { Admin, Resource } from 'react-admin';

import { dataProvider } from './api/dataProvider';
import { authProvider } from './auth/authProvider';
import { LoginPage } from './auth/LoginPage';
import { AdminLayout } from './layout/AdminLayout';
import { theme } from './theme';
import { UserCreate } from './users/UserCreate';
import { UserEdit } from './users/UserEdit';
import { UserList } from './users/UserList';
import { UserShow } from './users/UserShow';

export function App() {
  return (
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
    </Admin>
  );
}
