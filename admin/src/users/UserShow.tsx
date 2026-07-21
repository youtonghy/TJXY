import { Box, Divider, Stack, Typography } from '@mui/material';
import { Show, useRecordContext } from 'react-admin';

import type { UserRecord } from '../api/types';
import { UserStatus } from './UserStatus';

function UserDetails() {
  const record = useRecordContext<UserRecord>();
  if (record === undefined) return null;
  return (
    <Stack spacing={2} sx={{ maxWidth: 720, p: 3 }}>
      <Box><Typography variant="caption" color="text.secondary">Name</Typography><Typography>{record.Name}</Typography></Box>
      <Divider />
      <Box><Typography variant="caption" color="text.secondary">User ID</Typography><Typography sx={{ overflowWrap: 'anywhere' }}>{record.Id}</Typography></Box>
      <Divider />
      <Box><Typography variant="caption" color="text.secondary">Access</Typography><UserStatus isAdministrator={record.Policy.IsAdministrator} isDisabled={record.Policy.IsDisabled} /></Box>
      <Divider />
      <Box><Typography variant="caption" color="text.secondary">Password</Typography><Typography>{record.HasConfiguredPassword ? 'Configured' : 'Not configured'}</Typography></Box>
    </Stack>
  );
}

export function UserShow() {
  return <Show<UserRecord> title="User"><UserDetails /></Show>;
}
