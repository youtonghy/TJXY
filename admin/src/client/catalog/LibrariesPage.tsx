import { Alert, Skeleton } from '@heroui/react';
import { useEffect, useState } from 'react';
import {
  getLatest,
  getLibraries,
  latestTypesForLibrary,
  type Library,
  type MediaItem,
} from '../api/catalogApi';
import { MediaRow } from '../ui/MediaRow';
import { useTranslate } from '../../settings/i18n';

interface LibraryRow { library: Library; items: MediaItem[] }

export function LibrariesPage() {
  const [rows, setRows] = useState<LibraryRow[]>();
  const [unavailable, setUnavailable] = useState(false);
  const tr = useTranslate();

  useEffect(() => {
    let active = true;
    void getLibraries().then(async (libraries) => {
      const results = await Promise.allSettled(libraries.map(async (library) => ({
        library,
        items: await getLatest({
          includeItemTypes: latestTypesForLibrary(library),
          limit: 12,
          parentId: library.Id,
        }),
      })));
      if (!active) return;
      setRows(results.flatMap((result) => result.status === 'fulfilled' ? [result.value] : []));
      setUnavailable(results.some((result) => result.status === 'rejected'));
    }).catch(() => { if (active) { setRows([]); setUnavailable(true); } });
    return () => { active = false; };
  }, []);

  return (
    <div className="space-y-8">
      <div>
        <p className="text-sm font-medium text-accent">{tr('Your collections', '你的收藏')}</p>
        <h1 className="mt-1 text-3xl font-semibold">{tr('Libraries', '媒体库')}</h1>
        <p className="mt-1 text-muted">{tr('Browse every collection available to your account.', '浏览此账户可用的全部媒体库。')}</p>
      </div>
      {unavailable && (
        <Alert role="alert" status="warning">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>{tr('Some libraries are unavailable', '部分媒体库暂时不可用')}</Alert.Title>
            <Alert.Description>{tr('Refresh the page to try again.', '请刷新页面后重试。')}</Alert.Description>
          </Alert.Content>
        </Alert>
      )}
      {!rows ? (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
          {Array.from({ length: 12 }, (_, index) => (
            <Skeleton className="aspect-[2/3] rounded-xl" key={index} />
          ))}
        </div>
      ) : rows.length ? rows.map(({ library, items }) => (
        <MediaRow
          items={items}
          key={library.Id}
          limitToTwoRows
          moreTo={`/app/libraries/${library.Id}`}
          title={library.Name}
        />
      )) : (
        <p className="rounded-xl border border-dashed border-border p-10 text-center text-muted">
          {tr('No libraries are available.', '暂无可用的媒体库。')}
        </p>
      )}
    </div>
  );
}
