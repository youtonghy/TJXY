import { apiRequest } from '../api/httpClient';
import { listLibraries } from '../libraries/libraryApi';
import {
  cancelScheduledTask,
  expandItem,
  fullScanRoot,
  getTaskSnapshot,
  indexMediaSources,
  probeMedia,
  startScheduledTask,
  validateStorage,
} from './taskApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});
vi.mock('../libraries/libraryApi', () => ({ listLibraries: vi.fn() }));

const requestMock = vi.mocked(apiRequest);
const listLibrariesMock = vi.mocked(listLibraries);
const taskId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const rootId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';
const jobId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f13';

beforeEach(() => {
  requestMock.mockReset();
  listLibrariesMock.mockReset();
  listLibrariesMock.mockResolvedValue([{
    id: taskId,
    name: 'Movies',
    collectionType: 'movies',
    locations: [`tjxy://storage-root/${rootId}`],
    enabled: true,
    scanProfile: 'Lazy',
    profileVersion: 1,
    objectSelectionScope: 'title_layer',
    metadataPolicy: 'basic',
    metadataSourceMode: 'automatic_scrape',
    expansionPolicy: 'on_browse',
    probePolicy: 'on_playback',
  }]);
});

it('loads and validates scheduled tasks, recent jobs, and reusable library roots', async () => {
  requestMock
    .mockResolvedValueOnce([{
      Id: taskId,
      Name: 'Scan Media Library',
      State: 'Idle',
      Description: 'Scans libraries',
      Category: 'Library',
      Key: 'FullMediaScan',
    }])
    .mockResolvedValueOnce([{
      Id: jobId,
      TaskKind: 'FullMediaScan',
      ScopeType: 'Library',
      ScopeId: taskId,
      Status: 'Completed',
      Priority: 20,
      AttemptCount: 1,
      CreatedAt: '2026-07-24T01:02:03Z',
      StartedAt: '2026-07-24T01:02:04Z',
      CompletedAt: '2026-07-24T01:02:05Z',
      Outcome: 'NoMetadataMatch',
    }]);

  await expect(getTaskSnapshot()).resolves.toEqual({
    scheduled: [expect.objectContaining({ id: taskId, key: 'FullMediaScan', state: 'Idle' })],
    jobs: [expect.objectContaining({
      id: jobId,
      status: 'Completed',
      attemptCount: 1,
      outcome: 'NoMetadataMatch',
    })],
    roots: [{
      key: `${taskId}:${rootId}`,
      libraryId: taskId,
      storageRootId: rootId,
      label: 'Movies',
    }],
  });
  expect(requestMock).toHaveBeenNthCalledWith(1, '/ScheduledTasks', {});
  expect(requestMock).toHaveBeenNthCalledWith(2, '/Admin/Tasks/Jobs?Limit=50', {});
});

it('uses exact pessimistic task commands and validates returned job identifiers', async () => {
  requestMock.mockResolvedValue(undefined);
  await startScheduledTask(taskId);
  await cancelScheduledTask(taskId);
  requestMock.mockResolvedValueOnce({ JobId: jobId });
  await expect(validateStorage(rootId)).resolves.toEqual([jobId]);
  requestMock.mockResolvedValueOnce({ JobId: jobId });
  await expect(fullScanRoot(taskId, rootId)).resolves.toEqual([jobId]);
  requestMock.mockResolvedValueOnce({ JobId: jobId });
  await expect(expandItem(taskId)).resolves.toEqual([jobId]);
  requestMock.mockResolvedValueOnce({ JobId: jobId });
  await expect(indexMediaSources(taskId)).resolves.toEqual([jobId]);
  requestMock.mockResolvedValueOnce({ Jobs: [{ JobId: jobId }] });
  await expect(probeMedia(taskId)).resolves.toEqual([jobId]);

  expect(requestMock).toHaveBeenNthCalledWith(
    1,
    `/ScheduledTasks/Running/${taskId}`,
    { method: 'POST' },
  );
  expect(requestMock).toHaveBeenNthCalledWith(
    2,
    `/ScheduledTasks/Running/${taskId}`,
    { method: 'DELETE' },
  );
  expect(requestMock).toHaveBeenNthCalledWith(
    3,
    `/Admin/Tasks/ValidateStorage/${rootId}`,
    { method: 'POST' },
  );
  expect(requestMock).toHaveBeenNthCalledWith(
    4,
    `/Admin/Tasks/FullScan/${taskId}/${rootId}`,
    { method: 'POST' },
  );
  expect(requestMock).toHaveBeenNthCalledWith(
    5,
    `/Admin/Tasks/ExpandItem/${taskId}`,
    { method: 'POST' },
  );
  expect(requestMock).toHaveBeenNthCalledWith(
    6,
    `/Admin/Tasks/IndexMediaSources/${taskId}`,
    { method: 'POST' },
  );
});

it('rejects malformed job responses instead of rendering unsafe server data', async () => {
  requestMock
    .mockResolvedValueOnce([])
    .mockResolvedValueOnce([{
      Id: jobId,
      TaskKind: 'ProbeMedia',
      ScopeType: 'MediaSource',
      ScopeId: rootId,
      Status: 'Unknown',
      Priority: 100,
      AttemptCount: 1,
      CreatedAt: null,
      StartedAt: null,
      CompletedAt: null,
      LastError: 'secret',
    }]);

  await expect(getTaskSnapshot()).rejects.toMatchObject({ category: 'invalid-response' });
});
