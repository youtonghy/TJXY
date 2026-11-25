import type { MediaItem } from '../api/catalogApi';

export type ExternalLinkProvider = 'tmdb' | 'imdb' | 'tvdb';

export interface ExternalLink {
  provider: ExternalLinkProvider;
  label: string;
  url: string;
}

const NUMERIC_ID = /^[1-9][0-9]{0,11}$/;
const IMDB_ID = /^tt[0-9]{7,9}$/;
// IMDb has no season pages; it only titles movies, series, and episodes.
const IMDB_ITEM_TYPES = new Set(['Movie', 'Series', 'Episode']);

/**
 * Builds TMDB, IMDb, and TheTVDB page links from catalog provider IDs.
 *
 * Seasons and episodes do not carry a usable TMDB page ID of their own, so
 * their TMDB links are derived from the parent series ID plus the season and
 * episode numbers taken from the item's ancestors.
 */
export function buildExternalLinks(item: MediaItem, ancestors: readonly MediaItem[] = []): ExternalLink[] {
  const links: ExternalLink[] = [];
  const series = ancestors.find((ancestor) => ancestor.Type === 'Series');
  const season = ancestors.find((ancestor) => ancestor.Type === 'Season');
  const seriesTmdb = series ? numericId(series, 'tmdb') : undefined;

  const tmdbPath = (() => {
    switch (item.Type) {
      case 'Movie': {
        const id = numericId(item, 'tmdb');
        return id ? `movie/${id}` : undefined;
      }
      case 'Series': {
        const id = numericId(item, 'tmdb');
        return id ? `tv/${id}` : undefined;
      }
      case 'Season': {
        const seasonNumber = indexNumber(item);
        return seriesTmdb && seasonNumber !== undefined ? `tv/${seriesTmdb}/season/${seasonNumber}` : undefined;
      }
      case 'Episode': {
        const seasonNumber = season ? indexNumber(season) : undefined;
        const episodeNumber = indexNumber(item);
        return seriesTmdb && seasonNumber !== undefined && episodeNumber !== undefined
          ? `tv/${seriesTmdb}/season/${seasonNumber}/episode/${episodeNumber}`
          : undefined;
      }
      default:
        return undefined;
    }
  })();
  if (tmdbPath) links.push({ provider: 'tmdb', label: 'TMDB', url: `https://www.themoviedb.org/${tmdbPath}` });

  const imdb = providerId(item, 'imdb');
  if (imdb && IMDB_ID.test(imdb) && IMDB_ITEM_TYPES.has(item.Type ?? '')) {
    links.push({ provider: 'imdb', label: 'IMDb', url: `https://www.imdb.com/title/${imdb}/` });
  }

  const tvdbKind = { Episode: 'episode', Movie: 'movie', Season: 'season', Series: 'series' }[item.Type ?? ''];
  const tvdb = numericId(item, 'tvdb');
  if (tvdbKind && tvdb) {
    links.push({ provider: 'tvdb', label: 'TVDB', url: `https://thetvdb.com/dereferrer/${tvdbKind}/${tvdb}` });
  }

  return links;
}

function providerId(item: MediaItem, provider: ExternalLinkProvider) {
  const entry = Object.entries(item.ProviderIds ?? {}).find(([key]) => key.toLowerCase() === provider);
  return entry?.[1].trim();
}

function numericId(item: MediaItem, provider: ExternalLinkProvider) {
  const value = providerId(item, provider);
  return value && NUMERIC_ID.test(value) ? value : undefined;
}

function indexNumber(item: MediaItem) {
  const value = item.IndexNumber;
  return value !== undefined && Number.isSafeInteger(value) && value >= 0 ? String(value) : undefined;
}
