import {
  CheckCircleOutlineOutlined,
  ChevronRight,
  CloudOutlined,
  FolderOutlined,
  LaunchOutlined,
  RefreshOutlined,
} from '@mui/icons-material';
import {
  Alert,
  Box,
  Breadcrumbs,
  Button,
  CircularProgress,
  Divider,
  FormControl,
  InputLabel,
  List,
  ListItemButton,
  ListItemIcon,
  ListItemText,
  MenuItem,
  Select,
  Stack,
  TextField,
  ToggleButton,
  ToggleButtonGroup,
  Typography,
} from '@mui/material';
import { useEffect, useState } from 'react';
import { Title, useNotify } from 'react-admin';

import type {
  GoogleDriveChoice,
  GoogleDriveScope,
  GoogleOAuthStart,
  LibraryOption,
  StorageBindingResult,
} from './googleDriveApi';
import {
  bindGoogleDrive,
  listGoogleDirectories,
  listLibraries,
  listSharedDrives,
  startGoogleDriveOAuth,
} from './googleDriveApi';

type BusyOperation = 'start' | 'verify' | 'browse' | 'more' | 'bind' | null;

type FolderLocation = GoogleDriveChoice;

export function GoogleDrivePage() {
  const notify = useNotify();
  const [libraries, setLibraries] = useState<LibraryOption[]>([]);
  const [librariesPending, setLibrariesPending] = useState(true);
  const [targetLibraryId, setTargetLibraryId] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [oauth, setOAuth] = useState<GoogleOAuthStart | null>(null);
  const [authorized, setAuthorized] = useState(false);
  const [scope, setScope] = useState<GoogleDriveScope>('MyDrive');
  const [sharedDrives, setSharedDrives] = useState<GoogleDriveChoice[]>([]);
  const [nextSharedPage, setNextSharedPage] = useState<string | null>(null);
  const [sharedDriveId, setSharedDriveId] = useState('');
  const [path, setPath] = useState<FolderLocation[]>([]);
  const [directories, setDirectories] = useState<GoogleDriveChoice[]>([]);
  const [busy, setBusy] = useState<BusyOperation>(null);
  const [binding, setBinding] = useState<StorageBindingResult | null>(null);

  const currentFolder = path.at(-1);
  useEffect(() => {
    const abort = new AbortController();
    let active = true;
    void listLibraries(abort.signal)
      .then((records) => {
        if (!active) return;
        setLibraries(records);
        const initial = records.find((library) => library.enabled) ?? records[0];
        if (initial !== undefined) {
          setTargetLibraryId(initial.id);
          setDisplayName(initial.name);
        }
      })
      .catch((error: unknown) => {
        if (!active) return;
        notifyError(notify, error, 'Libraries could not be loaded.');
      })
      .finally(() => {
        if (!active) return;
        setLibrariesPending(false);
      });
    return () => {
      active = false;
      abort.abort();
    };
  }, [notify]);

  const startAuthorization = async () => {
    if (busy !== null) return;
    setBusy('start');
    setAuthorized(false);
    setBinding(null);
    try {
      const result = await startGoogleDriveOAuth(targetLibraryId);
      setOAuth(result);
      const popup = window.open(result.authorizationUrl, 'tjxy-google-oauth', 'noopener,noreferrer');
      if (popup === null) notify('The authorization window was blocked.', { type: 'warning' });
    } catch (error: unknown) {
      notifyError(notify, error, 'Google authorization could not start.');
    } finally {
      setBusy(null);
    }
  };

  const verifyAuthorization = async () => {
    if (oauth === null || busy !== null) return;
    setBusy('verify');
    try {
      const [drives, folders] = await Promise.all([
        listSharedDrives(oauth.state),
        listGoogleDirectories(oauth.state, { scope: 'MyDrive' }),
      ]);
      setAuthorized(true);
      setScope('MyDrive');
      setSharedDrives(drives.items);
      setNextSharedPage(drives.nextPageToken);
      setSharedDriveId(drives.items[0]?.id ?? '');
      setPath([{ id: 'root', name: 'My Drive' }]);
      setDirectories(folders);
    } catch (error: unknown) {
      if (errorCategory(error) === 'conflict') {
        notify('Google authorization has not completed yet.', { type: 'warning' });
      } else {
        notifyError(notify, error, 'Google authorization could not be verified.');
      }
    } finally {
      setBusy(null);
    }
  };

  const loadFolder = async (
    targetScope: GoogleDriveScope,
    targetSharedDriveId: string | undefined,
    folder: FolderLocation,
    nextPath: FolderLocation[],
  ) => {
    if (oauth === null || busy !== null) return;
    setBusy('browse');
    try {
      const folders = await listGoogleDirectories(oauth.state, {
        scope: targetScope,
        ...(targetSharedDriveId === undefined ? {} : { sharedDriveId: targetSharedDriveId }),
        ...(folder.id === 'root' ? {} : { parentId: folder.id }),
      });
      setPath(nextPath);
      setDirectories(folders);
    } catch (error: unknown) {
      notifyError(notify, error, 'The folder could not be opened.');
    } finally {
      setBusy(null);
    }
  };

  const changeScope = (nextScope: GoogleDriveScope | null) => {
    if (nextScope === null || nextScope === scope || busy !== null) return;
    setScope(nextScope);
    if (nextScope === 'MyDrive') {
      void loadFolder('MyDrive', undefined, { id: 'root', name: 'My Drive' }, [
        { id: 'root', name: 'My Drive' },
      ]);
      return;
    }
    const drive = sharedDrives.find((item) => item.id === sharedDriveId) ?? sharedDrives[0];
    if (drive === undefined) {
      notify('No Shared Drives are available.', { type: 'info' });
      return;
    }
    setSharedDriveId(drive.id);
    void loadFolder('SharedDrive', drive.id, drive, [drive]);
  };

  const changeSharedDrive = (driveId: string) => {
    const drive = sharedDrives.find((item) => item.id === driveId);
    if (drive === undefined || busy !== null) return;
    setSharedDriveId(drive.id);
    void loadFolder('SharedDrive', drive.id, drive, [drive]);
  };

  const loadMoreSharedDrives = async () => {
    if (oauth === null || nextSharedPage === null || busy !== null) return;
    setBusy('more');
    try {
      const page = await listSharedDrives(oauth.state, nextSharedPage);
      setSharedDrives((current) => uniqueChoices([...current, ...page.items]));
      setNextSharedPage(page.nextPageToken);
    } catch (error: unknown) {
      notifyError(notify, error, 'More Shared Drives could not be loaded.');
    } finally {
      setBusy(null);
    }
  };

  const bindCurrentFolder = async () => {
    if (oauth === null || currentFolder === undefined || busy !== null) return;
    setBusy('bind');
    try {
      const result = await bindGoogleDrive(oauth.state, {
        scope,
        displayName,
        rootObjectId: currentFolder.id,
        ...(scope === 'SharedDrive' ? { sharedDriveId } : {}),
      });
      setBinding(result);
      notify('Google Drive was added.', { type: 'success' });
    } catch (error: unknown) {
      notifyError(notify, error, 'Google Drive could not be added.');
    } finally {
      setBusy(null);
    }
  };

  return (
    <Box sx={{ maxWidth: 960, p: { xs: 2, sm: 3 } }}>
      <Title title="Storage accounts" />
      <Stack direction="row" spacing={1.5} sx={{ alignItems: 'center', mb: 3 }}>
        <CloudOutlined color="primary" />
        <Typography component="h1" variant="h1">Google Drive</Typography>
      </Stack>

      <Stack spacing={2.5} divider={<Divider flexItem />}>
        <Stack spacing={2}>
          <Typography component="h2" variant="h2">Authorization</Typography>
          <FormControl fullWidth disabled={librariesPending || busy !== null || oauth !== null}>
            <InputLabel id="target-library-label">Target library</InputLabel>
            <Select
              labelId="target-library-label"
              label="Target library"
              value={targetLibraryId}
              onChange={(event) => {
                const id = event.target.value;
                setTargetLibraryId(id);
                const library = libraries.find((item) => item.id === id);
                if (library !== undefined) setDisplayName(library.name);
              }}
            >
              {libraries.map((library) => (
                <MenuItem key={library.id} value={library.id} disabled={!library.enabled}>
                  {library.name}{library.enabled ? '' : ' (disabled)'}
                </MenuItem>
              ))}
            </Select>
          </FormControl>
          {librariesPending && <CircularProgress size={24} aria-label="Loading libraries" />}
          <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1}>
            <Button
              variant="contained"
              startIcon={busy === 'start' ? <CircularProgress size={18} color="inherit" /> : <LaunchOutlined />}
              disabled={targetLibraryId.length === 0 || busy !== null || oauth !== null}
              onClick={() => void startAuthorization()}
            >
              Authorize Google Drive
            </Button>
            {oauth !== null && !authorized && (
              <Button
                variant="outlined"
                startIcon={busy === 'verify' ? <CircularProgress size={18} /> : <RefreshOutlined />}
                disabled={busy !== null}
                onClick={() => void verifyAuthorization()}
              >
                Check authorization
              </Button>
            )}
          </Stack>
        </Stack>

        {authorized && binding === null && (
          <Stack spacing={2}>
            <Typography component="h2" variant="h2">Drive and folder</Typography>
            <ToggleButtonGroup
              exclusive
              size="small"
              value={scope}
              onChange={(_, value: GoogleDriveScope | null) => {
                changeScope(value);
              }}
              aria-label="Drive scope"
            >
              <ToggleButton value="MyDrive">My Drive</ToggleButton>
              <ToggleButton value="SharedDrive">Shared Drive</ToggleButton>
            </ToggleButtonGroup>
            {scope === 'SharedDrive' && (
              <Stack
                direction={{ xs: 'column', sm: 'row' }}
                spacing={1}
                sx={{ alignItems: { sm: 'center' } }}
              >
                <FormControl fullWidth>
                  <InputLabel id="shared-drive-label">Shared Drive</InputLabel>
                  <Select
                    labelId="shared-drive-label"
                    label="Shared Drive"
                    value={sharedDriveId}
                    disabled={busy !== null}
                    onChange={(event) => {
                      changeSharedDrive(event.target.value);
                    }}
                  >
                    {sharedDrives.map((drive) => (
                      <MenuItem key={drive.id} value={drive.id}>{drive.name}</MenuItem>
                    ))}
                  </Select>
                </FormControl>
                {nextSharedPage !== null && (
                  <Button
                    variant="outlined"
                    startIcon={busy === 'more' ? <CircularProgress size={18} /> : <RefreshOutlined />}
                    disabled={busy !== null}
                    onClick={() => void loadMoreSharedDrives()}
                  >
                    Load more
                  </Button>
                )}
              </Stack>
            )}
            <Breadcrumbs aria-label="Current folder">
              {path.map((folder, index) => (
                <Button
                  key={`${folder.id}-${String(index)}`}
                  size="small"
                  disabled={busy !== null || index === path.length - 1}
                  onClick={() => void loadFolder(
                    scope,
                    scope === 'SharedDrive' ? sharedDriveId : undefined,
                    folder,
                    path.slice(0, index + 1),
                  )}
                >
                  {folder.name}
                </Button>
              ))}
            </Breadcrumbs>
            <Box sx={{ border: '1px solid #dce2e5', borderRadius: 1, minHeight: 120, overflow: 'hidden' }}>
              {busy === 'browse' ? (
                <Stack sx={{ alignItems: 'center', py: 4 }}><CircularProgress size={24} /></Stack>
              ) : directories.length === 0 ? (
                <Typography color="text.secondary" sx={{ p: 2 }}>No child folders.</Typography>
              ) : (
                <List disablePadding aria-label="Folders">
                  {directories.map((folder, index) => (
                    <ListItemButton
                      key={folder.id}
                      divider={index < directories.length - 1}
                      aria-label={`Open ${folder.name}`}
                      onClick={() => void loadFolder(
                        scope,
                        scope === 'SharedDrive' ? sharedDriveId : undefined,
                        folder,
                        [...path, folder],
                      )}
                    >
                      <ListItemIcon><FolderOutlined /></ListItemIcon>
                      <ListItemText primary={folder.name} />
                      <ChevronRight />
                    </ListItemButton>
                  ))}
                </List>
              )}
            </Box>
            <TextField
              label="Display name"
              value={displayName}
              onChange={(event) => {
                setDisplayName(event.target.value);
              }}
              fullWidth
              slotProps={{ htmlInput: { maxLength: 2048 } }}
            />
            <Button
              variant="contained"
              startIcon={busy === 'bind' ? <CircularProgress size={18} color="inherit" /> : <CheckCircleOutlineOutlined />}
              disabled={busy !== null || currentFolder === undefined || displayName.trim().length === 0}
              onClick={() => void bindCurrentFolder()}
            >
              Bind this folder
            </Button>
          </Stack>
        )}

        {binding !== null && (
          <Alert severity="success" icon={<CheckCircleOutlineOutlined />}>
            <Typography sx={{ fontWeight: 700 }}>
              {binding.restartRequired ? 'Restart required' : 'Ready'}
            </Typography>
            <Typography variant="body2">Initial sync job: {binding.initialSyncJobId}</Typography>
          </Alert>
        )}
      </Stack>
    </Box>
  );
}

function uniqueChoices(items: GoogleDriveChoice[]): GoogleDriveChoice[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    if (seen.has(item.id)) return false;
    seen.add(item.id);
    return true;
  });
}

function errorCategory(error: unknown): string | undefined {
  if (typeof error !== 'object' || error === null || !('category' in error)) return undefined;
  return typeof error.category === 'string' ? error.category : undefined;
}

function notifyError(
  notify: ReturnType<typeof useNotify>,
  error: unknown,
  fallback: string,
): void {
  const message = error instanceof Error && error.message.length > 0 ? error.message : fallback;
  notify(message, { type: 'error' });
}
