import { Toast } from '@heroui/react';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useLocation } from 'react-router-dom';

import { defaultTestAuthProvider, renderWithAdmin } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
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
  type ScheduledTask,
  type TaskJob,
  type TaskSnapshot,
} from './taskApi';
import { TasksPage } from './TasksPage';

vi.mock('./taskApi', () => ({
  cancelScheduledTask: vi.fn(),
  discoverTitles: vi.fn(),
  expandItem: vi.fn(),
  fullScanRoot: vi.fn(),
  getTaskSnapshot: vi.fn(),
  indexMediaSources: vi.fn(),
  probeMedia: vi.fn(),
  resolveMetadata: vi.fn(),
  startScheduledTask: vi.fn(),
  validateStorage: vi.fn(),
}));

const snapshotMock = vi.mocked(getTaskSnapshot);
const startMock = vi.mocked(startScheduledTask);
const cancelMock = vi.mocked(cancelScheduledTask);
const validateMock = vi.mocked(validateStorage);
const discoverMock = vi.mocked(discoverTitles);
const expandMock = vi.mocked(expandItem);
const fullScanMock = vi.mocked(fullScanRoot);
const indexMock = vi.mocked(indexMediaSources);
const resolveMock = vi.mocked(resolveMetadata);
const probeMock = vi.mocked(probeMedia);
const taskId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const runningTaskId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f10';
const rootId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';
const jobId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f13';

const idleTask: ScheduledTask = {
  id: taskId,
  name: 'Scan Media Library',
  state: 'Idle',
  description: 'Scans libraries',
  category: 'Library',
  key: 'FullMediaScan',
};

const completedJob: TaskJob = {
  id: jobId,
  taskKind: 'FullMediaScan',
  scopeType: 'Library',
  scopeId: taskId,
  status: 'Completed',
  priority: 20,
  attemptCount: 1,
  createdAt: '2026-07-24T01:02:03Z',
  startedAt: '2026-07-24T01:02:04Z',
  completedAt: '2026-07-24T01:02:05Z',
};

const snapshot: TaskSnapshot = {
  scheduled: [idleTask],
  jobs: [completedJob],
  roots: [{
    key: `${taskId}:${rootId}`,
    libraryId: taskId,
    storageRootId: rootId,
    label: 'Movies',
  }],
};

const emptySnapshot: TaskSnapshot = { scheduled: [], jobs: [], roots: [] };

function renderTasks(authProvider = defaultTestAuthProvider) {
  return renderWithAdmin(
    <>
      <TasksPage />
      <AdminNotifications />
      <CurrentRoute />
    </>,
    { authProvider, initialEntries: ['/admin/tasks'], strict: true },
  );
}

function CurrentRoute() {
  const location = useLocation();
  return <span data-testid="current-route">{location.pathname}{location.search}</span>;
}

