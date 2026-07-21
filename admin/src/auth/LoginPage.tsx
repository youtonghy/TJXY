import { AdminPanelSettingsOutlined as AdminPanelSettingsOutlinedIcon } from '@mui/icons-material';
import { Box, Stack, Typography } from '@mui/material';
import { Login, LoginForm } from 'react-admin';

export function LoginPage() {
  return (
    <Login
      avatarIcon={<AdminPanelSettingsOutlinedIcon />}
      sx={{
        background: '#f6f7f8',
        justifyContent: 'center',
        '& .RaLogin-card': { marginTop: 0, width: 'min(92vw, 380px)' },
        '& .RaLogin-avatar': { marginBottom: 0 },
      }}
    >
      <Stack spacing={0.5} sx={{ px: 3, pt: 1, textAlign: 'center' }}>
        <Typography component="h1" variant="h2">TJXY Admin</Typography>
        <Typography color="text.secondary" variant="body2">Administrator access</Typography>
      </Stack>
      <LoginForm />
      <Box sx={{ height: 12 }} />
    </Login>
  );
}
