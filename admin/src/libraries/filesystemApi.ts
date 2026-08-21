import { ApiError, apiRequest } from '../api/httpClient';

export async function attachFilesystemFolder(
  libraryId: string,
  path: string,
): Promise<void> {
  await apiRequest('/Library/VirtualFolders/Paths', {
    method: 'POST',
    body: JSON.stringify({
      LibraryId: requireText(libraryId),
      Path: requireText(path),
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
