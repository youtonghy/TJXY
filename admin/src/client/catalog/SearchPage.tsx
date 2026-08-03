/* eslint-disable react-hooks/set-state-in-effect */
import { SearchField, Skeleton } from '@heroui/react';
import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { getPopular, searchHints, type SearchHint } from '../api/catalogApi';
import { MediaRow } from '../ui/MediaRow';
import { MediaTile } from '../ui/MediaTile';
import { useTranslate } from '../../settings/i18n';

export function SearchPage() {
  const [params, setParams] = useSearchParams();
  const query = params.get('q') ?? '';
  const [value, setValue] = useState(query);
  const [results, setResults] = useState<SearchHint[]>([]);
  const [popular, setPopular] = useState<SearchHint[]>([]);
  const [loading, setLoading] = useState(false);
  const tr = useTranslate();

  useEffect(() => {
    void getPopular(12).then(setPopular).catch(() => { setPopular([]); });
  }, []);
  useEffect(() => {
    setValue(query);
    if (!query.trim()) { setResults([]); return; }
    const timer = window.setTimeout(() => {
      setLoading(true);
      void searchHints(query).then(setResults).catch(() => { setResults([]); }).finally(() => {
        setLoading(false);
      });
    }, 250);
    return () => { window.clearTimeout(timer); };
  }, [query]);

  return (
    <div className="space-y-6">
      <div><h1 className="text-3xl font-semibold">{tr('Search', '搜索')}</h1><p className="mt-1 text-muted">{tr('Find something to watch.', '寻找想看的内容。')}</p></div>
      <SearchField aria-label={tr('Search media', '搜索媒体')} value={value} onChange={(next) => { setValue(next); setParams(next ? { q: next } : {}); }}>
        <SearchField.Group><SearchField.SearchIcon /><SearchField.Input placeholder={tr('Search movies, series, and episodes', '搜索电影、剧集和单集')} /></SearchField.Group>
      </SearchField>
      {loading ? (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">{Array.from({ length: 6 }, (_, index) => <Skeleton className="aspect-[2/3] rounded-xl" key={index} />)}</div>
      ) : results.length ? (
        <div className="grid grid-cols-2 gap-x-4 gap-y-7 sm:grid-cols-4 lg:grid-cols-6">{results.map((item) => <MediaTile item={item} key={item.Id} />)}</div>
      ) : query ? (
        <p className="py-12 text-center text-muted">{tr('No results for', '没有找到')} “{query}”。</p>
      ) : (
        <MediaRow items={popular} limitToTwoRows moreTo="/app/rankings" title={tr('Popular recommendations', '热门推荐')} />
      )}
    </div>
  );
}
