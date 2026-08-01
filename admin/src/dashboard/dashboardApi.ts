import { apiRequest } from '../api/httpClient';
import { invalidResponse, isRecord, validText } from '../api/responseValidation';

export type DashboardRange = 'today' | '7d' | '30d';

export interface DashboardTrendPoint {
  bucketStart: string;
  playCount: number;
  uniqueViewers: number;
}

export interface DashboardTopItem {
  itemId: string;
  name: string;
  itemType: string;
  productionYear: number | null;
  playCount: number;
  uniqueViewers: number;
}

export interface DashboardSummary {
  from: string;
  to: string;
  usersTotal: number;
  usersDisabled: number;
  catalogTotal: number;
  movies: number;
  series: number;
  episodes: number;
  playCount: number;
  uniqueViewers: number;
  currentlyWatching: number;
  trend: DashboardTrendPoint[];
  topItems: DashboardTopItem[];
}

export interface NowPlayingItem {
  sessionId: string;
  userId: string;
  userName: string;
  itemId: string;
  itemName: string;
  itemType: string;
  runtimeTicks: number | null;
  positionTicks: number;
  clientName: string;
  deviceName: string;
  startedAt: string;
  lastEventAt: string;
}

export interface LoginHistoryItem {
  sessionId: string;
  userId: string;
  userName: string;
  clientName: string;
  clientVersion: string;
  deviceName: string;
  createdAt: string;
  lastSeenAt: string | null;
  expiresAt: string | null;
  revokedAt: string | null;
  status: 'Active' | 'Expired' | 'Revoked';
}

export interface WatchHistoryItem {
  sessionId: string;
  userId: string;
  userName: string;
  itemId: string;
  itemName: string;
  itemType: string;
  runtimeTicks: number | null;
  positionTicks: number;
  startedAt: string;
  lastEventAt: string;
  stoppedAt: string | null;
}

export interface DashboardPage<T> {
  items: T[];
  totalRecordCount: number;
  startIndex: number;
}

export interface DashboardSnapshot {
  summary: DashboardSummary;
  nowPlaying: NowPlayingItem[];
}

export function dashboardWindow(range: DashboardRange, now = new Date()): { from: string; to: string } {
  const from = new Date(now);
  from.setHours(0, 0, 0, 0);
  if (range === '7d') from.setDate(from.getDate() - 6);
  if (range === '30d') from.setDate(from.getDate() - 29);
  return { from: from.toISOString(), to: now.toISOString() };
}

export async function getDashboardSnapshot(
  range: DashboardRange,
  signal?: AbortSignal,
): Promise<DashboardSnapshot> {
  const window = dashboardWindow(range);
  const query = new URLSearchParams({
    from: window.from,
    to: window.to,
    activeWithinSeconds: '60',
    topLimit: '10',
  });
  const options = signal === undefined ? {} : { signal };
  const [summary, nowPlaying] = await Promise.all([
    apiRequest<unknown>(`/Admin/Dashboard/Summary?${query.toString()}`, options),
    apiRequest<unknown>('/Admin/Dashboard/NowPlaying?activeWithinSeconds=60', options),
  ]);
  return {
    summary: toSummary(summary),
    nowPlaying: toArray(nowPlaying, toNowPlaying, 'currently playing list'),
  };
}

export async function getLoginHistory(
  startIndex: number,
  limit: number,
  signal?: AbortSignal,
): Promise<DashboardPage<LoginHistoryItem>> {
  return getPage('/Admin/Dashboard/LoginHistory', startIndex, limit, toLogin, signal);
}

export async function getWatchHistory(
  startIndex: number,
  limit: number,
  signal?: AbortSignal,
): Promise<DashboardPage<WatchHistoryItem>> {
  return getPage('/Admin/Dashboard/WatchHistory', startIndex, limit, toWatch, signal);
}

async function getPage<T>(
  path: string,
  startIndex: number,
  limit: number,
  convert: (value: unknown) => T,
  signal?: AbortSignal,
): Promise<DashboardPage<T>> {
  const query = new URLSearchParams({ startIndex: String(startIndex), limit: String(limit) });
  const value = await apiRequest<unknown>(
    `${path}?${query.toString()}`,
    signal === undefined ? {} : { signal },
  );
  if (
    !isRecord(value)
    || !Array.isArray(value.Items)
    || !validCount(value.TotalRecordCount)
    || !validCount(value.StartIndex)
  ) throw invalidResponse('dashboard history page');
  return {
    items: value.Items.map(convert),
    totalRecordCount: value.TotalRecordCount,
    startIndex: value.StartIndex,
  };
}

function toSummary(value: unknown): DashboardSummary {
  if (
    !isRecord(value)
    || !validDate(value.From)
    || !validDate(value.To)
    || !validCount(value.UsersTotal)
    || !validCount(value.UsersDisabled)
    || !validCount(value.CatalogTotal)
    || !validCount(value.Movies)
    || !validCount(value.Series)
    || !validCount(value.Episodes)
    || !validCount(value.PlayCount)
    || !validCount(value.UniqueViewers)
    || !validCount(value.CurrentlyWatching)
    || !Array.isArray(value.Trend)
    || !Array.isArray(value.TopItems)
  ) throw invalidResponse('dashboard summary');
  return {
    from: value.From,
    to: value.To,
    usersTotal: value.UsersTotal,
    usersDisabled: value.UsersDisabled,
    catalogTotal: value.CatalogTotal,
    movies: value.Movies,
    series: value.Series,
    episodes: value.Episodes,
    playCount: value.PlayCount,
    uniqueViewers: value.UniqueViewers,
    currentlyWatching: value.CurrentlyWatching,
    trend: value.Trend.map(toTrend),
    topItems: value.TopItems.map(toTopItem),
  };
}

