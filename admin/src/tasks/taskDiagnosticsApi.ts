import { ApiError, apiRequest } from '../api/httpClient';

export interface NfoChoice {
  ItemId: string;
  RootId: string;
  Fingerprint: string;
  Candidates: { Id: string; Name: string; Digest: string }[];
  ConflictFields: string[];
  SelectedObjectId: string | null;
  Status: 'NeedsSelection' | 'Selected';
}

export interface ScanIssue {
  ItemId: string;
  ChildJobId: string;
  TaskKind: string;
  ScopeType: string;
  ScopeId: string;
  NeedsSelection: boolean;
}

export interface ScanReport {
  Counters: { items: number; success: number; failed: number; skipped: number; needs_selection: number } | null;
  Issues: ScanIssue[];
  HasMore: boolean;
}

export interface ScanHistoryEntry { Id: string; CreatedAt: string; State: string }
export async function listScanHistory(offset = 0, signal?: AbortSignal): Promise<ScanHistoryEntry[]> {
  const result = await apiRequest<unknown>(`/Admin/Tasks/Scans?Offset=${String(offset)}`, { signal });
  if (!Array.isArray(result) || result.length > 50 || !result.every((entry: unknown) => record(entry) && uuid(entry.Id)
    && typeof entry.CreatedAt === 'string' && Number.isFinite(Date.parse(entry.CreatedAt)) && typeof entry.State === 'string')) throw invalid();
  return result as ScanHistoryEntry[];
}

export async function listNfoChoices(offset = 0, signal?: AbortSignal): Promise<NfoChoice[]> {
  const result = await apiRequest<unknown>(`/Admin/Tasks/NfoChoices?Offset=${String(offset)}`, { signal });
  if (!Array.isArray(result) || result.length > 50 || !result.every(validChoice)) throw invalid();
  return result;
}

export async function chooseNfo(choice: NfoChoice, candidate: string): Promise<void> {
  if (!choice.Candidates.some((entry) => entry.Id === candidate)) throw invalid();
  await apiRequest('/Admin/Tasks/NfoChoices', { method: 'POST', body: JSON.stringify({
    ItemId: choice.ItemId, RootId: choice.RootId, CandidateId: candidate, Fingerprint: choice.Fingerprint,
  }) });
}

export async function getScanReport(id: string, offset = 0, signal?: AbortSignal): Promise<ScanReport> {
  if (!uuid(id)) throw invalid();
  const result = await apiRequest<unknown>(`/Admin/Tasks/Scans/${encodeURIComponent(id)}?Offset=${String(offset)}`, { signal });
  if (!record(result) || !Array.isArray(result.Issues) || result.Issues.length > 50
    || !result.Issues.every(validIssue) || typeof result.HasMore !== 'boolean') throw invalid();
  const counters = result.Counters;
  if (counters !== null && (!record(counters)
    || !['items', 'success', 'failed', 'skipped', 'needs_selection'].every((key) => Number.isSafeInteger(counters[key]) && (counters[key] as number) >= 0))) throw invalid();
  return result as unknown as ScanReport;
}

export async function retryScanIssues(id: string, offset: number): Promise<number> {
  if (!uuid(id)) throw invalid();
  const result = await apiRequest<unknown>(`/Admin/Tasks/Scans/${encodeURIComponent(id)}/Retry?Offset=${String(offset)}`, { method: 'POST' });
  if (!record(result) || !Array.isArray(result.JobIds) || !result.JobIds.every(uuid)) throw invalid();
  return result.JobIds.length;
}

function validChoice(value: unknown): value is NfoChoice {
  return record(value) && uuid(value.ItemId) && uuid(value.RootId)
    && typeof value.Fingerprint === 'string' && /^[0-9a-f]{64}$/u.test(value.Fingerprint)
    && Array.isArray(value.Candidates) && value.Candidates.length <= 512 && value.Candidates.every((candidate) => record(candidate)
      && uuid(candidate.Id) && typeof candidate.Name === 'string' && candidate.Name.length <= 512
      && typeof candidate.Digest === 'string' && (candidate.Digest === '' || /^[0-9a-f]{64}$/u.test(candidate.Digest)))
    && Array.isArray(value.ConflictFields) && value.ConflictFields.every((field) => typeof field === 'string')
    && (value.SelectedObjectId === null || uuid(value.SelectedObjectId))
    && (value.Status === 'NeedsSelection' || value.Status === 'Selected');
}

function validIssue(value: unknown): value is ScanIssue {
  return record(value) && uuid(value.ItemId) && uuid(value.ChildJobId) && uuid(value.ScopeId)
    && typeof value.TaskKind === 'string' && typeof value.ScopeType === 'string' && typeof value.NeedsSelection === 'boolean';
}

function uuid(value: unknown): value is string {
  return typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(value);
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function invalid(): ApiError {
  return new ApiError(200, 'invalid-response', 'The server returned invalid task diagnostics.');
}
