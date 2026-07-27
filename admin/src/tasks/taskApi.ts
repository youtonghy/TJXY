import { ApiError, apiRequest } from '../api/httpClient';
import { listLibraries } from '../libraries/libraryApi';

export type ScheduledTaskState = 'Idle' | 'Running';
export type TaskJobStatus = 'Pending' | 'Retrying' | 'Running' | 'Completed' | 'Cancelled' | 'Failed';

export interface ScheduledTask {
  id: string;
  name: string;
  state: ScheduledTaskState;
  description: string;
  category: string;
  key: string;
}

export interface TaskJob {
  id: string;
  taskKind: string;
  scopeType: string;
  scopeId: string;
  status: TaskJobStatus;
  priority: number;
  attemptCount: number;
  createdAt: string | null;
  startedAt: string | null;
  completedAt: string | null;
}

export interface StorageRootOption {
  key: string;
  libraryId: string;
  storageRootId: string;
  label: string;
}

export interface TaskSnapshot {
  scheduled: ScheduledTask[];
  jobs: TaskJob[];
  roots: StorageRootOption[];
}

export async function getTaskSnapshot(signal?: AbortSignal): Promise<TaskSnapshot> {
  const options = signal === undefined ? {} : { signal };
  const [scheduled, jobs, libraries] = await Promise.all([
    apiRequest<unknown>('/ScheduledTasks', options),
    apiRequest<unknown>('/Admin/Tasks/Jobs?Limit=50', options),
    listLibraries(signal),
  ]);
  if (!Array.isArray(scheduled)) throw invalidResponse('scheduled task list');
  if (!Array.isArray(jobs)) throw invalidResponse('recent task list');
  return {
    scheduled: scheduled.map(toScheduledTask),
    jobs: jobs.map(toTaskJob),
    roots: libraries.flatMap((library) => library.locations.map((location, index) => {
      const rootId = storageRootId(location);
      return {
        key: `${library.id}:${rootId}`,
        libraryId: library.id,
        storageRootId: rootId,
        label: library.locations.length === 1 ? library.name : `${library.name} root ${String(index + 1)}`,
      };
    })),
  };
}

export async function startScheduledTask(id: string): Promise<void> {
  await apiRequest(`/ScheduledTasks/Running/${encodeURIComponent(requireId(id))}`, { method: 'POST' });
}

export async function cancelScheduledTask(id: string): Promise<void> {
  await apiRequest(`/ScheduledTasks/Running/${encodeURIComponent(requireId(id))}`, { method: 'DELETE' });
}

export async function validateStorage(rootId: string): Promise<string[]> {
  return submitSingle(`/Admin/Tasks/ValidateStorage/${encodeURIComponent(requireId(rootId))}`);
}

export async function discoverTitles(rootId: string): Promise<string[]> {
  return submitSingle(`/Admin/Tasks/DiscoverTitles/${encodeURIComponent(requireId(rootId))}`);
}

export async function fullScanRoot(libraryId: string, rootId: string): Promise<string[]> {
  return submitSingle(
    `/Admin/Tasks/FullScan/${encodeURIComponent(requireId(libraryId))}/${encodeURIComponent(requireId(rootId))}`,
  );
}

export async function resolveMetadata(itemId: string): Promise<string[]> {
  return submitSingle(`/Admin/Tasks/ResolveMetadata/${encodeURIComponent(requireId(itemId))}`);
}

export async function expandItem(itemId: string): Promise<string[]> {
  return submitSingle(`/Admin/Tasks/ExpandItem/${encodeURIComponent(requireId(itemId))}`);
}

export async function indexMediaSources(itemId: string): Promise<string[]> {
  return submitSingle(`/Admin/Tasks/IndexMediaSources/${encodeURIComponent(requireId(itemId))}`);
}

export async function probeMedia(itemId: string): Promise<string[]> {
  const value = await apiRequest<unknown>(
    `/Admin/Tasks/ProbeMedia/${encodeURIComponent(requireId(itemId))}`,
    { method: 'POST' },
  );
  if (!isRecord(value) || !Array.isArray(value.Jobs)) throw invalidResponse('Probe submission');
  return value.Jobs.map((job) => {
    if (!isRecord(job) || !validId(job.JobId)) throw invalidResponse('Probe job');
    return job.JobId;
  });
}

async function submitSingle(path: string): Promise<string[]> {
  const value = await apiRequest<unknown>(path, { method: 'POST' });
  if (!isRecord(value) || !validId(value.JobId)) throw invalidResponse('task submission');
  return [value.JobId];
}

function toScheduledTask(value: unknown): ScheduledTask {
  if (
    !isRecord(value)
    || !validId(value.Id)
    || !validText(value.Name)
    || (value.State !== 'Idle' && value.State !== 'Running')
    || !validText(value.Description)
    || !validText(value.Category)
    || !validText(value.Key)
  ) throw invalidResponse('scheduled task');
  return {
    id: value.Id,
    name: value.Name,
    state: value.State,
    description: value.Description,
    category: value.Category,
    key: value.Key,
  };
}

function toTaskJob(value: unknown): TaskJob {
  if (
    !isRecord(value)
    || !validId(value.Id)
    || !validText(value.TaskKind)
    || !validText(value.ScopeType)
    || !validId(value.ScopeId)
    || !isTaskJobStatus(value.Status)
    || !Number.isSafeInteger(value.Priority)
    || !Number.isSafeInteger(value.AttemptCount)
    || !validDate(value.CreatedAt)
    || !validDate(value.StartedAt)
    || !validDate(value.CompletedAt)
  ) throw invalidResponse('recent task');
  return {
    id: value.Id,
    taskKind: value.TaskKind,
    scopeType: value.ScopeType,
    scopeId: value.ScopeId,
    status: value.Status,
    priority: value.Priority as number,
    attemptCount: value.AttemptCount as number,
    createdAt: value.CreatedAt,
    startedAt: value.StartedAt,
    completedAt: value.CompletedAt,
  };
}

function storageRootId(location: string): string {
  const id = location.slice('tjxy://storage-root/'.length);
  if (!validId(id)) throw invalidResponse('library root');
  return id;
}

function requireId(value: string): string {
  const id = value.trim();
  if (!validId(id)) throw new ApiError(400, 'validation', 'A valid identifier is required.');
  return id;
}

function validId(value: unknown): value is string {
  return typeof value === 'string'
    && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(value);
}

function validText(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0 && value.length <= 512;
}

function validDate(value: unknown): value is string | null {
  return value === null || (typeof value === 'string' && !Number.isNaN(Date.parse(value)));
}

function isTaskJobStatus(value: unknown): value is TaskJobStatus {
  return value === 'Pending'
    || value === 'Retrying'
    || value === 'Running'
    || value === 'Completed'
    || value === 'Cancelled'
    || value === 'Failed';
}

function invalidResponse(subject: string): ApiError {
  return new ApiError(200, 'invalid-response', `The server returned an invalid ${subject}.`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
