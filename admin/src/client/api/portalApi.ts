import type { MediaItem } from './catalogApi';
import { clientRequest } from './clientApi';

export type InsightRange = 'today' | '7d' | '30d' | 'all';
export type TmdbMediaType = 'Movie' | 'Series';

export interface UserProfile { Username: string; Bio: string }
export interface PersonalSession {
  Id: string;
  DeviceId: string;
  DeviceName: string;
  ClientName: string;
  ApplicationVersion: string;
  CreatedAt: string;
  LastActivityDate: string;
  IsCurrent: boolean;
}
export interface UpdateProfileRequest { Username: string; Bio: string; CurrentPassword: string; NewPassword?: string }
export interface ChangePasswordRequest { CurrentPassword: string; NewPassword: string }
export interface InsightDailyPoint { Date: string; WatchedTicks: number }
export interface InsightGenre { Name: string; WatchedTicks: number }
export interface InsightTimelineEvent {
  At: string;
  ItemId: string;
  Kind: 'MovieWatched' | 'SeriesCompleted' | 'SeriesStarted';
  Name: string;
}
export interface UserInsights {
  WatchedTicks: number;
  PlayCount: number;
  UniqueTitles: number;
  Media: { Movies: number; Series: number };
  Daily: InsightDailyPoint[];
  Genres: InsightGenre[];
  Recent: MediaItem[];
  Timeline: InsightTimelineEvent[];
}
export interface TmdbRankingItem {
  Rank: number;
  TmdbId: number;
  Name: string;
  Overview?: string;
  ProductionYear?: number;
  Rating?: number;
  PosterUrl?: string;
  LocalItemId?: string;
}
export interface ServerRankingItem {
  Rank: number;
  Id: string;
  Name: string;
  ItemType: string;
  ProductionYear?: number;
  Overview?: string;
  PrimaryImageTag?: string;
  PosterUrl?: string;
  PlayCount: number;
  UniqueViewers: number;
}

export function getProfile(): Promise<UserProfile> {
  return clientRequest<UserProfile>('/Users/Me/Profile');
}

export function updateProfile(request: UpdateProfileRequest): Promise<UserProfile> {
  return clientRequest<UserProfile>('/Users/Me/Profile', {
    body: JSON.stringify(request),
    method: 'PATCH',
  });
}

export function changePassword(request: ChangePasswordRequest): Promise<void> {
  return clientRequest('/Users/Me/Password', { body: JSON.stringify(request), method: 'POST' });
}

export function listPersonalSessions(signal?: AbortSignal): Promise<PersonalSession[]> {
  return clientRequest<PersonalSession[]>('/Users/Me/Sessions', signal === undefined ? {} : { signal });
}

export function revokePersonalSession(id: string): Promise<void> {
  return clientRequest(`/Users/Me/Sessions/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

export function getUserInsights(range: InsightRange): Promise<UserInsights> {
  return clientRequest<UserInsights>(`/Users/Me/Insights?range=${range}`);
}

export async function getTmdbRanking(mediaType: TmdbMediaType): Promise<TmdbRankingItem[]> {
  const value = await clientRequest<{ Items?: TmdbRankingItem[] }>(`/Discover/Tmdb/Popular?mediaType=${mediaType}`);
  return Array.isArray(value.Items) ? value.Items : [];
}

export async function getServerRanking(): Promise<ServerRankingItem[]> {
  const value = await clientRequest<{ Items?: ServerRankingItem[] }>('/Discover/Server/Top?period=yesterday&limit=20');
  return Array.isArray(value.Items) ? value.Items : [];
}
