import {
  AccountTreeOutlined,
  CancelOutlined,
  PlayArrowOutlined,
  PlaylistAddCheckOutlined,
  RefreshOutlined,
  StorageOutlined,
  TaskAltOutlined,
  TravelExploreOutlined,
} from '@mui/icons-material';
import {
  Alert,
  Box,
  Button,
  Chip,
  CircularProgress,
  FormControl,
  IconButton,
  InputLabel,
  MenuItem,
  Paper,
  Select,
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
import { useCallback, useEffect, useRef, useState } from 'react';
import { Title, useNotify } from 'react-admin';

import type { ScheduledTask, TaskJob, TaskSnapshot } from './taskApi';
import {
  cancelScheduledTask,
  discoverTitles,
  expandItem,
  fullScanRoot,
  getTaskSnapshot,
  indexMediaSources,
  probeMedia,
  resolveMetadata,
  startScheduledTask,
  validateStorage,
} from './taskApi';

const POLL_INTERVAL_MS = 5_000;
type BusyOperation = string | null;

export function TasksPage() {
  const notify = useNotify();
  const [snapshot, setSnapshot] = useState<TaskSnapshot>({ scheduled: [], jobs: [], roots: [] });
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<BusyOperation>(null);
  const [selectedRoot, setSelectedRoot] = useState('');
  const [itemId, setItemId] = useState('');
  const requestVersion = useRef(0);

  const applySnapshot = useCallback((next: TaskSnapshot) => {
    setSnapshot(next);
    setSelectedRoot((current) => {
      if (next.roots.some((root) => root.key === current)) return current;
      return next.roots[0]?.key ?? '';
    });
    setLoading(false);
  }, []);

  const load = useCallback(async (signal?: AbortSignal) => {
    const version = requestVersion.current + 1;
    requestVersion.current = version;
    try {
      const next = await getTaskSnapshot(signal);
      if (version === requestVersion.current) applySnapshot(next);
    } catch (error: unknown) {
      if (signal?.aborted === true || version !== requestVersion.current) return;
      notifyError(notify, error, 'Tasks could not be loaded.');
      setLoading(false);
    }
  }, [applySnapshot, notify]);

  const selectedRootOption = snapshot.roots.find((root) => root.key === selectedRoot);

  useEffect(() => {
    const abort = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      await load(abort.signal);
      if (!abort.signal.aborted) timer = setTimeout(() => { void poll(); }, POLL_INTERVAL_MS);
    };
    void poll();
    return () => {
      requestVersion.current += 1;
      abort.abort();
      if (timer !== undefined) clearTimeout(timer);
    };
  }, [load]);

  const run = async (
    operation: string,
    command: () => Promise<unknown>,
    success: string,
  ): Promise<void> => {
    if (busy !== null) return;
    setBusy(operation);
    try {
      const jobIds = await command();
      const suffix = Array.isArray(jobIds) && jobIds.length > 0
        ? ` ${String(jobIds.length)} durable job${jobIds.length === 1 ? '' : 's'} accepted.`
        : '';
      notify(`${success}${suffix}`, { type: 'success' });
      await load();
    } catch (error: unknown) {
      notifyError(notify, error, 'The task command could not be completed.');
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
      <Title title="Tasks" />
      <Stack
        direction="row"
        spacing={1.5}
        sx={{ alignItems: 'center', justifyContent: 'space-between', mb: 3 }}
      >
        <Stack direction="row" spacing={1.5} sx={{ alignItems: 'center' }}>
          <TaskAltOutlined color="primary" />
          <Typography component="h1" variant="h1">Tasks</Typography>
        </Stack>
        <Tooltip title="Reload tasks">
          <span>
            <IconButton
              aria-label="Reload tasks"
              disabled={loading || busy !== null}
              onClick={() => { setLoading(true); void load(); }}
            >
              <RefreshOutlined />
            </IconButton>
          </span>
        </Tooltip>
      </Stack>

      <SectionHeading title="Scheduled tasks" />
      <ScheduledTasksTable
        tasks={snapshot.scheduled}
        loading={loading}
        busy={busy}
        onRun={(task) => void run(
          `scheduled-${task.id}`,
          task.state === 'Idle'
            ? () => startScheduledTask(task.id)
            : () => cancelScheduledTask(task.id),
          task.state === 'Idle' ? 'Scheduled task started.' : 'Scheduled task cancelled.',
        )}
      />

      <SectionHeading title="Manual commands" />
      <Box sx={{ display: 'grid', gap: 2, gridTemplateColumns: { xs: '1fr', md: '1fr 1fr' }, mb: 4 }}>
        <Paper variant="outlined" sx={{ p: 2 }}>
          <Stack spacing={2}>
            <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
              <StorageOutlined color="action" />
              <Typography component="h2" variant="h6">Library root</Typography>
            </Stack>
            {snapshot.roots.length === 0 ? (
              <Alert severity="info">No storage roots are attached to a library.</Alert>
            ) : (
              <FormControl fullWidth disabled={busy !== null}>
                <InputLabel id="task-root-label">Library root</InputLabel>
                <Select
                  labelId="task-root-label"
                  label="Library root"
                  value={selectedRoot}
                  onChange={(event) => { setSelectedRoot(event.target.value); }}
                >
                  {snapshot.roots.map((root) => (
                    <MenuItem key={root.key} value={root.key}>{root.label}</MenuItem>
                  ))}
                </Select>
              </FormControl>
            )}
            <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1}>
              <Button
                startIcon={<StorageOutlined />}
                disabled={selectedRoot.length === 0 || busy !== null}
                onClick={() => void run(
                  'validate-storage',
                  () => validateStorage(selectedRootOption?.storageRootId ?? ''),
                  'Storage validation submitted.',
                )}
              >
                Validate storage
              </Button>
              <Button
                startIcon={<TravelExploreOutlined />}
                disabled={selectedRoot.length === 0 || busy !== null}
                onClick={() => void run(
                  'discover-titles',
                  () => discoverTitles(selectedRootOption?.storageRootId ?? ''),
                  'Title discovery submitted.',
                )}
              >
                Discover titles
              </Button>
              <Button
                startIcon={<PlayArrowOutlined />}
                disabled={selectedRootOption === undefined || busy !== null}
                onClick={() => void run(
                  'full-scan-root',
                  () => fullScanRoot(
                    selectedRootOption?.libraryId ?? '',
                    selectedRootOption?.storageRootId ?? '',
                  ),
                  'Full scan submitted.',
                )}
              >
                Full scan
              </Button>
            </Stack>
          </Stack>
        </Paper>

        <Paper variant="outlined" sx={{ p: 2 }}>
          <Stack spacing={2}>
            <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
              <TravelExploreOutlined color="action" />
              <Typography component="h2" variant="h6">Catalog item</Typography>
            </Stack>
            <TextField
              label="Catalog item ID"
              value={itemId}
              disabled={busy !== null}
              onChange={(event) => { setItemId(event.target.value); }}
              slotProps={{ htmlInput: { maxLength: 64 } }}
            />
            <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1}>
              <Button
                disabled={!isUuid(itemId) || busy !== null}
                onClick={() => void run(
                  'resolve-metadata',
                  () => resolveMetadata(itemId),
                  'Metadata resolution submitted.',
                )}
              >
                Resolve metadata
              </Button>
              <Button
                startIcon={<AccountTreeOutlined />}
                disabled={!isUuid(itemId) || busy !== null}
                onClick={() => void run(
                  'expand-item',
                  () => expandItem(itemId),
                  'Item expansion submitted.',
                )}
              >
                Expand item
              </Button>
              <Button
                startIcon={<PlaylistAddCheckOutlined />}
                disabled={!isUuid(itemId) || busy !== null}
                onClick={() => void run(
                  'index-media-sources',
                  () => indexMediaSources(itemId),
                  'Source indexing submitted.',
                )}
              >
                Index sources
              </Button>
              <Button
                disabled={!isUuid(itemId) || busy !== null}
                onClick={() => void run(
                  'probe-media',
                  () => probeMedia(itemId),
                  'Media probe submitted.',
                )}
              >
                Probe media
              </Button>
            </Stack>
          </Stack>
        </Paper>
      </Box>

      <SectionHeading title="Recent durable jobs" />
      <RecentJobsTable jobs={snapshot.jobs} loading={loading} />
    </Box>
  );
}

