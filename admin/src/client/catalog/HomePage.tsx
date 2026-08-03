import { Alert, Skeleton } from '@heroui/react';
import { useEffect, useState } from 'react';

import {
  getLatest,
  getLibraries,
  getResumeItems,
  latestTypesForLibrary,
  type Library,
  type MediaItem,
} from '../api/catalogApi';
import { MediaRow } from '../ui/MediaRow';
import { useTranslate } from '../../settings/i18n';

const HOME_LIBRARY_LIMIT = 12;
const playbackDestination = (item: MediaItem): string => `/app/play/${item.Id}`;

interface LibraryRow {
  library: Library;
  items: MediaItem[];
}

export function HomePage() {
  const [resume, setResume] = useState<MediaItem[]>([]);
  const [libraryRows, setLibraryRows] = useState<LibraryRow[]>([]);
  const [hasUnavailableLibraries, setHasUnavailableLibraries] = useState(false);
  const [loading, setLoading] = useState(true);
  const tr = useTranslate();

  useEffect(() => {
    let active = true;
    void Promise.all([getResumeItems(), getLibraries()])
      .then(async ([resumeItems, libraries]) => {
        const results = await Promise.allSettled(libraries.map(async (library) => ({
          library,
          items: await getLatest({
            includeItemTypes: latestTypesForLibrary(library),
            limit: HOME_LIBRARY_LIMIT,
            parentId: library.Id,
          }),
        })));
        if (!active) return;
        setResume(resumeItems);
        setLibraryRows(results.flatMap((result) => (
          result.status === 'fulfilled' && result.value.items.length > 0
            ? [result.value]
            : []
        )));
        setHasUnavailableLibraries(results.some((result) => result.status === 'rejected'));
      })
      .catch(() => {
        if (active) setHasUnavailableLibraries(true);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => { active = false; };
  }, []);

  return (
    <div className="space-y-10">
      <div>
        <p className="text-sm font-medium text-accent">{tr('Your library', '你的媒体库')}</p>
        <h1 className="mt-1 text-3xl font-semibold text-foreground sm:text-4xl">
          {tr('What do you want to watch?', '今天想看什么？')}
        </h1>
        <p className="mt-2 max-w-2xl text-muted">
          {tr('Pick up where you left off or explore something new.', '从上次离开的地方继续，或探索新的内容。')}
        </p>
      </div>

      {loading ? (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
          {Array.from({ length: 6 }, (_, index) => (
            <Skeleton className="aspect-[2/3] rounded-xl" key={index} />
          ))}
        </div>
      ) : (
        <>
          {hasUnavailableLibraries && (
            <Alert role="alert" status="warning">
              <Alert.Indicator />
              <Alert.Content>
                <Alert.Title>{tr('Some library sections are unavailable', '部分媒体库暂时不可用')}</Alert.Title>
                <Alert.Description>{tr('Refresh the page to try loading them again.', '请刷新页面后重试。')}</Alert.Description>
              </Alert.Content>
            </Alert>
          )}
          <MediaRow itemTo={playbackDestination} items={resume} title={tr('Continue watching', '继续观看')} />
          {libraryRows.map(({ library, items }) => (
            <MediaRow
              items={items}
              key={library.Id}
              limitToTwoRows
              moreTo={`/app/libraries/${library.Id}`}
              title={library.Name}
            />
          ))}
        </>
      )}
    </div>
  );
}
