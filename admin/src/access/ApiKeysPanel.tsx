import {
  AddOutlined,
  ContentCopyOutlined,
  DeleteOutlineOutlined,
  RefreshOutlined,
  VisibilityOffOutlined,
  VisibilityOutlined,
} from '@mui/icons-material';
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

import type { ApiKeyInfo } from './apiKeyApi';
import { createApiKey, deleteApiKey, listApiKeys } from './apiKeyApi';
import { formatAccessDate } from './formatAccessDate';
import { ResponsiveTableCell } from './ResponsiveTableCell';
import { useAuthoritativeLoad } from './useAuthoritativeLoad';

const KEY_MASK = '****************';
type LoadResult = { records: ApiKeyInfo[] } | { error: unknown };
type BusyOperation = 'create' | 'delete' | null;

export function ApiKeysPanel() {
  const notify = useNotify();
  const [keys, setKeys] = useState<ApiKeyInfo[]>([]);
  const [busy, setBusy] = useState<BusyOperation>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [appName, setAppName] = useState('');
  const [deleting, setDeleting] = useState<ApiKeyInfo | null>(null);
  const [revealed, setRevealed] = useState<ReadonlySet<number>>(() => new Set());

  const resetReveals = useCallback(() => {
    setRevealed(new Set());
  }, []);

  const applyLoadResult = useCallback((result: LoadResult) => {
    if ('records' in result) setKeys(result.records);
    else notify('API keys could not be loaded.', { type: 'error' });
  }, [notify]);

  const { isMounted, loading, reload: loadKeys } = useAuthoritativeLoad(fetchApiKeys, applyLoadResult);

  const reload = useCallback(async () => {
    resetReveals();
    await loadKeys();
  }, [loadKeys, resetReveals]);

  const create = async () => {
    if (busy !== null || appName.trim().length === 0) return;
    setBusy('create');
    try {
      await createApiKey(appName);
      if (!isMounted()) return;
      notify('API key created.', { type: 'success' });
      setCreateOpen(false);
      setAppName('');
      await reload();
    } catch {
      if (isMounted()) notify('The API key could not be created.', { type: 'error' });
    } finally {
      if (isMounted()) setBusy(null);
    }
  };

  const remove = async () => {
    if (busy !== null || deleting === null) return;
    setBusy('delete');
    try {
      await deleteApiKey(deleting.accessToken);
      if (!isMounted()) return;
      notify(`API key deleted for ${deleting.appName}.`, { type: 'success' });
      setDeleting(null);
      await reload();
    } catch {
      if (isMounted()) notify('The API key could not be deleted.', { type: 'error' });
    } finally {
      if (isMounted()) setBusy(null);
    }
  };

  const toggleReveal = (id: number) => {
    setRevealed((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const copy = async (key: ApiKeyInfo) => {
    try {
      await navigator.clipboard.writeText(key.accessToken);
      notify(`API key copied for ${key.appName}.`, { type: 'success' });
    } catch {
      notify('The API key could not be copied.', { type: 'error' });
    }
  };

  return (
    <Stack spacing={2}>
      <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1} sx={{ alignItems: { sm: 'center' }, justifyContent: 'space-between' }}>
        <Typography component="h2" variant="h2">API Keys</Typography>
        <Stack direction="row" spacing={0.5}>
          <Tooltip title="Reload API keys">
            <span>
              <IconButton aria-label="Reload API keys" disabled={loading || busy !== null} onClick={() => void reload()}>
                <RefreshOutlined />
              </IconButton>
            </span>
          </Tooltip>
          <Button
            variant="contained"
            startIcon={<AddOutlined />}
            disabled={loading || busy !== null}
            onClick={() => { setCreateOpen(true); }}
          >
            Create API key
          </Button>
        </Stack>
      </Stack>
      <TableContainer sx={{ maxWidth: '100%', overflow: 'hidden' }}>
        <Table aria-label="API Keys" sx={{ tableLayout: 'fixed', width: '100%' }}>
          <TableHead sx={{ display: { xs: 'none', sm: 'table-header-group' } }}>
            <TableRow>
              <TableCell>Application</TableCell>
              <TableCell>Key</TableCell>
              <TableCell>Created</TableCell>
              <TableCell>Last used</TableCell>
              <TableCell align="right" sx={{ width: 144 }}>Actions</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {keys.map((key) => {
              const visible = revealed.has(key.id);
              return (
                <TableRow
                  key={key.id}
                  hover
                  sx={{ display: { xs: 'block', sm: 'table-row' }, borderBottom: { xs: '1px solid', sm: 0 }, borderColor: { xs: 'divider' }, py: { xs: 1 } }}
                >
                  <ResponsiveTableCell label="Application"><Typography component="span" sx={{ fontWeight: 650, overflowWrap: 'anywhere' }}>{key.appName}</Typography></ResponsiveTableCell>
                  <ResponsiveTableCell label="Key"><Typography component="code" sx={{ overflowWrap: 'anywhere' }}>{visible ? key.accessToken : KEY_MASK}</Typography></ResponsiveTableCell>
                  <ResponsiveTableCell label="Created">{formatAccessDate(key.dateCreated)}</ResponsiveTableCell>
                  <ResponsiveTableCell label="Last used">{key.dateLastActivity === null ? 'Never' : formatAccessDate(key.dateLastActivity)}</ResponsiveTableCell>
                  <TableCell align="right" sx={{ border: 0, display: { xs: 'block', sm: 'table-cell' }, px: { xs: 0, sm: 2 }, py: 1 }}>
                    <Stack direction="row" spacing={0.25} sx={{ justifyContent: 'flex-end' }}>
                      <Tooltip title={`${visible ? 'Hide' : 'Show'} key for ${key.appName}`}>
                        <span>
                          <IconButton
                            aria-label={`${visible ? 'Hide' : 'Show'} key for ${key.appName}`}
                            disabled={loading || busy !== null}
                            onClick={() => { toggleReveal(key.id); }}
                          >
                            {visible ? <VisibilityOffOutlined /> : <VisibilityOutlined />}
                          </IconButton>
                        </span>
                      </Tooltip>
                      <Tooltip title={`Copy key for ${key.appName}`}>
                        <span>
                          <IconButton aria-label={`Copy key for ${key.appName}`} disabled={loading || busy !== null} onClick={() => void copy(key)}>
                            <ContentCopyOutlined />
                          </IconButton>
                        </span>
                      </Tooltip>
                      <Tooltip title={`Delete key for ${key.appName}`}>
                        <span>
                          <IconButton aria-label={`Delete key for ${key.appName}`} disabled={loading || busy !== null} onClick={() => { setDeleting(key); }}>
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
      {loading && <CircularProgress size={28} aria-label="Loading API keys" sx={{ alignSelf: 'center' }} />}
      {!loading && keys.length === 0 && (
        <Typography color="text.secondary" sx={{ py: 3, textAlign: 'center' }}>No API keys exist.</Typography>
      )}

      <Dialog open={createOpen} onClose={() => { if (busy === null) setCreateOpen(false); }} fullWidth maxWidth="xs">
        <DialogTitle>Create API key</DialogTitle>
        <DialogContent>
          <TextField
            autoFocus
            fullWidth
            label="Application name"
            value={appName}
            disabled={busy !== null}
            onChange={(event) => { setAppName(event.target.value); }}
            slotProps={{ htmlInput: { maxLength: 256 } }}
            sx={{ mt: 1 }}
          />
        </DialogContent>
        <DialogActions>
          <Button disabled={busy !== null} onClick={() => { setCreateOpen(false); }}>Cancel</Button>
          <Button
            variant="contained"
            startIcon={busy === 'create' ? <CircularProgress color="inherit" size={18} /> : <AddOutlined />}
            disabled={busy !== null || appName.trim().length === 0}
            onClick={() => void create()}
          >
            Create key
          </Button>
        </DialogActions>
      </Dialog>

      <Dialog open={deleting !== null} onClose={() => { if (busy === null) setDeleting(null); }} fullWidth maxWidth="xs">
        <DialogTitle>Delete API key</DialogTitle>
        <DialogContent>
          <DialogContentText>Delete the API key for {deleting?.appName ?? 'this application'}?</DialogContentText>
        </DialogContent>
        <DialogActions>
          <Button disabled={busy !== null} onClick={() => { setDeleting(null); }}>Cancel</Button>
          <Button
            color="error"
            variant="contained"
            startIcon={busy === 'delete' ? <CircularProgress color="inherit" size={18} /> : <DeleteOutlineOutlined />}
            disabled={busy !== null}
            onClick={() => void remove()}
          >
            Delete key
          </Button>
        </DialogActions>
      </Dialog>
    </Stack>
  );
}

async function fetchApiKeys(signal?: AbortSignal): Promise<LoadResult> {
  try {
    return { records: await listApiKeys(signal) };
  } catch (error: unknown) {
    return { error };
  }
}
