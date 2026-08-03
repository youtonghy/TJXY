/* eslint-disable @typescript-eslint/restrict-template-expressions */
import { clientRequest } from './clientApi';

export interface MediaNamedCode { Code: string; Name: string; }
export interface MediaPerson { Id: string; Name: string; Role?: string; Type?: string; }
export interface MediaUserData { IsFavorite?: boolean; Played?: boolean; PlaybackPositionTicks?: number; }
export interface MediaItem {
  Id: string;
  Name: string;
  Type?: string;
  MediaType?: string;
  IsFolder?: boolean;
  ParentId?: string;
  ProductionYear?: number;
  Overview?: string;
  OriginalTitle?: string;
  CommunityRating?: number;
  IndexNumber?: number;
  Tagline?: string;
  VoteCount?: number;
  RunTimeTicks?: number;
  PremiereDate?: string;
  EndDate?: string;
  Status?: string;
  OfficialRating?: string;
  OriginalLanguage?: string;
  Genres?: string[];
  Studios?: string[];
  Countries?: MediaNamedCode[];
  Languages?: MediaNamedCode[];
  People?: MediaPerson[];
  ProviderIds?: Record<string, string>;
  HasMediaSources?: boolean;
  PrimaryImageTag?: string;
  ImageTags?: Record<string, string>;
  UserData?: MediaUserData;
}
export interface ItemPage { Items: MediaItem[]; TotalRecordCount: number; StartIndex: number; }
export interface Library { Id: string; Name: string; CollectionType?: string; ImageTags?: Record<string, string>; }
export interface LatestItemsOptions { limit?: number; parentId?: string; includeItemTypes?: string; }
export interface GetItemsOptions {
  genre?: string;
  includeItemTypes?: string;
  limit?: number;
  parentId?: string;
  productionYear?: number;
  recursive?: boolean;
  sortBy?: 'DateCreated' | 'ProductionYear' | 'Runtime' | 'SortName';
  sortOrder?: 'Ascending' | 'Descending';
  startIndex?: number;
}
export interface LibraryFilterFacets { Genres: string[]; ProductionYears: number[] }
export type SearchHint = MediaItem;

export async function getLibraries(): Promise<Library[]> { const value = await clientRequest<{ Items?: unknown }>('/UserViews'); return Array.isArray(value.Items) ? value.Items.filter(isRecord).map((item) => item as unknown as Library) : []; }
export async function getItems(params: GetItemsOptions = {}): Promise<ItemPage> {
  const query = new URLSearchParams({ limit: String(params.limit ?? 24), startIndex: String(params.startIndex ?? 0) });
  if (params.parentId) query.set('parentId', params.parentId);
  if (params.includeItemTypes) query.set('includeItemTypes', params.includeItemTypes);
  if (params.genre) query.set('genre', params.genre);
  if (params.productionYear) query.set('productionYear', String(params.productionYear));
  if (params.recursive !== undefined) query.set('recursive', String(params.recursive));
  if (params.sortBy) query.set('sortBy', params.sortBy);
  if (params.sortOrder) query.set('sortOrder', params.sortOrder);
  return clientRequest<ItemPage>(`/Items?${query}`);
}
export function getLibraryFilterFacets(parentId: string): Promise<LibraryFilterFacets> {
  const query = new URLSearchParams({ parentId });
  return clientRequest<LibraryFilterFacets>(`/Items/Filters?${query}`);
}
export async function getLatest(options: number | LatestItemsOptions = 18): Promise<MediaItem[]> {
  const normalized = typeof options === 'number' ? { limit: options } : options;
  const query = new URLSearchParams({ limit: String(normalized.limit ?? 18) });
  if (normalized.parentId) query.set('parentId', normalized.parentId);
  if (normalized.includeItemTypes) query.set('includeItemTypes', normalized.includeItemTypes);
  const value = await clientRequest<unknown>(`/Items/Latest?${query}`);
  return Array.isArray(value)
    ? value.filter(isRecord).map((item) => item as unknown as MediaItem)
    : [];
}
export async function getResumeItems(limit = 12): Promise<MediaItem[]> { const value = await clientRequest<ItemPage>(`/UserItems/Resume?mediaTypes=Video&limit=${limit}&enableUserData=true`); return Array.isArray(value.Items) ? value.Items : []; }
export async function getPopular(limit = 12): Promise<MediaItem[]> {
  let value: ItemPage;
  try {
    value = await clientRequest<ItemPage>(`/Discover/Popular?limit=${limit}`);
  } catch {
    return getLatest({ limit, includeItemTypes: 'Movie,Series' });
  }
  const summaries = Array.isArray(value.Items) && value.Items.length > 0
    ? value.Items
    : (await getLatest({ limit, includeItemTypes: 'Movie,Series' }));
  return Promise.all(summaries.map(async (summary) => {
    try {
      return await getItem(summary.Id);
    } catch {
      return summary;
    }
  }));
}
export async function getItem(id: string): Promise<MediaItem> { return clientRequest<MediaItem>(`/Items/${encodeURIComponent(id)}`); }
export async function getChildren(parentId: string): Promise<MediaItem[]> { return (await getItems({ parentId, limit: 200 })).Items; }
export async function searchHints(term: string): Promise<SearchHint[]> { const value = await clientRequest<{ SearchHints?: unknown }>(`/Search/Hints?searchTerm=${encodeURIComponent(term)}&limit=24`); return Array.isArray(value.SearchHints) ? value.SearchHints.filter(isRecord).map((item) => item as unknown as SearchHint) : []; }
export async function toggleFavorite(userId: string, itemId: string, favorite: boolean): Promise<void> { await clientRequest(`/Users/${userId}/FavoriteItems/${itemId}`, { method: favorite ? 'POST' : 'DELETE' }); }
export async function togglePlayed(userId: string, itemId: string, played: boolean): Promise<void> { await clientRequest(`/Users/${userId}/PlayedItems/${itemId}`, { method: played ? 'POST' : 'DELETE' }); }
function isRecord(value: unknown): value is Record<string, unknown> { return typeof value === 'object' && value !== null; }

export function latestTypesForLibrary(library: Library): string | undefined {
  if (library.CollectionType === 'movies') return 'Movie';
  if (library.CollectionType === 'tvshows') return 'Series';
  if (library.CollectionType === 'music') return 'Audio';
  return undefined;
}
