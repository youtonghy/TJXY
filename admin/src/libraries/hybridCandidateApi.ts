import { ApiError, apiRequest } from '../api/httpClient';

export interface HybridCandidate {
  id: string;
  name: string;
  productionYear: number | null;
  structureState: string;
  selectedAt: string;
}

export interface HybridCandidatePage {
  items: HybridCandidate[];
  totalRecordCount: number;
  startIndex: number;
}

export async function listHybridCandidates(
  libraryId: string,
  startIndex: number,
  limit: number,
  signal?: AbortSignal,
): Promise<HybridCandidatePage> {
  const id = requireId(libraryId);
  if (!isNonNegativeInteger(startIndex) || !Number.isSafeInteger(limit) || limit < 1 || limit > 100) {
    throw new ApiError(400, 'validation', 'A valid candidate page is required.');
  }
  const query = new URLSearchParams({
    StartIndex: String(startIndex),
    Limit: String(limit),
  });
  const value = await apiRequest<unknown>(
    `/Admin/Libraries/${id}/HybridCandidates?${query.toString()}`,
    signal === undefined ? {} : { signal },
  );
  return toCandidatePage(value);
}

export async function pinHybridCandidate(libraryId: string, itemId: string): Promise<void> {
  await apiRequest(candidatePath(libraryId, itemId), { method: 'PUT' });
}

export async function unpinHybridCandidate(libraryId: string, itemId: string): Promise<void> {
  await apiRequest(candidatePath(libraryId, itemId), { method: 'DELETE' });
}

function candidatePath(libraryId: string, itemId: string): string {
  return `/Admin/Libraries/${requireId(libraryId)}/HybridCandidates/${requireId(itemId)}`;
}

function toCandidatePage(value: unknown): HybridCandidatePage {
  if (
    !isRecord(value)
    || !Array.isArray(value.Items)
    || !isNonNegativeInteger(value.TotalRecordCount)
    || !isNonNegativeInteger(value.StartIndex)
  ) throw invalidResponse('hybrid candidate page');

  const items = value.Items.map(toCandidate);
  if (items.length > value.TotalRecordCount) throw invalidResponse('hybrid candidate page');
  return {
    items,
    totalRecordCount: value.TotalRecordCount,
    startIndex: value.StartIndex,
  };
}

function toCandidate(value: unknown): HybridCandidate {
  if (
    !isRecord(value)
    || !validId(value.Id)
    || !validText(value.Name)
    || !validProductionYear(value.ProductionYear)
    || !validText(value.StructureState)
    || !validDate(value.SelectedAt)
  ) throw invalidResponse('hybrid candidate');
  return {
    id: value.Id,
    name: value.Name,
    productionYear: value.ProductionYear,
    structureState: value.StructureState,
    selectedAt: value.SelectedAt,
  };
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

function validProductionYear(value: unknown): value is number | null {
  return value === null
    || (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 && value <= 9999);
}

function validDate(value: unknown): value is string {
  return typeof value === 'string' && !Number.isNaN(Date.parse(value));
}

function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}

function invalidResponse(subject: string): ApiError {
  return new ApiError(200, 'invalid-response', `The server returned an invalid ${subject}.`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