function toTrend(value: unknown): DashboardTrendPoint {
  if (!isRecord(value) || !validDate(value.BucketStart) || !validCount(value.PlayCount) || !validCount(value.UniqueViewers)) {
    throw invalidResponse('dashboard trend point');
  }
  return { bucketStart: value.BucketStart, playCount: value.PlayCount, uniqueViewers: value.UniqueViewers };
}

function toTopItem(value: unknown): DashboardTopItem {
  if (
    !isRecord(value)
    || !validId(value.ItemId)
    || !validText(value.Name, 512)
    || !validText(value.ItemType, 64)
    || !validNullableYear(value.ProductionYear)
    || !validCount(value.PlayCount)
    || !validCount(value.UniqueViewers)
  ) throw invalidResponse('dashboard top item');
  return {
    itemId: value.ItemId,
    name: value.Name,
    itemType: value.ItemType,
    productionYear: value.ProductionYear,
    playCount: value.PlayCount,
    uniqueViewers: value.UniqueViewers,
  };
}

function toNowPlaying(value: unknown): NowPlayingItem {
  if (
    !isRecord(value)
    || !validId(value.SessionId)
    || !validId(value.UserId)
    || !validText(value.UserName, 256)
    || !validId(value.ItemId)
    || !validText(value.ItemName, 512)
    || !validText(value.ItemType, 64)
    || !validNullableTicks(value.RuntimeTicks)
    || !validTicks(value.PositionTicks)
    || !validText(value.ClientName, 256)
    || !validText(value.DeviceName, 256)
    || !validDate(value.StartedAt)
    || !validDate(value.LastEventAt)
  ) throw invalidResponse('currently playing item');
  return {
    sessionId: value.SessionId,
    userId: value.UserId,
    userName: value.UserName,
    itemId: value.ItemId,
    itemName: value.ItemName,
    itemType: value.ItemType,
    runtimeTicks: value.RuntimeTicks,
    positionTicks: value.PositionTicks,
    clientName: value.ClientName,
    deviceName: value.DeviceName,
    startedAt: value.StartedAt,
    lastEventAt: value.LastEventAt,
  };
}

function toLogin(value: unknown): LoginHistoryItem {
  if (
    !isRecord(value)
    || !validId(value.SessionId)
    || !validId(value.UserId)
    || !validText(value.UserName, 256)
    || !validText(value.ClientName, 256)
    || !validText(value.ClientVersion, 128)
    || !validText(value.DeviceName, 256)
    || !validDate(value.CreatedAt)
    || !validNullableDate(value.LastSeenAt)
    || !validNullableDate(value.ExpiresAt)
    || !validNullableDate(value.RevokedAt)
  ) throw invalidResponse('login history item');
  const status = value.RevokedAt !== null
    ? 'Revoked'
    : value.ExpiresAt !== null && new Date(value.ExpiresAt).getTime() <= Date.now()
      ? 'Expired'
      : 'Active';
  return {
    sessionId: value.SessionId,
    userId: value.UserId,
    userName: value.UserName,
    clientName: value.ClientName,
    clientVersion: value.ClientVersion,
    deviceName: value.DeviceName,
    createdAt: value.CreatedAt,
    lastSeenAt: value.LastSeenAt,
    expiresAt: value.ExpiresAt,
    revokedAt: value.RevokedAt,
    status,
  };
}

function toWatch(value: unknown): WatchHistoryItem {
  if (
    !isRecord(value)
    || !validId(value.SessionId)
    || !validId(value.UserId)
    || !validText(value.UserName, 256)
    || !validId(value.ItemId)
    || !validText(value.ItemName, 512)
    || !validText(value.ItemType, 64)
    || !validNullableTicks(value.RuntimeTicks)
    || !validTicks(value.PositionTicks)
    || !validDate(value.StartedAt)
    || !validDate(value.LastEventAt)
    || !validNullableDate(value.StoppedAt)
  ) throw invalidResponse('watch history item');
  return {
    sessionId: value.SessionId,
    userId: value.UserId,
    userName: value.UserName,
    itemId: value.ItemId,
    itemName: value.ItemName,
    itemType: value.ItemType,
    runtimeTicks: value.RuntimeTicks,
    positionTicks: value.PositionTicks,
    startedAt: value.StartedAt,
    lastEventAt: value.LastEventAt,
    stoppedAt: value.StoppedAt,
  };
}

function toArray<T>(value: unknown, convert: (item: unknown) => T, label: string): T[] {
  if (!Array.isArray(value)) throw invalidResponse(label);
  return value.map(convert);
}

function validId(value: unknown): value is string {
  return typeof value === 'string' && /^[0-9a-f-]{36}$/iu.test(value);
}

function validCount(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}

function validTicks(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}

function validNullableTicks(value: unknown): value is number | null {
  return value === null || validTicks(value);
}

function validNullableYear(value: unknown): value is number | null {
  return value === null || (typeof value === 'number' && Number.isInteger(value));
}

function validDate(value: unknown): value is string {
  return typeof value === 'string' && Number.isFinite(Date.parse(value));
}

function validNullableDate(value: unknown): value is string | null {
  return value === null || validDate(value);
}
