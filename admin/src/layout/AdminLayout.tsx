import { AppBar, Layout, Menu, TitlePortal } from 'react-admin';

const AdminAppBar = () => (
  <AppBar color="inherit" elevation={0} sx={{ borderBottom: '1px solid #dce2e5' }}>
    <TitlePortal />
  </AppBar>
);

const AdminMenu = () => (
  <Menu>
    <Menu.ResourceItem name="users" />
  </Menu>
);

export function AdminLayout({ children }: { children: React.ReactNode }) {
  return <Layout appBar={AdminAppBar} menu={AdminMenu}>{children}</Layout>;
}