function ScheduledTasksTable({
  tasks,
  loading,
  busy,
  onRun,
}: {
  tasks: ScheduledTask[];
  loading: boolean;
  busy: BusyOperation;
  onRun: (task: ScheduledTask) => void;
}) {
  return (
    <TableContainer component={Paper} variant="outlined" sx={{ maxWidth: '100%', mb: 4, overflowX: 'auto' }}>
      <Table aria-label="Scheduled tasks" sx={{ minWidth: 620 }}>
        <TableHead><TableRow>
          <TableCell>Name</TableCell><TableCell>Category</TableCell><TableCell>Status</TableCell><TableCell align="right">Action</TableCell>
        </TableRow></TableHead>
        <TableBody>
          {tasks.map((task) => (
            <TableRow key={task.id} hover>
              <TableCell component="th" scope="row">
                <Typography variant="body2" sx={{ fontWeight: 600 }}>{task.name}</Typography>
                <Typography variant="caption" color="text.secondary">{task.description}</Typography>
              </TableCell>
              <TableCell>{task.category}</TableCell>
              <TableCell><StatusChip status={task.state} /></TableCell>
              <TableCell align="right">
                <Button
                  size="small"
                  color={task.state === 'Running' ? 'error' : 'primary'}
                  startIcon={task.state === 'Running' ? <CancelOutlined /> : <PlayArrowOutlined />}
                  disabled={busy !== null}
                  onClick={() => { onRun(task); }}
                >
                  {task.state === 'Running' ? 'Cancel' : 'Start'}
                </Button>
              </TableCell>
            </TableRow>
          ))}
          {!loading && tasks.length === 0 && <EmptyRow columns={4} message="No scheduled tasks are available." />}
        </TableBody>
      </Table>
      {loading && <LoadingRows label="Loading scheduled tasks" />}
    </TableContainer>
  );
}

