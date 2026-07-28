import type { GoogleDriveChoice } from './googleDriveApi';

export function uniqueChoices(items: GoogleDriveChoice[]): GoogleDriveChoice[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    if (seen.has(item.id)) return false;
    seen.add(item.id);
    return true;
  });
}
