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
  Typography,
} from '@mui/material';
import { useEffect, useState } from 'react';
import { Title, useNotify } from 'react-admin';

import { uniqueChoices } from './directoryChoices';
import type { GoogleDriveChoice, GoogleOAuthStart, LibraryOption, StorageBindingResult } from './googleDriveApi';
import {
  bindOneDrive,
  listLibraries,
  listOneDriveDirectories,
  startOneDriveOAuth,
} from './googleDriveApi';

type BusyOperation = 'start' | 'verify' | 'browse' | 'more' | 'bind' | null;

export function OneDrivePage() {
  const notify = useNotify();
  const [libraries, setLibraries] = useState<LibraryOption[]>([]);
  const [librariesPending, setLibrariesPending] = useState(true);
  const [targetLibraryId, setTargetLibraryId] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [oauth, setOAuth] = useState<GoogleOAuthStart | null>(null);
  const [authorized, setAuthorized] = useState(false);
  const [path, setPath] = useState<GoogleDriveChoice[]>([]);
  const [directories, setDirectories] = useState<GoogleDriveChoice[]>([]);
  const [nextDirectoryPage, setNextDirectoryPage] = useState<string | null>(null);
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
        if (active) notifyError(notify, error, 'Libraries could not be loaded.');
      })
      .finally(() => {
        if (active) setLibrariesPending(false);
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
      const result = await startOneDriveOAuth(targetLibraryId);
      setOAuth(result);
      const popup = window.open(result.authorizationUrl, 'tjxy-onedrive-oauth', 'noopener,noreferrer');
      if (popup === null) notify('The authorization window was blocked.', { type: 'warning' });
    } catch (error: unknown) {
      notifyError(notify, error, 'Microsoft authorization could not start.');
    } finally {
      setBusy(null);
    }
  };

  const loadFolder = async (folder: GoogleDriveChoice, nextPath: GoogleDriveChoice[]) => {
    if (oauth === null || busy !== null) return;
    setBusy('browse');
    try {
      const page = await listOneDriveDirectories(oauth.state, {
        ...(folder.id === 'root' ? {} : { parentId: folder.id }),
      });
      setPath(nextPath);
      setDirectories(page.items);
      setNextDirectoryPage(page.nextPageToken);
    } catch (error: unknown) {
      notifyError(notify, error, 'The folder could not be opened.');
    } finally {
      setBusy(null);
    }
  };

  const verifyAuthorization = async () => {
    if (oauth === null || busy !== null) return;
    setBusy('verify');
    try {
      const page = await listOneDriveDirectories(oauth.state);
      setAuthorized(true);
      setPath([{ id: 'root', name: 'OneDrive' }]);
      setDirectories(page.items);
      setNextDirectoryPage(page.nextPageToken);
    } catch (error: unknown) {
      if (errorCategory(error) === 'conflict') {
        notify('Microsoft authorization has not completed yet.', { type: 'warning' });
      } else {
        notifyError(notify, error, 'Microsoft authorization could not be verified.');
      }
    } finally {
      setBusy(null);
    }
  };

  const loadMoreDirectories = async () => {
    if (
      oauth === null
      || currentFolder === undefined
      || nextDirectoryPage === null
      || busy !== null
    ) return;
    setBusy('more');
    try {
      const page = await listOneDriveDirectories(oauth.state, {
        ...(currentFolder.id === 'root' ? {} : { parentId: currentFolder.id }),
        pageToken: nextDirectoryPage,
      });
      setDirectories((current) => uniqueChoices([...current, ...page.items]));
      setNextDirectoryPage(page.nextPageToken);
    } catch (error: unknown) {
      notifyError(notify, error, 'More folders could not be loaded.');
    } finally {
      setBusy(null);
    }
  };

  const bindCurrentFolder = async () => {
    if (oauth === null || currentFolder === undefined || busy !== null) return;
    setBusy('bind');
    try {
      const result = await bindOneDrive(oauth.state, {
        displayName,
        rootObjectId: currentFolder.id,
      });
      setBinding(result);
      notify('OneDrive was added.', { type: 'success' });
    } catch (error: unknown) {
      notifyError(notify, error, 'OneDrive could not be added.');
    } finally {
      setBusy(null);
    }
  };

  return (
    <Box sx={{ maxWidth: 960, p: { xs: 2, sm: 3 } }}>
      <Title title="Storage accounts" />
      <Stack direction="row" spacing={1.5} sx={{ alignItems: 'center', mb: 3 }}>
        <CloudOutlined color="primary" />
        <Typography component="h1" variant="h1">OneDrive</Typography>
      </Stack>

      <Stack spacing={2.5} divider={<Divider flexItem />}>
        <Stack spacing={2}>
          <Typography component="h2" variant="h2">Authorization</Typography>
          <FormControl fullWidth disabled={librariesPending || busy !== null || oauth !== null}>
            <InputLabel id="onedrive-target-library-label">Target library</InputLabel>
            <Select
              labelId="onedrive-target-library-label"
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
              Authorize OneDrive
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
            <Typography component="h2" variant="h2">Folder</Typography>
            <Breadcrumbs separator={<ChevronRight fontSize="small" />}>
              {path.map((folder, index) => (
                <Button
                  key={folder.id}
                  size="small"
                  disabled={busy !== null || index === path.length - 1}
                  onClick={() => void loadFolder(folder, path.slice(0, index + 1))}
                >
                  {folder.name}
                </Button>
              ))}
            </Breadcrumbs>
            <List dense disablePadding sx={{ border: '1px solid #dce2e5', borderRadius: 1 }}>
              {directories.map((folder) => (
                <ListItemButton
                  key={folder.id}
                  disabled={busy !== null}
                  onClick={() => void loadFolder(folder, [...path, folder])}
                >
                  <ListItemIcon sx={{ minWidth: 36 }}><FolderOutlined fontSize="small" /></ListItemIcon>
                  <ListItemText primary={folder.name} />
                </ListItemButton>
              ))}
              {directories.length === 0 && (
                <ListItemText
                  sx={{ px: 2, py: 1 }}
                  primary={nextDirectoryPage === null
                    ? 'No subfolders'
                    : 'No folders on this page.'}
                />
              )}
            </List>
            {nextDirectoryPage !== null && (
              <Button
                variant="outlined"
                aria-label="Load more folders"
                startIcon={busy === 'more' ? <CircularProgress size={18} /> : <RefreshOutlined />}
                disabled={busy !== null}
                onClick={() => void loadMoreDirectories()}
              >
                Load more
              </Button>
            )}
            <TextField
              label="Storage name"
              value={displayName}
              disabled={busy !== null}
              onChange={(event) => {
                setDisplayName(event.target.value);
              }}
              slotProps={{ htmlInput: { maxLength: 256 } }}
            />
            <Button
              variant="contained"
              startIcon={busy === 'bind' ? <CircularProgress size={18} color="inherit" /> : <CheckCircleOutlineOutlined />}
              disabled={currentFolder === undefined || displayName.trim().length === 0 || busy !== null}
              onClick={() => void bindCurrentFolder()}
            >
              Add OneDrive
            </Button>
          </Stack>
        )}

        {binding !== null && (
          <Alert severity="success">OneDrive is ready and active.</Alert>
        )}
      </Stack>
    </Box>
  );
}

function errorCategory(error: unknown): string | undefined {
  return typeof error === 'object' && error !== null && 'category' in error
    ? String(error.category)
    : undefined;
}

function notifyError(
  notify: ReturnType<typeof useNotify>,
  error: unknown,
  fallback: string,
): void {
  const message = error instanceof Error && error.message.length > 0 ? error.message : fallback;
  notify(message, { type: 'error' });
}
