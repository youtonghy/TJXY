/* eslint-disable @typescript-eslint/restrict-template-expressions */
import { clientRequest } from './clientApi';

export interface MediaItem { Id: string; Name: string; Type?: string; MediaType?: string; IsFolder?: boolean; ParentId?: string; ProductionYear?: number; Overview?: string; ImageTags?: Record<string, string>; UserData?: { IsFavorite?: boolean; Played?: boolean; PlaybackPositionTicks?: number }; }
export interface ItemPage { Items: MediaItem[]; TotalRecordCount: number; StartIndex: number; }
export interface Library { Id: string; Name: string; CollectionType?: string; ImageTags?: Record<string, string>; }
export interface SearchHint extends MediaItem { PrimaryImageTag?: string; }

export async function getLibraries(): Promise<Library[]> { const value = await clientRequest<{ Items?: unknown }>('/UserViews'); return Array.isArray(value.Items) ? value.Items.filter(isRecord).map((item) => item as unknown as Library) : []; }
export async function getItems(params: { parentId?: string; startIndex?: number; limit?: number; includeItemTypes?: string } = {}): Promise<ItemPage> {
  const query = new URLSearchParams({ Limit: String(params.limit ?? 24), StartIndex: String(params.startIndex ?? 0) });
  if (params.parentId) query.set('ParentId', params.parentId);
  if (params.includeItemTypes) query.set('IncludeItemTypes', params.includeItemTypes);
  return clientRequest<ItemPage>(`/Items?${query}`);
}
export async function getLatest(limit = 18): Promise<MediaItem[]> { const value = await clientRequest<unknown>(`/Items/Latest?Limit=${limit}`); return Array.isArray(value) ? value.filter(isRecord).map((item) => item as unknown as MediaItem) : []; }
export async function getItem(id: string): Promise<MediaItem> { return clientRequest<MediaItem>(`/Items/${encodeURIComponent(id)}`); }
export async function searchHints(term: string): Promise<SearchHint[]> { const value = await clientRequest<{ SearchHints?: unknown }>(`/Search/Hints?SearchTerm=${encodeURIComponent(term)}&Limit=24`); return Array.isArray(value.SearchHints) ? value.SearchHints.filter(isRecord).map((item) => item as unknown as SearchHint) : []; }
export async function toggleFavorite(userId: string, itemId: string, favorite: boolean): Promise<void> { await clientRequest(`/Users/${userId}/FavoriteItems/${itemId}`, { method: favorite ? 'POST' : 'DELETE' }); }
export async function togglePlayed(userId: string, itemId: string, played: boolean): Promise<void> { await clientRequest(`/Users/${userId}/PlayedItems/${itemId}`, { method: played ? 'POST' : 'DELETE' }); }
function isRecord(value: unknown): value is Record<string, unknown> { return typeof value === 'object' && value !== null; }
