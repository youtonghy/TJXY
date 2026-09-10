import { ApiError, apiRequest } from '../api/httpClient';

export interface LibraryFolder { id: string; name: string; path: string | null; provider: string }
export interface FolderEntry { name: string; path: string; isDirectory: boolean; size: number | null; modifiedAt: string | null }
export interface FolderContents { items: FolderEntry[]; indexed: boolean }

export async function listLibraryFolders(libraryId: string, signal?: AbortSignal): Promise<LibraryFolder[]> {
  const value = await apiRequest<unknown>(base(libraryId), { signal });
  if (!Array.isArray(value)) throw invalidResponse();
  return value.map((item) => {
    if (!record(item) || !text(item.Id) || !text(item.Name) || !text(item.Provider)
      || !(item.Path === null || text(item.Path))) throw invalidResponse();
    return { id: item.Id, name: item.Name, provider: item.Provider, path: item.Path };
  });
}

export async function listFolderContents(libraryId: string, rootId: string, path: string, signal?: AbortSignal): Promise<FolderContents> {
  const query = new URLSearchParams({ Path: path });
  const value = await apiRequest<unknown>(`${base(libraryId)}/${encodeURIComponent(rootId)}/Contents?${query.toString()}`, { signal });
  if (!record(value) || !Array.isArray(value.Items) || typeof value.Indexed !== 'boolean') throw invalidResponse();
  return { indexed: value.Indexed, items: value.Items.map((item) => {
    if (!record(item) || !text(item.Name) || !text(item.Path) || typeof item.IsDirectory !== 'boolean'
      || !(item.Size === null || (typeof item.Size === 'number' && Number.isSafeInteger(item.Size) && item.Size >= 0))
      || !(item.ModifiedAt === null || (text(item.ModifiedAt) && Number.isFinite(Date.parse(item.ModifiedAt))))) throw invalidResponse();
    return { name: item.Name, path: item.Path, isDirectory: item.IsDirectory, size: item.Size, modifiedAt: item.ModifiedAt };
  }) };
}

function base(libraryId: string) { return `/Admin/Libraries/${encodeURIComponent(libraryId)}/Folders`; }
function record(value: unknown): value is Record<string, unknown> { return typeof value === 'object' && value !== null && !Array.isArray(value); }
function text(value: unknown): value is string { return typeof value === 'string' && value.length > 0; }
function invalidResponse() { return new ApiError(200, 'invalid-response', 'Invalid folder response.'); }
