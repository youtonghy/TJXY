import { apiRequest, mediaBrowserTokenHeader } from '../api/httpClient';

export type LogMode = 'Error' | 'Debug';

export interface LoggingSettings {
  mode: LogMode;
  retentionDays: number;
  revision: number;
  directory: string;
}

export interface LogFile {
  date: string;
  sizeBytes: number;
  current: boolean;
}

export interface LogPage {
  date: string;
  lines: string[];
  offset: number;
  nextOffset: number;
  sizeBytes: number;
  hasOlder: boolean;
}

export async function getLoggingSettings(signal?: AbortSignal): Promise<LoggingSettings> {
  return parseSettings(await apiRequest<Record<string, unknown>>('/Admin/Logs/Settings', { signal }));
}

export async function saveLoggingSettings(settings: LoggingSettings): Promise<LoggingSettings> {
  return parseSettings(await apiRequest<Record<string, unknown>>('/Admin/Logs/Settings', {
    method: 'PUT',
    body: JSON.stringify({
      Mode: settings.mode,
      RetentionDays: settings.retentionDays,
      ...(settings.revision > 0 ? { Revision: settings.revision } : {}),
    }),
  }));
}

export async function listLogFiles(signal?: AbortSignal): Promise<LogFile[]> {
  const value = await apiRequest<unknown>('/Admin/Logs', { signal });
  if (!Array.isArray(value)) throw new Error('Invalid log file response');
  return value.map((entry) => {
    if (!record(entry) || typeof entry.Date !== 'string' || typeof entry.SizeBytes !== 'number' || typeof entry.Current !== 'boolean') {
      throw new Error('Invalid log file response');
    }
    return { date: entry.Date, sizeBytes: entry.SizeBytes, current: entry.Current };
  });
}

export async function getLogPage(date: string, before?: number, signal?: AbortSignal): Promise<LogPage> {
  const suffix = before === undefined ? '' : `?Before=${encodeURIComponent(String(before))}`;
  const value = await apiRequest<Record<string, unknown>>(`/Admin/Logs/${encodeURIComponent(date)}${suffix}`, { signal });
  if (
    typeof value.Date !== 'string' || !Array.isArray(value.Lines) || !value.Lines.every((line) => typeof line === 'string')
    || typeof value.Offset !== 'number' || typeof value.NextOffset !== 'number'
    || typeof value.SizeBytes !== 'number' || typeof value.HasOlder !== 'boolean'
  ) throw new Error('Invalid log page response');
  return { date: value.Date, lines: value.Lines, offset: value.Offset, nextOffset: value.NextOffset, sizeBytes: value.SizeBytes, hasOlder: value.HasOlder };
}

export async function downloadLogFile(date: string): Promise<void> {
  const response = await fetch(`/Admin/Logs/${encodeURIComponent(date)}/Download`, {
    headers: { Authorization: mediaBrowserTokenHeader() },
  });
  if (!response.ok) throw new Error('Log download failed');
  const url = URL.createObjectURL(await response.blob());
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `tjxy.${date}.log`;
  anchor.click();
  URL.revokeObjectURL(url);
}

function parseSettings(value: Record<string, unknown>): LoggingSettings {
  if ((value.Mode !== 'Error' && value.Mode !== 'Debug') || typeof value.RetentionDays !== 'number' || typeof value.Revision !== 'number' || typeof value.Directory !== 'string') {
    throw new Error('Invalid logging settings response');
  }
  return { mode: value.Mode, retentionDays: value.RetentionDays, revision: value.Revision, directory: value.Directory };
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