function RecentJobsTable({ jobs, loading }: { jobs: TaskJob[]; loading: boolean }) {
  return (
    <TableContainer component={Paper} variant="outlined" sx={{ maxWidth: '100%', overflowX: 'auto' }}>
      <Table aria-label="Recent durable jobs" sx={{ minWidth: 880 }}>
        <TableHead><TableRow>
          <TableCell>Task</TableCell><TableCell>Scope</TableCell><TableCell>Status</TableCell><TableCell align="right">Attempts</TableCell><TableCell>Created</TableCell><TableCell>Finished</TableCell>
        </TableRow></TableHead>
        <TableBody>
          {jobs.map((job) => (
            <TableRow key={job.id} hover>
              <TableCell>{job.taskKind}</TableCell>
              <TableCell>
                <Typography variant="body2">{job.scopeType}</Typography>
                <Typography variant="caption" color="text.secondary">{job.scopeId}</Typography>
              </TableCell>
              <TableCell><StatusChip status={job.status} /></TableCell>
              <TableCell align="right">{job.attemptCount}</TableCell>
              <TableCell>{formatDate(job.createdAt)}</TableCell>
              <TableCell>{formatDate(job.completedAt)}</TableCell>
            </TableRow>
          ))}
          {!loading && jobs.length === 0 && <EmptyRow columns={6} message="No durable jobs have been submitted." />}
        </TableBody>
      </Table>
      {loading && <LoadingRows label="Loading recent jobs" />}
    </TableContainer>
  );
}

function SectionHeading({ title }: { title: string }) {
  return <Typography component="h2" variant="h6" sx={{ mb: 1.5 }}>{title}</Typography>;
}

function StatusChip({ status }: { status: ScheduledTask['state'] | TaskJob['status'] }) {
  const color = status === 'Completed' ? 'success'
    : status === 'Running' || status === 'Retrying' ? 'info'
      : status === 'Failed' || status === 'Cancelled' ? 'error'
        : 'default';
  return <Chip label={status} color={color} size="small" variant="outlined" />;
}

function EmptyRow({ columns, message }: { columns: number; message: string }) {
  return <TableRow><TableCell colSpan={columns}><Typography color="text.secondary" sx={{ py: 3, textAlign: 'center' }}>{message}</Typography></TableCell></TableRow>;
}

function LoadingRows({ label }: { label: string }) {
  return <Stack sx={{ alignItems: 'center', py: 3 }}><CircularProgress size={28} aria-label={label} /></Stack>;
}

function formatDate(value: string | null): string {
  return value === null ? 'Not finished' : new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium', timeStyle: 'short',
  }).format(new Date(value));
}

function isUuid(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(value.trim());
}

function notifyError(
  notify: ReturnType<typeof useNotify>,
  error: unknown,
  fallback: string,
): void {
  const message = error instanceof Error && error.message.length > 0 ? error.message : fallback;
  notify(message, { type: 'error' });
}
