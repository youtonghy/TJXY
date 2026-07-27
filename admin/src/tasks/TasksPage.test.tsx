import { ThemeProvider } from '@mui/material/styles';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { theme } from '../theme';
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
import { TasksPage } from './TasksPage';

const notify = vi.fn();
vi.mock('react-admin', () => ({
  Title: ({ title }: { title: string }) => <title>{title}</title>,
  useNotify: () => notify,
}));
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
const rootId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';
const jobId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f13';

beforeEach(() => {
  notify.mockReset();
  for (const mock of [snapshotMock, startMock, cancelMock, validateMock, discoverMock, expandMock, fullScanMock, indexMock, resolveMock, probeMock]) {
    mock.mockReset();
  }
  snapshotMock.mockResolvedValue({
    scheduled: [{
      id: taskId,
      name: 'Scan Media Library',
      state: 'Idle',
      description: 'Scans libraries',
      category: 'Library',
      key: 'FullMediaScan',
    }],
    jobs: [{
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
    }],
    roots: [{
      key: `${taskId}:${rootId}`,
      libraryId: taskId,
      storageRootId: rootId,
      label: 'Movies',
    }],
  });
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

it('starts scheduled work and reloads authoritative job state', async () => {
  render(<ThemeProvider theme={theme}><TasksPage /></ThemeProvider>);
  const user = userEvent.setup();

  expect(await screen.findByRole('rowheader', { name: /Scan Media Library/ })).toBeVisible();
  expect(screen.getByRole('table', { name: 'Recent durable jobs' })).toHaveTextContent('Completed');
  await user.click(screen.getByRole('button', { name: 'Start' }));

  expect(startMock).toHaveBeenCalledWith(taskId);
  await waitFor(() => { expect(snapshotMock).toHaveBeenCalledTimes(2); });
  expect(notify).toHaveBeenCalledWith('Scheduled task started.', { type: 'success' });
});

it('submits root and item commands with validated identifiers', async () => {
  render(<ThemeProvider theme={theme}><TasksPage /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByRole('table', { name: 'Scheduled tasks' });
  await user.click(screen.getByRole('button', { name: 'Validate storage' }));
  expect(validateMock).toHaveBeenCalledWith(rootId);
  await user.click(screen.getByRole('button', { name: 'Full scan' }));
  expect(fullScanMock).toHaveBeenCalledWith(taskId, rootId);

  await user.type(screen.getByRole('textbox', { name: 'Catalog item ID' }), taskId);
  await user.click(screen.getByRole('button', { name: 'Resolve metadata' }));
  expect(resolveMock).toHaveBeenCalledWith(taskId);
  expect(notify).toHaveBeenCalledWith(
    'Metadata resolution submitted. 1 durable job accepted.',
    { type: 'success' },
  );
  await user.click(screen.getByRole('button', { name: 'Expand item' }));
  expect(expandMock).toHaveBeenCalledWith(taskId);
  await user.click(screen.getByRole('button', { name: 'Index sources' }));
  expect(indexMock).toHaveBeenCalledWith(taskId);
});
