import { DeleteOutlineOutlined, EditOutlined, RefreshOutlined, SaveOutlined } from '@mui/icons-material';
import {
  Button,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  IconButton,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TextField,
  Tooltip,
  Typography,
} from '@mui/material';
import { useCallback, useState } from 'react';
import { useNotify } from 'react-admin';

import type { DeviceInfo } from './deviceApi';
import { deleteDevice, listDevices, updateDeviceName } from './deviceApi';
import { formatAccessDate } from './formatAccessDate';
import { ResponsiveTableCell } from './ResponsiveTableCell';
import { useAuthoritativeLoad } from './useAuthoritativeLoad';

type LoadResult = { records: DeviceInfo[] } | { error: unknown };
type BusyOperation = 'rename' | 'revoke' | null;

export function DevicesPanel() {
  const notify = useNotify();
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [busy, setBusy] = useState<BusyOperation>(null);
  const [editing, setEditing] = useState<DeviceInfo | null>(null);
  const [customName, setCustomName] = useState('');
  const [revoking, setRevoking] = useState<DeviceInfo | null>(null);

  const applyLoadResult = useCallback((result: LoadResult) => {
    if ('records' in result) setDevices(result.records);
    else notify('Devices could not be loaded.', { type: 'error' });
  }, [notify]);

  const { isMounted, loading, reload } = useAuthoritativeLoad(fetchDevices, applyLoadResult);

  const saveName = async () => {
    if (editing === null || busy !== null) return;
    setBusy('rename');
    try {
      const nextName = customName.trim();
      await updateDeviceName(editing.id, nextName.length === 0 ? null : nextName);
      if (!isMounted()) return;
      notify('Device name saved.', { type: 'success' });
      setEditing(null);
      await reload();
    } catch {
      if (isMounted()) notify('The device name could not be saved.', { type: 'error' });
    } finally {
      if (isMounted()) setBusy(null);
    }
  };

  const revoke = async () => {
    if (revoking === null || busy !== null) return;
    setBusy('revoke');
    try {
      await deleteDevice(revoking.id);
      if (!isMounted()) return;
      notify('Device access revoked.', { type: 'success' });
      setRevoking(null);
      await reload();
    } catch {
      if (isMounted()) notify('Device access could not be revoked.', { type: 'error' });
    } finally {
      if (isMounted()) setBusy(null);
    }
  };

  return (
    <Stack spacing={2}>
      <Stack direction="row" sx={{ alignItems: 'center', justifyContent: 'space-between' }}>
        <Typography component="h2" variant="h2">Devices</Typography>
        <Tooltip title="Reload devices">
          <span>
            <IconButton
              aria-label="Reload devices"
              disabled={loading || busy !== null}
              onClick={() => void reload()}
            >
              <RefreshOutlined />
            </IconButton>
          </span>
        </Tooltip>
      </Stack>
      <TableContainer sx={{ maxWidth: '100%', overflow: 'hidden' }}>
        <Table aria-label="Devices" sx={{ tableLayout: 'fixed', width: '100%' }}>
          <TableHead sx={{ display: { xs: 'none', sm: 'table-header-group' } }}>
            <TableRow>
              <TableCell>Device</TableCell>
              <TableCell>Application</TableCell>
              <TableCell>Last user</TableCell>
              <TableCell>Last activity</TableCell>
              <TableCell align="right" sx={{ width: 104 }}>Actions</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {devices.map((device) => {
              const label = effectiveName(device);
              return (
                <TableRow
                  key={device.id}
                  hover
                  sx={{
                    display: { xs: 'block', sm: 'table-row' },
                    borderBottom: { xs: '1px solid', sm: 0 },
                    borderColor: { xs: 'divider' },
                    py: { xs: 1 },
                  }}
                >
                  <ResponsiveTableCell label="Device">
                    <Typography component="span" sx={{ fontWeight: 650, overflowWrap: 'anywhere' }}>{label}</Typography>
                    {device.customName !== null && (
                      <Typography color="text.secondary" component="span" sx={{ overflowWrap: 'anywhere' }} variant="body2">{device.name}</Typography>
                    )}
                  </ResponsiveTableCell>
                  <ResponsiveTableCell label="Application">{device.appName} {device.appVersion}</ResponsiveTableCell>
                  <ResponsiveTableCell label="Last user">{device.lastUserName}</ResponsiveTableCell>
                  <ResponsiveTableCell label="Last activity">{formatAccessDate(device.dateLastActivity)}</ResponsiveTableCell>
                  <TableCell
                    align="right"
                    sx={{ border: 0, display: { xs: 'block', sm: 'table-cell' }, px: { xs: 0, sm: 2 }, py: 1 }}
                  >
                    <Stack direction="row" spacing={0.5} sx={{ justifyContent: 'flex-end' }}>
                      <Tooltip title={`Edit ${label}`}>
                        <span>
                          <IconButton
                            aria-label={`Edit ${label}`}
                            disabled={loading || busy !== null}
                            onClick={() => {
                              setEditing(device);
                              setCustomName(device.customName ?? '');
                            }}
                          >
                            <EditOutlined />
                          </IconButton>
                        </span>
                      </Tooltip>
                      <Tooltip title={`Revoke ${label}`}>
                        <span>
                          <IconButton
                            aria-label={`Revoke ${label}`}
                            disabled={loading || busy !== null}
                            onClick={() => { setRevoking(device); }}
                          >
                            <DeleteOutlineOutlined />
                          </IconButton>
                        </span>
                      </Tooltip>
                    </Stack>
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      </TableContainer>
      {loading && <CircularProgress size={28} aria-label="Loading devices" sx={{ alignSelf: 'center' }} />}
      {!loading && devices.length === 0 && (
        <Typography color="text.secondary" sx={{ py: 3, textAlign: 'center' }}>No devices are active.</Typography>
      )}

      <Dialog open={editing !== null} onClose={() => { if (busy === null) setEditing(null); }} fullWidth maxWidth="xs">
        <DialogTitle>Edit {editing === null ? 'device' : effectiveName(editing)}</DialogTitle>
        <DialogContent>
          <TextField
            autoFocus
            fullWidth
            label="Custom device name"
            value={customName}
            disabled={busy !== null}
            onChange={(event) => { setCustomName(event.target.value); }}
            slotProps={{ htmlInput: { maxLength: 256 } }}
            sx={{ mt: 1 }}
          />
        </DialogContent>
        <DialogActions>
          <Button disabled={busy !== null} onClick={() => { setEditing(null); }}>Cancel</Button>
          <Button
            variant="contained"
            startIcon={busy === 'rename' ? <CircularProgress color="inherit" size={18} /> : <SaveOutlined />}
            disabled={busy !== null}
            onClick={() => void saveName()}
          >
            Save device name
          </Button>
        </DialogActions>
      </Dialog>

      <Dialog open={revoking !== null} onClose={() => { if (busy === null) setRevoking(null); }} fullWidth maxWidth="xs">
        <DialogTitle>Revoke device</DialogTitle>
        <DialogContent>
          <DialogContentText>
            Revoke all active sessions for {revoking === null ? 'this device' : effectiveName(revoking)}?
          </DialogContentText>
        </DialogContent>
        <DialogActions>
          <Button disabled={busy !== null} onClick={() => { setRevoking(null); }}>Cancel</Button>
          <Button
            color="error"
            variant="contained"
            startIcon={busy === 'revoke' ? <CircularProgress color="inherit" size={18} /> : <DeleteOutlineOutlined />}
            disabled={busy !== null}
            onClick={() => void revoke()}
          >
            Revoke device
          </Button>
        </DialogActions>
      </Dialog>
    </Stack>
  );
}

function effectiveName(device: DeviceInfo): string {
  const customName = device.customName?.trim();
  return customName === undefined || customName.length === 0 ? device.name : customName;
}

async function fetchDevices(signal?: AbortSignal): Promise<LoadResult> {
  try {
    return { records: await listDevices(signal) };
  } catch (error: unknown) {
    return { error };
  }
}
