import {
  AddOutlined,
  DeleteOutlineOutlined,
  EditOutlined,
  FolderCopyOutlined,
  PushPinOutlined,
  RefreshOutlined,
} from '@mui/icons-material';
import {
  Alert,
  Box,
  Button,
  Checkbox,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControl,
  FormControlLabel,
  IconButton,
  InputLabel,
  MenuItem,
  Paper,
  Select,
  Stack,
  Switch,
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
import { useCallback, useEffect, useState } from 'react';
import { Title, useNotify } from 'react-admin';

import { HybridCandidatesDialog } from './HybridCandidatesDialog';
import type {
  CreateLibraryRequest,
  EffectiveLibraryPolicy,
  ExpansionPolicy,
  LibraryCollectionType,
  LibraryOption,
  MetadataPolicy,
  ObjectSelectionScope,
  ProbePolicy,
  ScanProfile,
  UpdateLibraryPolicyRequest,
} from './libraryApi';
import {
  createLibrary,
  deleteLibrary,
  listLibraries,
  renameLibrary,
  updateLibraryPolicy,
} from './libraryApi';

type BusyOperation = 'create' | 'rename' | 'policy' | 'delete' | null;
type LibraryLoadResult = { records: LibraryOption[] } | { error: unknown };

const PROFILES: ScanProfile[] = ['Full', 'Lazy', 'Hybrid', 'Manual'];
const COLLECTION_TYPES: { value: LibraryCollectionType; label: string }[] = [
  { value: 'mixed', label: 'Mixed content' },
  { value: 'movies', label: 'Movies' },
  { value: 'tvshows', label: 'TV shows' },
  { value: 'music', label: 'Music' },
  { value: 'homevideos', label: 'Home videos' },
];

export function LibrariesPage() {
  const notify = useNotify();
  const [libraries, setLibraries] = useState<LibraryOption[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<BusyOperation>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [selected, setSelected] = useState<LibraryOption | null>(null);
  const [candidateLibrary, setCandidateLibrary] = useState<LibraryOption | null>(null);

  const applyLoadResult = useCallback((result: LibraryLoadResult) => {
    if ('records' in result) setLibraries(result.records);
    else notifyError(notify, result.error, 'Libraries could not be loaded.');
    setLoading(false);
  }, [notify]);

  const reload = useCallback(async () => {
    setLoading(true);
    applyLoadResult(await fetchLibraryResult());
  }, [applyLoadResult]);

  useEffect(() => {
    const abort = new AbortController();
    void fetchLibraryResult(abort.signal).then((result) => {
      if (!abort.signal.aborted) applyLoadResult(result);
    });
    return () => {
      abort.abort();
    };
  }, [applyLoadResult]);

  const create = async (request: CreateLibraryRequest): Promise<boolean> => {
    if (busy !== null) return false;
    setBusy('create');
    try {
      await createLibrary(request);
      notify('Library created.', { type: 'success' });
      setCreateOpen(false);
      await reload();
      return true;
    } catch (error: unknown) {
      notifyError(notify, error, 'The library could not be created.');
      return false;
    } finally {
      setBusy(null);
    }
  };

  const rename = async (library: LibraryOption, nextName: string): Promise<void> => {
    if (busy !== null) return;
    setBusy('rename');
    try {
      await renameLibrary(library.name, nextName);
      notify('Library renamed.', { type: 'success' });
      setSelected(null);
      await reload();
    } catch (error: unknown) {
      notifyError(notify, error, 'The library could not be renamed.');
    } finally {
      setBusy(null);
    }
  };

  const savePolicy = async (request: UpdateLibraryPolicyRequest): Promise<void> => {
    if (busy !== null) return;
    setBusy('policy');
    try {
      await updateLibraryPolicy(request);
      notify('Scan policy saved.', { type: 'success' });
      setSelected(null);
      await reload();
    } catch (error: unknown) {
      if (errorCategory(error) === 'conflict') {
        notify('The library changed on the server. Latest settings were reloaded.', { type: 'warning' });
        setSelected(null);
        await reload();
      } else {
        notifyError(notify, error, 'The scan policy could not be saved.');
      }
    } finally {
      setBusy(null);
    }
  };

  const remove = async (library: LibraryOption): Promise<void> => {
    if (busy !== null) return;
    setBusy('delete');
    try {
      await deleteLibrary(library.name);
      notify('Library deleted.', { type: 'success' });
      setSelected(null);
      await reload();
    } catch (error: unknown) {
      notifyError(notify, error, 'The library could not be deleted.');
    } finally {
      setBusy(null);
    }
  };

  return (
    <Box sx={{
      boxSizing: 'border-box',
      maxWidth: { xs: 'calc(100vw - 16px)', sm: 1200 },
      minWidth: 0,
      width: '100%',
      p: { xs: 2, sm: 3 },
    }}>
      <Title title="Libraries" />
      <Stack
        direction={{ xs: 'column', sm: 'row' }}
        spacing={1.5}
        sx={{ alignItems: { sm: 'center' }, justifyContent: 'space-between', mb: 3 }}
      >
        <Stack direction="row" spacing={1.5} sx={{ alignItems: 'center' }}>
          <FolderCopyOutlined color="primary" />
          <Typography component="h1" variant="h1">Libraries</Typography>
        </Stack>
        <Stack direction="row" spacing={1}>
          <Tooltip title="Reload libraries">
            <span>
              <IconButton
                aria-label="Reload libraries"
                disabled={loading || busy !== null}
                onClick={() => void reload()}
              >
                <RefreshOutlined />
              </IconButton>
            </span>
          </Tooltip>
          <Button
            variant="contained"
            startIcon={<AddOutlined />}
            disabled={busy !== null}
            onClick={() => { setCreateOpen(true); }}
          >
            Add library
          </Button>
        </Stack>
      </Stack>

      <TableContainer
        component={Paper}
        variant="outlined"
        sx={{ maxWidth: '100%', overflowX: 'auto', width: '100%' }}
      >
        <Table aria-label="Libraries" sx={{ minWidth: 900 }}>
          <TableHead>
            <TableRow>
              <TableCell>Name</TableCell>
              <TableCell>Type</TableCell>
              <TableCell>Status</TableCell>
              <TableCell>Scan profile</TableCell>
              <TableCell>Effective policy</TableCell>
              <TableCell align="right">Roots</TableCell>
              <TableCell align="right">Actions</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {libraries.map((library) => (
              <TableRow key={library.id} hover>
                <TableCell component="th" scope="row">{library.name}</TableCell>
                <TableCell>{collectionLabel(library.collectionType)}</TableCell>
                <TableCell>{library.enabled ? 'Enabled' : 'Disabled'}</TableCell>
                <TableCell>{library.scanProfile}</TableCell>
                <TableCell>
                  <Typography variant="body2">
                    {library.objectSelectionScope} / {library.metadataPolicy} / {library.expansionPolicy} / {library.probePolicy}
                  </Typography>
                </TableCell>
                <TableCell align="right">{library.locations.length}</TableCell>
                <TableCell align="right">
                  <Stack direction="row" spacing={0.5} sx={{ justifyContent: 'flex-end' }}>
                    <Tooltip title={`Manage background candidates for ${library.name}`}>
                      <IconButton
                        aria-label={`Manage background candidates for ${library.name}`}
                        disabled={busy !== null}
                        onClick={() => { setCandidateLibrary(library); }}
                      >
                        <PushPinOutlined />
                      </IconButton>
                    </Tooltip>
                    <Tooltip title={`Edit ${library.name}`}>
                      <IconButton
                        aria-label={`Edit ${library.name}`}
                        disabled={busy !== null}
                        onClick={() => { setSelected(library); }}
                      >
                        <EditOutlined />
                      </IconButton>
                    </Tooltip>
                  </Stack>
                </TableCell>
              </TableRow>
            ))}
            {!loading && libraries.length === 0 && (
              <TableRow>
                <TableCell colSpan={7}>
                  <Typography color="text.secondary" sx={{ py: 3, textAlign: 'center' }}>
                    No libraries are configured.
                  </Typography>
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
        {loading && (
          <Stack sx={{ alignItems: 'center', py: 4 }}>
            <CircularProgress size={28} aria-label="Loading libraries" />
          </Stack>
        )}
      </TableContainer>

      <CreateLibraryDialog
        open={createOpen}
        pending={busy === 'create'}
        onClose={() => { setCreateOpen(false); }}
        onCreate={create}
      />
      {selected !== null && (
        <EditLibraryDialog
          key={`${selected.id}-${String(selected.profileVersion)}`}
          library={selected}
          busy={busy}
          onClose={() => { setSelected(null); }}
          onRename={rename}
          onSavePolicy={savePolicy}
          onDelete={remove}
        />
      )}
      {candidateLibrary !== null && (
        <HybridCandidatesDialog
          key={candidateLibrary.id}
          library={candidateLibrary}
          onClose={() => { setCandidateLibrary(null); }}
        />
      )}
    </Box>
  );
}

function CreateLibraryDialog({
  open,
  pending,
  onClose,
  onCreate,
}: {
  open: boolean;
  pending: boolean;
  onClose: () => void;
  onCreate: (request: CreateLibraryRequest) => Promise<boolean>;
}) {
  const [name, setName] = useState('');
  const [collectionType, setCollectionType] = useState<LibraryCollectionType>('mixed');
  const [scanProfile, setScanProfile] = useState<ScanProfile>('Lazy');
  const [enabled, setEnabled] = useState(true);

  const close = () => {
    if (pending) return;
    setName('');
    setCollectionType('mixed');
    setScanProfile('Lazy');
    setEnabled(true);
    onClose();
  };

  return (
    <Dialog open={open} onClose={close} fullWidth maxWidth="sm">
      <DialogTitle>Add library</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <TextField
            autoFocus
            label="Library name"
            value={name}
            disabled={pending}
            onChange={(event) => { setName(event.target.value); }}
            slotProps={{ htmlInput: { maxLength: 256 } }}
          />
          <FormControl fullWidth disabled={pending}>
            <InputLabel id="create-library-type-label">Content type</InputLabel>
            <Select
              labelId="create-library-type-label"
              label="Content type"
              value={collectionType}
              onChange={(event) => { setCollectionType(event.target.value); }}
            >
              {COLLECTION_TYPES.map((option) => (
                <MenuItem key={option.value} value={option.value}>{option.label}</MenuItem>
              ))}
            </Select>
          </FormControl>
          <ProfileSelect
            labelId="create-library-profile-label"
            value={scanProfile}
            disabled={pending}
            onChange={setScanProfile}
          />
          <FormControlLabel
            control={<Switch checked={enabled} onChange={(_, checked) => { setEnabled(checked); }} />}
            label="Enabled"
            disabled={pending}
          />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={close} disabled={pending}>Cancel</Button>
        <Button
          variant="contained"
          disabled={pending || name.trim().length === 0}
          startIcon={pending ? <CircularProgress size={18} color="inherit" /> : <AddOutlined />}
          onClick={() => void onCreate({ name, collectionType, enabled, scanProfile })}
        >
          Create library
        </Button>
      </DialogActions>
    </Dialog>
  );
}

function EditLibraryDialog({
  library,
  busy,
  onClose,
  onRename,
  onSavePolicy,
  onDelete,
}: {
  library: LibraryOption;
  busy: BusyOperation;
  onClose: () => void;
  onRename: (library: LibraryOption, nextName: string) => Promise<void>;
  onSavePolicy: (request: UpdateLibraryPolicyRequest) => Promise<void>;
  onDelete: (library: LibraryOption) => Promise<void>;
}) {
  const [name, setName] = useState(library.name);
  const [enabled, setEnabled] = useState(library.enabled);
  const [scanProfile, setScanProfile] = useState<ScanProfile>(library.scanProfile);
  const [advanced, setAdvanced] = useState(false);
  const [policy, setPolicy] = useState<EffectiveLibraryPolicy>({
    objectSelectionScope: library.objectSelectionScope,
    metadataPolicy: library.metadataPolicy,
    expansionPolicy: library.expansionPolicy,
    probePolicy: library.probePolicy,
  });
  const [deleteArmed, setDeleteArmed] = useState(false);
  const pending = busy !== null;

  return (
    <Dialog
      open
      onClose={() => {
        if (!pending) onClose();
      }}
      fullWidth
      maxWidth="md"
    >
      <DialogTitle>Edit {library.name}</DialogTitle>
      <DialogContent>
        <Stack spacing={3} sx={{ pt: 1 }}>
          <Stack spacing={1.5}>
            <Typography component="h2" variant="h3">Identity</Typography>
            <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1}>
              <TextField
                fullWidth
                label="Library name"
                value={name}
                disabled={pending}
                onChange={(event) => { setName(event.target.value); }}
                slotProps={{ htmlInput: { maxLength: 256 } }}
              />
              <Button
                variant="outlined"
                disabled={pending || name.trim().length === 0 || name.trim() === library.name}
                onClick={() => void onRename(library, name)}
              >
                Rename
              </Button>
            </Stack>
            <Typography color="text.secondary" variant="body2">
              {collectionLabel(library.collectionType)} · {library.locations.length} storage root(s)
            </Typography>
          </Stack>

          <Stack spacing={1.5}>
            <Typography component="h2" variant="h3">Scan policy</Typography>
            <FormControlLabel
              control={<Switch checked={enabled} onChange={(_, checked) => { setEnabled(checked); }} />}
              label="Enabled"
              disabled={pending}
            />
            <ProfileSelect
              labelId="edit-library-profile-label"
              value={scanProfile}
              disabled={pending}
              onChange={setScanProfile}
            />
            <FormControlLabel
              control={<Checkbox checked={advanced} onChange={(_, checked) => { setAdvanced(checked); }} />}
              label="Override effective policy"
              disabled={pending}
            />
            {advanced && (
              <PolicyOverrides policy={policy} disabled={pending} onChange={setPolicy} />
            )}
            {!advanced && (
              <Typography color="text.secondary" variant="body2">
                Current effective policy: {library.objectSelectionScope} / {library.metadataPolicy} / {library.expansionPolicy} / {library.probePolicy}
              </Typography>
            )}
            <Button
              variant="contained"
              disabled={pending}
              onClick={() => void onSavePolicy({
                id: library.id,
                enabled,
                scanProfile,
                profileVersion: library.profileVersion,
                ...(advanced ? { effectivePolicy: policy } : {}),
              })}
            >
              Save scan policy
            </Button>
          </Stack>

          <Stack spacing={1.5}>
            <Typography component="h2" variant="h3">Delete library</Typography>
            {deleteArmed && (
              <Alert severity="warning">
                This removes the library mapping. Shared catalog and storage entities are preserved.
              </Alert>
            )}
            <Stack direction="row" spacing={1}>
              {deleteArmed && (
                <Button disabled={pending} onClick={() => { setDeleteArmed(false); }}>Cancel delete</Button>
              )}
              <Button
                color="error"
                variant={deleteArmed ? 'contained' : 'outlined'}
                startIcon={<DeleteOutlineOutlined />}
                disabled={pending}
                onClick={() => {
                  if (deleteArmed) void onDelete(library);
                  else setDeleteArmed(true);
                }}
              >
                {deleteArmed ? 'Confirm delete' : 'Delete library'}
              </Button>
            </Stack>
          </Stack>
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={pending}>Close</Button>
      </DialogActions>
    </Dialog>
  );
}

function ProfileSelect({
  labelId,
  value,
  disabled,
  onChange,
}: {
  labelId: string;
  value: ScanProfile;
  disabled: boolean;
  onChange: (value: ScanProfile) => void;
}) {
  return (
    <FormControl fullWidth disabled={disabled}>
      <InputLabel id={labelId}>Scan profile</InputLabel>
      <Select
        labelId={labelId}
        label="Scan profile"
        value={value}
        onChange={(event) => { onChange(event.target.value); }}
      >
        {PROFILES.map((profile) => <MenuItem key={profile} value={profile}>{profile}</MenuItem>)}
      </Select>
    </FormControl>
  );
}

function PolicyOverrides({
  policy,
  disabled,
  onChange,
}: {
  policy: EffectiveLibraryPolicy;
  disabled: boolean;
  onChange: (policy: EffectiveLibraryPolicy) => void;
}) {
  return (
    <Box sx={{ display: 'grid', gap: 2, gridTemplateColumns: { xs: '1fr', sm: '1fr 1fr' } }}>
      <PolicySelect
        label="Object selection"
        value={policy.objectSelectionScope}
        options={['all_synced_objects', 'title_layer', 'library_roots']}
        disabled={disabled}
        onChange={(value) => { onChange({ ...policy, objectSelectionScope: value as ObjectSelectionScope }); }}
      />
      <PolicySelect
        label="Metadata"
        value={policy.metadataPolicy}
        options={['full', 'basic', 'none']}
        disabled={disabled}
        onChange={(value) => { onChange({ ...policy, metadataPolicy: value as MetadataPolicy }); }}
      />
      <PolicySelect
        label="Expansion"
        value={policy.expansionPolicy}
        options={['eager', 'on_browse', 'background', 'manual']}
        disabled={disabled}
        onChange={(value) => { onChange({ ...policy, expansionPolicy: value as ExpansionPolicy }); }}
      />
      <PolicySelect
        label="Probe"
        value={policy.probePolicy}
        options={['eager', 'on_playback', 'manual']}
        disabled={disabled}
        onChange={(value) => { onChange({ ...policy, probePolicy: value as ProbePolicy }); }}
      />
    </Box>
  );
}

function PolicySelect({
  label,
  value,
  options,
  disabled,
  onChange,
}: {
  label: string;
  value: string;
  options: string[];
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  const labelId = `${label.toLowerCase().replaceAll(' ', '-')}-label`;
  return (
    <FormControl fullWidth disabled={disabled}>
      <InputLabel id={labelId}>{label}</InputLabel>
      <Select
        labelId={labelId}
        label={label}
        value={value}
        onChange={(event) => { onChange(event.target.value); }}
      >
        {options.map((option) => <MenuItem key={option} value={option}>{option}</MenuItem>)}
      </Select>
    </FormControl>
  );
}

function collectionLabel(value: string): string {
  return COLLECTION_TYPES.find((option) => option.value === value)?.label ?? value;
}

async function fetchLibraryResult(signal?: AbortSignal): Promise<LibraryLoadResult> {
  try {
    return { records: await listLibraries(signal) };
  } catch (error: unknown) {
    return { error };
  }
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