beforeEach(() => {
  for (const mock of [
    snapshotMock,
    startMock,
    cancelMock,
    validateMock,
    discoverMock,
    expandMock,
    fullScanMock,
    indexMock,
    resolveMock,
    probeMock,
  ]) mock.mockReset();
  snapshotMock.mockResolvedValue(snapshot);
  startMock.mockResolvedValue(undefined);
  cancelMock.mockResolvedValue(undefined);
  validateMock.mockResolvedValue([jobId]);
  discoverMock.mockResolvedValue([jobId]);
  resolveMock.mockResolvedValue([jobId]);
  expandMock.mockResolvedValue([jobId]);
  fullScanMock.mockResolvedValue([jobId]);
  indexMock.mockResolvedValue([jobId]);
  probeMock.mockResolvedValue([jobId]);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it('shows a stable skeleton during the initial snapshot request', () => {
  snapshotMock.mockReturnValue(new Promise(() => undefined));
  renderTasks();

  expect(screen.getByRole('status', { name: 'Loading tasks' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Reload tasks' })).toHaveAttribute('data-pending', 'true');
  expect(screen.getByRole('button', { name: 'Reload tasks' })).toHaveAttribute('aria-disabled', 'true');
});

it('renders explicit successful empty states and the root guidance', async () => {
  snapshotMock.mockResolvedValue(emptySnapshot);
  renderTasks();

  expect(await screen.findByText('No scheduled tasks are available.')).toBeVisible();
  expect(screen.getByText('No durable jobs have been submitted.')).toBeVisible();
  expect(screen.getByText('No storage roots are attached to a library.')).toBeVisible();
});

it('shows a safe initial error and retries without sending a danger toast', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  snapshotMock
    .mockRejectedValueOnce(new Error('private-task-detail'))
    .mockResolvedValueOnce(emptySnapshot);
  renderTasks();
  const user = userEvent.setup();

  expect(await screen.findByRole('heading', { name: 'Unable to load this content' })).toBeVisible();
  expect(screen.queryByText('private-task-detail')).not.toBeInTheDocument();
  expect(dangerToast).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Retry' }));
  expect(await screen.findByText('No scheduled tasks are available.')).toBeVisible();
});

it('retains valid data after a failed poll and cleans up the timer and request', async () => {
  vi.useFakeTimers();
  let activePollSignal: AbortSignal | undefined;
  snapshotMock
    .mockResolvedValueOnce(snapshot)
    .mockRejectedValueOnce(new Error('private-poll-detail'));
  const view = renderTasks();

  await act(async () => { await Promise.resolve(); });
  expect(screen.getByRole('list', { name: 'Scheduled tasks' })).toHaveTextContent('Scan Media Library');

  await act(async () => {
    vi.advanceTimersByTime(5_000);
    await Promise.resolve();
    await Promise.resolve();
  });
  expect(screen.getByText('Showing the last available data')).toBeVisible();
  expect(screen.getByRole('list', { name: 'Scheduled tasks' })).toHaveTextContent('Scan Media Library');
  expect(screen.queryByText('private-poll-detail')).not.toBeInTheDocument();

  snapshotMock.mockImplementationOnce((signal) => {
    activePollSignal = signal;
    return new Promise(() => undefined);
  });
  act(() => { vi.advanceTimersByTime(5_000); });
  const callsBeforeUnmount = snapshotMock.mock.calls.length;
  view.unmount();
  expect(activePollSignal?.aborted).toBe(true);
  vi.advanceTimersByTime(10_000);
  expect(snapshotMock).toHaveBeenCalledTimes(callsBeforeUnmount);
});

it('does not abort or restart a snapshot request that exceeds the polling interval', async () => {
  vi.useFakeTimers();
  let initialSignal: AbortSignal | undefined;
  let finishInitial: ((value: TaskSnapshot) => void) | undefined;
  snapshotMock
    .mockImplementationOnce((signal) => {
      initialSignal = signal;
      return new Promise((resolve) => { finishInitial = resolve; });
    })
    .mockResolvedValueOnce(snapshot);
  renderTasks();

  await act(async () => { await Promise.resolve(); });
  expect(snapshotMock).toHaveBeenCalledOnce();
  act(() => { vi.advanceTimersByTime(15_000); });
  expect(snapshotMock).toHaveBeenCalledOnce();
  expect(initialSignal?.aborted).toBe(false);

  await act(async () => {
    finishInitial?.(snapshot);
    await Promise.resolve();
  });
  act(() => { vi.advanceTimersByTime(5_000); });
  await act(async () => { await Promise.resolve(); });
  expect(snapshotMock).toHaveBeenCalledTimes(2);
});

it('keeps records visible and preserves reload focus while a manual refresh is pending', async () => {
  let finishReload: ((value: TaskSnapshot) => void) | undefined;
  snapshotMock
    .mockResolvedValueOnce(snapshot)
    .mockImplementationOnce(() => new Promise((resolve) => { finishReload = resolve; }));
  renderTasks();
  const user = userEvent.setup();

  const scheduled = await screen.findByRole('list', { name: 'Scheduled tasks' });
  await user.click(screen.getByRole('button', { name: 'Reload tasks' }));
  expect(screen.getByRole('button', { name: 'Reload tasks' })).toHaveAttribute('data-pending', 'true');
  expect(screen.getByRole('button', { name: 'Reload tasks' })).toHaveFocus();
  expect(screen.getByRole('status')).toHaveTextContent('Refreshing tasks');
  expect(scheduled).toHaveTextContent('Scan Media Library');

  finishReload?.(snapshot);
  await waitFor(() => {
    expect(screen.getByRole('button', { name: 'Reload tasks' })).not.toHaveAttribute('data-pending');
  });
});

it('starts idle scheduled work and refreshes authoritative state', async () => {
  const successToast = vi.spyOn(Toast.toast, 'success').mockReturnValue('start-toast');
  renderTasks();
  const user = userEvent.setup();
  const scheduled = await screen.findByRole('list', { name: 'Scheduled tasks' });

  await user.click(within(scheduled).getByRole('button', { name: 'Start Scan Media Library' }));

  expect(startMock).toHaveBeenCalledWith(taskId);
  await waitFor(() => { expect(snapshotMock).toHaveBeenCalledTimes(2); });
  expect(successToast).toHaveBeenCalledWith('Scheduled task started.', expect.any(Object));
});

it('confirms cancellation and keeps the dialog open with safe copy after failure', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('cancel-toast');
  const runningTask: ScheduledTask = { ...idleTask, id: runningTaskId, name: 'Refresh Guide', state: 'Running' };
  snapshotMock.mockResolvedValue({ ...snapshot, scheduled: [runningTask] });
  cancelMock.mockRejectedValue(new Error('private-cancel-detail'));
  renderTasks();
  const user = userEvent.setup();
  const scheduled = await screen.findByRole('list', { name: 'Scheduled tasks' });

  await user.click(within(scheduled).getByRole('button', { name: 'Cancel Refresh Guide' }));
  const dialog = screen.getByRole('dialog', { name: 'Cancel scheduled task' });
  expect(dialog).toHaveTextContent('Refresh Guide');
  expect(cancelMock).not.toHaveBeenCalled();
  await user.click(within(dialog).getByRole('button', { name: 'Cancel task' }));

  expect(cancelMock).toHaveBeenCalledWith(runningTaskId);
  expect(await within(dialog).findByText('Review the current state and try again.')).toBeVisible();
  expect(dangerToast).not.toHaveBeenCalled();
  expect(screen.queryByText('private-cancel-detail')).not.toBeInTheDocument();
});

it('closes a confirmed cancellation and refreshes the task snapshot after success', async () => {
  const runningTask: ScheduledTask = { ...idleTask, id: runningTaskId, name: 'Refresh Guide', state: 'Running' };
  snapshotMock
    .mockResolvedValueOnce({ ...snapshot, scheduled: [runningTask] })
    .mockResolvedValueOnce(snapshot);
  renderTasks();
  const user = userEvent.setup();
  const scheduled = await screen.findByRole('list', { name: 'Scheduled tasks' });

  await user.click(within(scheduled).getByRole('button', { name: 'Cancel Refresh Guide' }));
  const dialog = screen.getByRole('dialog', { name: 'Cancel scheduled task' });
  await user.click(within(dialog).getByRole('button', { name: 'Cancel task' }));

  expect(cancelMock).toHaveBeenCalledWith(runningTaskId);
  await waitFor(() => { expect(snapshotMock).toHaveBeenCalledTimes(2); });
  await waitFor(() => {
    expect(screen.queryByRole('dialog', { name: 'Cancel scheduled task' })).not.toBeInTheDocument();
  });
  await waitFor(() => {
    expect(screen.getByRole('heading', { name: 'Scheduled tasks' })).toHaveFocus();
  });
});

it('submits every root and item command with validated identifiers', async () => {
  renderTasks();
  const user = userEvent.setup();

  await screen.findByRole('list', { name: 'Scheduled tasks' });
  await user.click(screen.getByRole('button', { name: 'Validate storage' }));
  await user.click(screen.getByRole('button', { name: 'Discover titles' }));
  await user.click(screen.getByRole('button', { name: 'Full scan' }));

  expect(validateMock).toHaveBeenCalledWith(rootId);
  expect(discoverMock).toHaveBeenCalledWith(rootId);
  expect(fullScanMock).toHaveBeenCalledWith(taskId, rootId);

  const target = screen.getByRole('radiogroup', { name: 'Command target' });
  await user.click(within(target).getByRole('radio', { name: 'Catalog item' }));
  const item = screen.getByRole('textbox', { name: 'Catalog item ID' });
  await user.type(item, taskId);
  await user.click(screen.getByRole('button', { name: 'Resolve metadata' }));
  await user.click(screen.getByRole('button', { name: 'Expand item' }));
  await user.click(screen.getByRole('button', { name: 'Index sources' }));
  await user.click(screen.getByRole('button', { name: 'Probe media' }));

  expect(resolveMock).toHaveBeenCalledWith(taskId);
  expect(expandMock).toHaveBeenCalledWith(taskId);
  expect(indexMock).toHaveBeenCalledWith(taskId);
  expect(probeMock).toHaveBeenCalledWith(taskId);
});

it('switches manual command targets while preserving each target draft', async () => {
  snapshotMock.mockResolvedValue({
    ...snapshot,
    roots: [
      ...snapshot.roots,
      {
        key: `${taskId}:${runningTaskId}`,
        libraryId: taskId,
        storageRootId: runningTaskId,
        label: 'TV Shows',
      },
    ],
  });
  renderTasks();
  const user = userEvent.setup();

  await screen.findByRole('list', { name: 'Scheduled tasks' });
  const target = screen.getByRole('radiogroup', { name: 'Command target' });
  const rootTarget = within(target).getByRole('radio', { name: 'Library root' });
  const itemTarget = within(target).getByRole('radio', { name: 'Catalog item' });
  expect(rootTarget).toBeChecked();
  expect(screen.getByRole('button', { name: 'Full scan' })).toBeVisible();
  expect(screen.queryByRole('textbox', { name: 'Catalog item ID' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Movies Library root' }));
  await user.click(screen.getByRole('option', { name: 'TV Shows' }));

  await user.click(itemTarget);
  const item = screen.getByRole('textbox', { name: 'Catalog item ID' });
  await user.type(item, taskId);
  expect(screen.queryByRole('button', { name: 'Full scan' })).not.toBeInTheDocument();

  await user.click(rootTarget);
  expect(screen.getByRole('button', { name: 'Full scan' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'TV Shows Library root' })).toBeVisible();
  await user.click(itemTarget);
  expect(screen.getByRole('textbox', { name: 'Catalog item ID' })).toHaveValue(taskId);
});

it('isolates pending state per command and reports the durable job count', async () => {
  const successToast = vi.spyOn(Toast.toast, 'success').mockReturnValue('command-toast');
  let finishValidation: ((jobs: string[]) => void) | undefined;
  validateMock.mockReturnValue(new Promise((resolve) => { finishValidation = resolve; }));
  renderTasks();
  const user = userEvent.setup();

  await screen.findByRole('list', { name: 'Scheduled tasks' });
  await user.click(screen.getByRole('button', { name: 'Validate storage' }));
  const pendingButton = screen.getByRole('button', { name: 'Validate storage' });
  expect(pendingButton).toHaveAttribute('data-pending', 'true');
  expect(pendingButton).toHaveAttribute('aria-disabled', 'true');
  expect(pendingButton).toHaveFocus();
  expect(screen.getByRole('button', { name: 'Discover titles' })).toBeEnabled();
  await user.click(screen.getByRole('button', { name: 'Discover titles' }));
  expect(discoverMock).toHaveBeenCalledWith(rootId);

  finishValidation?.([jobId, taskId]);
  await waitFor(() => {
    expect(successToast).toHaveBeenCalledWith(
      'Storage validation submitted. 2 durable jobs accepted.',
      expect.any(Object),
    );
  });
});

it('renders exhaustive visible status tones and readable identifiers', async () => {
  const runningTask: ScheduledTask = { ...idleTask, id: runningTaskId, name: 'Refresh Guide', state: 'Running' };
  const statuses: TaskJob['status'][] = [
    'Pending',
    'Retrying',
    'Running',
    'Completed',
    'Cancelled',
    'Failed',
  ];
  const jobs = statuses.map((status, index): TaskJob => ({
    ...completedJob,
    id: `${jobId}-${String(index)}`,
    status,
    taskKind: `${status}MediaScan`,
  }));
  snapshotMock.mockResolvedValue({ ...snapshot, scheduled: [idleTask, runningTask], jobs });
  renderTasks();

  const scheduled = await screen.findByRole('list', { name: 'Scheduled tasks' });
  expect(within(scheduled).getByText('Idle').closest('[data-tone="neutral"]')).not.toBeNull();
  expect(within(scheduled).getByText('Running').closest('[data-tone="accent"]')).not.toBeNull();

  const jobsGrid = await screen.findByRole('grid', { name: 'Recent durable jobs' });
  await within(jobsGrid).findAllByRole('rowheader');
  const jobTones = new Map<string, string>([
    ['Pending', 'neutral'],
    ['Retrying', 'warning'],
    ['Running', 'accent'],
    ['Completed', 'success'],
    ['Cancelled', 'neutral'],
    ['Failed', 'danger'],
  ]);
  for (const [label, tone] of jobTones) {
    expect(within(jobsGrid).getByText(label).closest(`[data-tone="${tone}"]`)).not.toBeNull();
  }
  expect(screen.getByText('Pending Media Scan')).toBeVisible();
  expect(screen.getByText('PendingMediaScan')).toBeVisible();
});

it('redirects an initial 403 without showing local task errors or toasts', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  const checkError = vi.fn().mockRejectedValue({
    logoutUser: false,
    message: false,
    redirectTo: '/admin/access-denied',
  });
  snapshotMock.mockRejectedValue({ status: 403, message: 'private-auth-detail' });
  renderTasks({ ...defaultTestAuthProvider, checkError });

  await waitFor(() => { expect(checkError).toHaveBeenCalled(); });
  await waitFor(() => { expect(screen.getByTestId('current-route')).toHaveTextContent('/admin/access-denied'); });
  expect(screen.queryByRole('heading', { name: 'Unable to load this content' })).not.toBeInTheDocument();
  expect(dangerToast).not.toHaveBeenCalled();
  expect(screen.queryByText('private-auth-detail')).not.toBeInTheDocument();
});

it('redirects a 403 from manual refresh without replacing the current snapshot', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  const checkError = vi.fn().mockRejectedValue({
    logoutUser: false,
    message: false,
    redirectTo: '/admin/access-denied',
  });
  snapshotMock
    .mockResolvedValueOnce(snapshot)
    .mockRejectedValueOnce({ status: 403, message: 'private-refresh-auth-detail' });
  renderTasks({ ...defaultTestAuthProvider, checkError });
  const user = userEvent.setup();
  const scheduled = await screen.findByRole('list', { name: 'Scheduled tasks' });

  await user.click(screen.getByRole('button', { name: 'Reload tasks' }));

  await waitFor(() => { expect(screen.getByTestId('current-route')).toHaveTextContent('/admin/access-denied'); });
  expect(scheduled).toHaveTextContent('Scan Media Library');
  expect(dangerToast).not.toHaveBeenCalled();
  expect(screen.queryByText('private-refresh-auth-detail')).not.toBeInTheDocument();
});

it('redirects a 403 raised by background polling', async () => {
  vi.useFakeTimers();
  const checkError = vi.fn().mockRejectedValue({
    logoutUser: false,
    message: false,
    redirectTo: '/admin/access-denied',
  });
  snapshotMock
    .mockResolvedValueOnce(snapshot)
    .mockRejectedValueOnce({ status: 403, message: 'private-poll-auth-detail' });
  const view = renderTasks({ ...defaultTestAuthProvider, checkError });
  await act(async () => { await Promise.resolve(); });

  await act(async () => {
    vi.advanceTimersByTime(5_000);
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });

  expect(checkError).toHaveBeenCalled();
  expect(screen.getByTestId('current-route')).toHaveTextContent('/admin/access-denied');
  expect(screen.queryByText('private-poll-auth-detail')).not.toBeInTheDocument();
  view.unmount();
  act(() => { vi.advanceTimersByTime(0); });
});

it('logs out after a 401 task command without showing a command error toast', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  const logout = vi.fn().mockResolvedValue(undefined);
  const checkError = vi.fn().mockRejectedValue({ message: false });
  startMock.mockRejectedValue({ status: 401, message: 'private-command-auth-detail' });
  renderTasks({ ...defaultTestAuthProvider, checkError, logout });
  const user = userEvent.setup();
  const scheduled = await screen.findByRole('list', { name: 'Scheduled tasks' });

  await user.click(within(scheduled).getByRole('button', { name: 'Start Scan Media Library' }));

  await waitFor(() => { expect(checkError).toHaveBeenCalled(); });
  await waitFor(() => { expect(logout).toHaveBeenCalled(); });
  await waitFor(() => { expect(screen.getByTestId('current-route')).toHaveTextContent('/admin/login'); });
  expect(dangerToast).not.toHaveBeenCalled();
  expect(screen.queryByText('private-command-auth-detail')).not.toBeInTheDocument();
});

it('redirects a 403 cancellation and closes its confirmation without local feedback', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  const checkError = vi.fn().mockRejectedValue({
    logoutUser: false,
    message: false,
    redirectTo: '/admin/access-denied',
  });
  const runningTask: ScheduledTask = { ...idleTask, id: runningTaskId, name: 'Refresh Guide', state: 'Running' };
  snapshotMock.mockResolvedValue({ ...snapshot, scheduled: [runningTask] });
  cancelMock.mockRejectedValue({ status: 403, message: 'private-cancel-auth-detail' });
  renderTasks({ ...defaultTestAuthProvider, checkError });
  const user = userEvent.setup();
  const scheduled = await screen.findByRole('list', { name: 'Scheduled tasks' });

  await user.click(within(scheduled).getByRole('button', { name: 'Cancel Refresh Guide' }));
  const dialog = screen.getByRole('dialog', { name: 'Cancel scheduled task' });
  await user.click(within(dialog).getByRole('button', { name: 'Cancel task' }));

  await waitFor(() => { expect(screen.getByTestId('current-route')).toHaveTextContent('/admin/access-denied'); });
  await waitFor(() => {
    expect(screen.queryByRole('dialog', { name: 'Cancel scheduled task' })).not.toBeInTheDocument();
  });
  expect(dangerToast).not.toHaveBeenCalled();
  expect(screen.queryByText('private-cancel-auth-detail')).not.toBeInTheDocument();
});
