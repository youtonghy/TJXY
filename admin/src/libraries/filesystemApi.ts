import { ApiError, apiRequest } from '../api/httpClient';

export interface FilesystemRoot {
  id: string;
  name: string;
}

export interface FilesystemDirectory {
  name: string;
  relativePath: string;
  modifiedAt: string | null;
}

export interface FilesystemSelection {
  rootId: string;
  relativePath: string;
}

export async function listFilesystemRoots(signal?: AbortSignal): Promise<FilesystemRoot[]> {
  const value = await apiRequest<unknown>(
    '/Admin/Filesystem/Roots',
    signal === undefined ? {} : { signal },
  );
  if (!Array.isArray(value)) throw invalidResponse();
  return value.map((root) => {
    if (!isRecord(root) || !validText(root.Id) || !validText(root.Name)) throw invalidResponse();
    return { id: root.Id, name: root.Name };
  });
}

export async function listFilesystemDirectories(
  rootId: string,
  relativePath: string,
  signal?: AbortSignal,
): Promise<FilesystemDirectory[]> {
  const query = new URLSearchParams({ RootId: requireText(rootId), Path: relativePath });
  const value = await apiRequest<unknown>(
    `/Admin/Filesystem/Directories?${query.toString()}`,
    signal === undefined ? {} : { signal },
  );
  if (!isRecord(value) || !Array.isArray(value.Items)) throw invalidResponse();
  return value.Items.map((entry) => {
    if (
      !isRecord(entry)
      || !validText(entry.Name)
      || typeof entry.RelativePath !== 'string'
      || (entry.ModifiedAt !== null && entry.ModifiedAt !== undefined && !validText(entry.ModifiedAt))
    ) throw invalidResponse();
    return {
      name: entry.Name,
      relativePath: entry.RelativePath,
      modifiedAt: typeof entry.ModifiedAt === 'string' ? entry.ModifiedAt : null,
    };
  });
}

export async function attachFilesystemFolder(
  libraryId: string,
  selection: FilesystemSelection,
): Promise<void> {
  await apiRequest('/Library/VirtualFolders/Paths', {
    method: 'POST',
    body: JSON.stringify({
      LibraryId: requireText(libraryId),
      FilesystemSelection: {
        RootId: requireText(selection.rootId),
        RelativePath: selection.relativePath,
      },
    }),
  });
}

function requireText(value: string): string {
  if (!validText(value)) throw new ApiError(400, 'validation', 'A filesystem identifier is required.');
  return value;
}

function validText(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0 && value.length <= 16_384;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function invalidResponse(): ApiError {
  return new ApiError(200, 'invalid-response', 'The server returned an invalid filesystem response.');
}
