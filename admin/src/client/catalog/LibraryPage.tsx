import { Button, Disclosure, DisclosureGroup, ListBox, Pagination, Skeleton } from '@heroui/react';
import { CellSelect } from '@heroui-pro/react/cell-select';
import { RotateCcw } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { Link, useParams, useSearchParams } from 'react-router-dom';

import { getItems, getLibraryFilterFacets, type ItemPage, type LibraryFilterFacets } from '../api/catalogApi';
import { MediaTile } from '../ui/MediaTile';
import { useTranslate } from '../../settings/i18n';

const pageSize = 24;
const allFilterKey = '__all__';

const sortOptions = [
  { label: 'Title A-Z', value: 'SortName:Ascending' },
  { label: 'Title Z-A', value: 'SortName:Descending' },
  { label: 'Newest release', value: 'ProductionYear:Descending' },
  { label: 'Oldest release', value: 'ProductionYear:Ascending' },
  { label: 'Recently added', value: 'DateCreated:Descending' },
] as const;

export function LibraryPage() {
  const { id } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const [page, setPage] = useState<ItemPage>();
  const [facets, setFacets] = useState<LibraryFilterFacets>({ Genres: [], ProductionYears: [] });
  const tr = useTranslate();
  const mediaType = searchParams.get('type') ?? '';
  const genre = searchParams.get('genre') ?? '';
  const year = searchParams.get('year') ?? '';
  const sort = searchParams.get('sort') ?? sortOptions[0].value;
  const currentPage = Math.max(1, Number(searchParams.get('page') ?? '1') || 1);
  const [sortBy, sortOrder] = sort.split(':') as [
    'DateCreated' | 'ProductionYear' | 'Runtime' | 'SortName',
    'Ascending' | 'Descending',
  ];

  useEffect(() => {
    if (!id) return;
    let active = true;
    void getItems({
      genre: genre || undefined,
      includeItemTypes: mediaType || undefined,
      parentId: id,
      productionYear: year ? Number(year) : undefined,
      recursive: Boolean(mediaType || genre || year),
      sortBy,
      sortOrder,
      startIndex: (currentPage - 1) * pageSize,
    }).then((nextPage) => { if (active) setPage(nextPage); });
    return () => { active = false; };
  }, [currentPage, genre, id, mediaType, sortBy, sortOrder, year]);

  useEffect(() => {
    if (!id) return;
    let active = true;
    void getLibraryFilterFacets(id).then((nextFacets) => { if (active) setFacets(nextFacets); });
    return () => { active = false; };
  }, [id]);

  const genres = useMemo(() => sortedUnique([
    ...(genre ? [genre] : []),
    ...facets.Genres,
  ]), [facets.Genres, genre]);
  const years = useMemo(() => sortedUnique([
    ...(year ? [year] : []),
    ...facets.ProductionYears.map(String),
  ]).sort((left, right) => Number(right) - Number(left)), [facets.ProductionYears, year]);
  const mediaTypeOptions = [
    { label: tr('All media', '全部媒体'), value: '' },
    { label: tr('Movies', '电影'), value: 'Movie' },
    { label: tr('Series', '剧集'), value: 'Series' },
    { label: tr('Audio', '音频'), value: 'Audio' },
  ];
  const genreOptions = [
    { label: tr('All genres', '全部类型'), value: '' },
    ...genres.map((value) => ({ label: value, value })),
  ];
  const yearOptions = [
    { label: tr('All years', '全部年份'), value: '' },
    ...years.map((value) => ({ label: value, value })),
  ];
  const translatedSortOptions = sortOptions.map((option) => ({
    label: tr(option.label, ({ 'Title A-Z': '标题 A-Z', 'Title Z-A': '标题 Z-A', 'Newest release': '最新上映', 'Oldest release': '最早上映', 'Recently added': '最近添加' } as Record<string, string>)[option.label] ?? option.label),
    value: option.value,
  }));
  const totalPages = Math.max(1, Math.ceil((page?.TotalRecordCount ?? 0) / pageSize));
  const hasFilters = Boolean(mediaType || genre || year || sort !== sortOptions[0].value);

  if (!id) return <p>{tr('Library not found.', '找不到媒体库。')}</p>;

  const setFilter = (name: string, value: string) => {
    setSearchParams((current) => {
      const next = new URLSearchParams(current);
      if (value) next.set(name, value); else next.delete(name);
      if (name !== 'page') next.delete('page');
      return next;
    });
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3 text-sm text-muted"><Link to="/app/">{tr('Home', '首页')}</Link><span>/</span><span>{tr('Library', '媒体库')}</span></div>
      <div><h1 className="text-3xl font-semibold">{tr('Library', '媒体库')}</h1><p className="mt-1 text-muted">{tr('Browse movies, series, and audio in this collection.', '浏览此媒体库中的电影、剧集和音频。')}</p></div>

      <section aria-label={tr('Library filters', '媒体库筛选')}>
        <DisclosureGroup className="overflow-hidden rounded-lg border border-border bg-surface">
          <Disclosure id="library-filters">
            <Disclosure.Heading>
              <Disclosure.Trigger className="flex min-h-10 w-full items-center justify-between gap-3 rounded-lg px-3 text-left text-sm font-medium hover:bg-default">
                <span>{tr('Filter titles', '筛选影片')}</span>
                <Disclosure.Indicator className="size-4 shrink-0 text-muted" />
              </Disclosure.Trigger>
            </Disclosure.Heading>
            <Disclosure.Content>
              <Disclosure.Body className="border-t border-border p-3">
                <div className="mb-2 flex justify-end">
                  <Button
                    aria-label={tr('Clear library filters', '清除媒体库筛选')}
                    isDisabled={!hasFilters}
                    onPress={() => { setSearchParams({}); }}
                    size="sm"
                    variant="tertiary"
                  >
                    <RotateCcw aria-hidden="true" className="size-4" />
                    {tr('Clear filters', '清除筛选')}
                  </Button>
                </div>
                <div className="grid gap-2 sm:grid-cols-2">
                  <LibraryCellSelect label={tr('Media type', '媒体类型')} options={mediaTypeOptions} value={mediaType} onChange={(value) => { setFilter('type', value); }} />
                  <LibraryCellSelect label={tr('Genre', '类型')} options={genreOptions} value={genre} onChange={(value) => { setFilter('genre', value); }} />
                  <LibraryCellSelect label={tr('Year', '年份')} options={yearOptions} value={year} onChange={(value) => { setFilter('year', value); }} />
                  <LibraryCellSelect label={tr('Sort by', '排序')} options={translatedSortOptions} value={sort} onChange={(value) => { setFilter('sort', value); }} />
                </div>
              </Disclosure.Body>
            </Disclosure.Content>
          </Disclosure>
        </DisclosureGroup>
      </section>

      {!page ? <LibrarySkeleton /> : page.Items.length ? (
        <>
          <div className="grid grid-cols-2 gap-x-4 gap-y-7 sm:grid-cols-4 lg:grid-cols-6">{page.Items.map((item) => <MediaTile item={item} key={item.Id} />)}</div>
          <div className="flex justify-center">
            <Pagination aria-label={tr('Library pagination', '媒体库分页')}>
              <Pagination.Content>
                <Pagination.Item><Pagination.Previous isDisabled={currentPage === 1} onPress={() => { setFilter('page', String(currentPage - 1)); }}><Pagination.PreviousIcon /></Pagination.Previous></Pagination.Item>
                <Pagination.Item><Pagination.Link isActive isDisabled>{currentPage}</Pagination.Link></Pagination.Item>
                <Pagination.Item><Pagination.Next isDisabled={currentPage >= totalPages} onPress={() => { setFilter('page', String(currentPage + 1)); }}><Pagination.NextIcon /></Pagination.Next></Pagination.Item>
              </Pagination.Content>
            </Pagination>
          </div>
        </>
      ) : (
        <div className="rounded-xl border border-dashed border-border p-10 text-center">
          <p className="font-medium">{tr('No titles match these filters', '没有符合筛选条件的内容')}</p>
          <p className="mt-1 text-sm text-muted">{tr('Clear one or more filters to broaden the library.', '清除一个或多个筛选条件以扩大结果范围。')}</p>
          <Button className="mt-4" onPress={() => { setSearchParams({}); }} variant="secondary"><RotateCcw aria-hidden="true" className="size-4" />{tr('Clear filters', '清除筛选')}</Button>
        </div>
      )}
    </div>
  );
}

function LibraryCellSelect({ label, onChange, options, value }: { label: string; onChange: (value: string) => void; options: { label: string; value: string }[]; value: string }) {
  return (
    <CellSelect
      aria-label={label}
      className="w-full"
      onChange={(nextValue) => { onChange(nextValue === allFilterKey ? '' : String(nextValue ?? '')); }}
      value={value || allFilterKey}
      variant="secondary"
    >
      <CellSelect.Trigger className="w-full">
        <CellSelect.Label>{label}</CellSelect.Label>
        <CellSelect.Value />
        <CellSelect.Indicator />
      </CellSelect.Trigger>
      <CellSelect.Popover>
        <ListBox>
          {options.map((option) => (
            <ListBox.Item id={option.value || allFilterKey} key={option.value || allFilterKey} textValue={option.label}>
              {option.label}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </CellSelect.Popover>
    </CellSelect>
  );
}

function LibrarySkeleton() {
  return <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">{Array.from({ length: 12 }, (_, index) => <Skeleton className="aspect-[2/3] rounded-xl" key={index} />)}</div>;
}

function sortedUnique(values: string[]): string[] {
  return [...new Set(values)].sort((left, right) => left.localeCompare(right));
}
