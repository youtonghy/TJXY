import { CloudOutlined, FolderCopyOutlined, SecurityOutlined, TaskAltOutlined } from '@mui/icons-material';
import { AppBar, Layout, Menu, TitlePortal } from 'react-admin';

const AdminAppBar = () => (
  <AppBar color="inherit" elevation={0} sx={{ borderBottom: '1px solid #dce2e5' }}>
    <TitlePortal />
  </AppBar>
);

const AdminMenu = () => (
  <Menu>
    <Menu.ResourceItem name="users" />
    <Menu.Item
      to="/admin/access"
      primaryText="Access"
      leftIcon={<SecurityOutlined />}
    />
    <Menu.Item
      to="/admin/tasks"
      primaryText="Tasks"
      leftIcon={<TaskAltOutlined />}
    />
    <Menu.Item
      to="/admin/libraries"
      primaryText="Libraries"
      leftIcon={<FolderCopyOutlined />}
    />
    <Menu.Item
      to="/admin/storage/google-drive"
      primaryText="Google Drive"
      leftIcon={<CloudOutlined />}
    />
    <Menu.Item
      to="/admin/storage/onedrive"
      primaryText="OneDrive"
      leftIcon={<CloudOutlined />}
    />
  </Menu>
);

export function AdminLayout({ children }: { children: React.ReactNode }) {
  return <Layout appBar={AdminAppBar} menu={AdminMenu}>{children}</Layout>;
}
