import { EditOutlined as EditOutlinedIcon, VisibilityOutlined as VisibilityOutlinedIcon } from '@mui/icons-material';
import { Box, Stack, Tooltip, Typography } from '@mui/material';
import { EditButton, List, ShowButton, useListContext } from 'react-admin';

import type { UserRecord } from '../api/types';
import { UserStatus } from './UserStatus';

function UserRows() {
  const { data = [], isPending } = useListContext<UserRecord>();
  if (isPending) return <Typography sx={{ p: 3 }}>Loading users...</Typography>;
  if (data.length === 0) return <Typography sx={{ p: 3 }}>No users found.</Typography>;
  return (
    <Box role="table" aria-label="Users" sx={{ border: '1px solid #dce2e5', borderRadius: 1, overflow: 'hidden' }}>
      <Box role="row" sx={headerSx}>
        <Typography role="columnheader" variant="caption">Name</Typography>
        <Typography role="columnheader" variant="caption">Access</Typography>
        <Typography role="columnheader" variant="caption" sx={{ textAlign: 'right' }}>Actions</Typography>
      </Box>
      {data.map((record) => (
        <Box role="row" key={record.id} sx={rowSx}>
          <Box role="cell" data-label="Name">
            <Typography sx={{ fontWeight: 650 }}>{record.Name}</Typography>
            <Typography variant="caption" color="text.secondary">{record.Id}</Typography>
          </Box>
          <Box role="cell" data-label="Access">
            <UserStatus isAdministrator={record.Policy.IsAdministrator} isDisabled={record.Policy.IsDisabled} />
          </Box>
          <Stack role="cell" data-label="Actions" direction="row" sx={{ justifyContent: 'flex-end' }}>
            <Tooltip title="View user"><ShowButton record={record} label="" icon={<VisibilityOutlinedIcon />} /></Tooltip>
            <Tooltip title="Edit user"><EditButton record={record} label="" icon={<EditOutlinedIcon />} /></Tooltip>
          </Stack>
        </Box>
      ))}
    </Box>
  );
}

export function UserList() {
  return (
    <List<UserRecord>
      title="Users"
      perPage={25}
      sort={{ field: 'Name', order: 'ASC' }}
      exporter={false}
      sx={{
        '& .RaList-main': { maxWidth: 1120 },
        '& .RaList-actions': {
          minHeight: { xs: 0, sm: 'auto' },
          height: { xs: 0, sm: 'auto' },
          bgcolor: { xs: 'transparent', sm: 'background.paper' },
        },
        '& .RaList-actions .RaTopToolbar-root': {
          minHeight: { xs: 0, sm: 'auto' },
          height: { xs: 0, sm: 'auto' },
        },
      }}
    >
      <UserRows />
    </List>
  );
}

const headerSx = {
  display: { xs: 'none', sm: 'grid' },
  gridTemplateColumns: 'minmax(180px, 1fr) minmax(240px, 1fr) 112px',
  gap: 2,
  p: 1.5,
  bgcolor: '#eef1f2',
  color: 'text.secondary',
};

const rowSx = {
  display: 'grid',
  gridTemplateColumns: { xs: 'minmax(0, 1fr)', sm: 'minmax(180px, 1fr) minmax(240px, 1fr) 112px' },
  gap: { xs: 1.5, sm: 2 },
  alignItems: 'center',
  p: 1.5,
  borderTop: '1px solid #e5e9eb',
  '&:first-of-type': { borderTop: 0 },
  '& [role="cell"]': { minWidth: 0 },
  '& [role="cell"]::before': {
    display: { xs: 'block', sm: 'none' },
    content: 'attr(data-label)',
    mb: 0.5,
    color: 'text.secondary',
    fontSize: '0.75rem',
    fontWeight: 650,
  },
};
