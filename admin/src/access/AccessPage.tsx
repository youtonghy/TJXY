import { SecurityOutlined } from '@mui/icons-material';
import { Box, Stack, Tab, Tabs, Typography } from '@mui/material';
import { useState } from 'react';
import { Title } from 'react-admin';

import { ApiKeysPanel } from './ApiKeysPanel';
import { DevicesPanel } from './DevicesPanel';

type AccessTab = 'devices' | 'api-keys';

export function AccessPage() {
  const [tab, setTab] = useState<AccessTab>('devices');
  return (
    <Box sx={{ boxSizing: 'border-box', maxWidth: 1120, minWidth: 0, p: { xs: 2, sm: 3 }, width: '100%' }}>
      <Title title="Access" />
      <Stack direction="row" spacing={1.5} sx={{ alignItems: 'center', mb: 2 }}>
        <SecurityOutlined color="primary" />
        <Typography component="h1" variant="h1">Access</Typography>
      </Stack>
      <Tabs
        aria-label="Access management"
        value={tab}
        onChange={(_, value: AccessTab) => { setTab(value); }}
        sx={{ borderBottom: '1px solid', borderColor: 'divider', mb: 2 }}
      >
        <Tab
          id="access-tab-devices"
          aria-controls="access-panel-devices"
          label="Devices"
          value="devices"
        />
        <Tab
          id="access-tab-api-keys"
          aria-controls="access-panel-api-keys"
          label="API Keys"
          value="api-keys"
        />
      </Tabs>
      <Box
        id={tab === 'devices' ? 'access-panel-devices' : 'access-panel-api-keys'}
        aria-labelledby={tab === 'devices' ? 'access-tab-devices' : 'access-tab-api-keys'}
        role="tabpanel"
      >
        {tab === 'devices' ? <DevicesPanel /> : <ApiKeysPanel />}
      </Box>
    </Box>
  );
}
