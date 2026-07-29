import { Toast } from '@heroui/react';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

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
    </>,
    { authProvider, initialEntries: ['/admin/tasks'] },
  );
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
  expect(screen.getByRole('button', { name: 'Reload tasks' })).toBeDisabled();
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

it('keeps records visible and disables reload while a manual refresh is pending', async () => {
  let finishReload: ((value: TaskSnapshot) => void) | undefined;
  snapshotMock
    .mockResolvedValueOnce(snapshot)
    .mockImplementationOnce(() => new Promise((resolve) => { finishReload = resolve; }));
  renderTasks();
  const user = userEvent.setup();

  const scheduled = await screen.findByRole('list', { name: 'Scheduled tasks' });
  await user.click(screen.getByRole('button', { name: 'Reload tasks' }));
  expect(screen.getByRole('button', { name: 'Reload tasks' })).toBeDisabled();
  expect(screen.getByRole('status')).toHaveTextContent('Refreshing tasks');
  expect(scheduled).toHaveTextContent('Scan Media Library');

  finishReload?.(snapshot);
  await waitFor(() => { expect(screen.getByRole('button', { name: 'Reload tasks' })).toBeEnabled(); });
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
  expect(dangerToast).toHaveBeenCalledWith(
    'The task command could not be completed.',
    expect.any(Object),
  );
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

it('isolates pending state per command and reports the durable job count', async () => {
  const successToast = vi.spyOn(Toast.toast, 'success').mockReturnValue('command-toast');
  let finishValidation: ((jobs: string[]) => void) | undefined;
  validateMock.mockReturnValue(new Promise((resolve) => { finishValidation = resolve; }));
  renderTasks();
  const user = userEvent.setup();

  await screen.findByRole('list', { name: 'Scheduled tasks' });
  await user.click(screen.getByRole('button', { name: 'Validate storage' }));
  expect(screen.getByRole('button', { name: 'Validate storage' })).toBeDisabled();
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

  await screen.findByRole('list', { name: 'Scheduled tasks' });
  const expectedTones = new Map<string, string>([
    ['Idle', 'neutral'],
    ['Running', 'accent'],
    ['Pending', 'neutral'],
    ['Retrying', 'warning'],
    ['Completed', 'success'],
    ['Cancelled', 'neutral'],
    ['Failed', 'danger'],
  ]);
  for (const [label, tone] of expectedTones) {
    const matches = screen.getAllByText(label);
    expect(matches.some((match) => match.closest(`[data-tone="${tone}"]`) !== null)).toBe(true);
  }
  expect(screen.getByText('Pending Media Scan')).toBeVisible();
  expect(screen.getByText('PendingMediaScan')).toBeVisible();
});

it('delegates authorization failures without showing local task errors or toasts', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  const checkError = vi.fn().mockRejectedValue({ logoutUser: false, message: false });
  snapshotMock.mockRejectedValue({ status: 403, message: 'private-auth-detail' });
  renderTasks({ ...defaultTestAuthProvider, checkError });

  await waitFor(() => { expect(checkError).toHaveBeenCalled(); });
  await waitFor(() => {
    expect(screen.queryByRole('status', { name: 'Loading tasks' })).not.toBeInTheDocument();
  });
  expect(screen.queryByRole('heading', { name: 'Unable to load this content' })).not.toBeInTheDocument();
  expect(dangerToast).not.toHaveBeenCalled();
  expect(screen.queryByText('private-auth-detail')).not.toBeInTheDocument();
});

it('delegates command authorization failures without showing a command error toast', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  const checkError = vi.fn().mockRejectedValue({ logoutUser: false, message: false });
  startMock.mockRejectedValue({ status: 401, message: 'private-command-auth-detail' });
  renderTasks({ ...defaultTestAuthProvider, checkError });
  const user = userEvent.setup();
  const scheduled = await screen.findByRole('list', { name: 'Scheduled tasks' });

  await user.click(within(scheduled).getByRole('button', { name: 'Start Scan Media Library' }));

  await waitFor(() => { expect(checkError).toHaveBeenCalled(); });
  expect(dangerToast).not.toHaveBeenCalled();
  expect(screen.queryByText('private-command-auth-detail')).not.toBeInTheDocument();
});
