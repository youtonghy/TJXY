import { Button, Label, Pagination, Skeleton } from '@heroui/react';
import { NativeSelect } from '@heroui-pro/react/native-select';
import { RotateCcw } from 'lucide-react';
import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { Link, useParams, useSearchParams } from 'react-router-dom';

import { getItems, getLibraryFilterFacets, type ItemPage, type LibraryFilterFacets } from '../api/catalogApi';
import { MediaTile } from '../ui/MediaTile';

const pageSize = 24;

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
  const totalPages = Math.max(1, Math.ceil((page?.TotalRecordCount ?? 0) / pageSize));
  const hasFilters = Boolean(mediaType || genre || year || sort !== sortOptions[0].value);

  if (!id) return <p>Library not found.</p>;

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
      <div className="flex items-center gap-3 text-sm text-muted"><Link to="/app/">Home</Link><span>/</span><span>Library</span></div>
      <div><h1 className="text-3xl font-semibold">Library</h1><p className="mt-1 text-muted">Browse movies, series, and audio in this collection.</p></div>

      <section aria-label="Library filters" className="border-y border-border py-4">
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-[repeat(4,minmax(0,1fr))_auto] lg:items-end">
          <LibrarySelect label="Media type" name="type" value={mediaType} onChange={(value) => { setFilter('type', value); }}>
            <NativeSelect.Option value="">All media</NativeSelect.Option>
            <NativeSelect.Option value="Movie">Movies</NativeSelect.Option>
            <NativeSelect.Option value="Series">Series</NativeSelect.Option>
            <NativeSelect.Option value="Audio">Audio</NativeSelect.Option>
          </LibrarySelect>
          <LibrarySelect label="Genre" name="genre" value={genre} onChange={(value) => { setFilter('genre', value); }}>
            <NativeSelect.Option value="">All genres</NativeSelect.Option>
            {genres.map((value) => <NativeSelect.Option key={value} value={value}>{value}</NativeSelect.Option>)}
          </LibrarySelect>
          <LibrarySelect label="Year" name="year" value={year} onChange={(value) => { setFilter('year', value); }}>
            <NativeSelect.Option value="">All years</NativeSelect.Option>
            {years.map((value) => <NativeSelect.Option key={value} value={value}>{value}</NativeSelect.Option>)}
          </LibrarySelect>
          <LibrarySelect label="Sort by" name="sort" value={sort} onChange={(value) => { setFilter('sort', value); }}>
            {sortOptions.map((option) => <NativeSelect.Option key={option.value} value={option.value}>{option.label}</NativeSelect.Option>)}
          </LibrarySelect>
          <Button
            aria-label="Clear library filters"
            className="justify-self-start lg:justify-self-auto"
            isDisabled={!hasFilters}
            onPress={() => { setSearchParams({}); }}
            variant="tertiary"
          >
            <RotateCcw aria-hidden="true" className="size-4" />
            Clear
          </Button>
        </div>
      </section>

      {!page ? <LibrarySkeleton /> : page.Items.length ? (
        <>
          <div className="grid grid-cols-2 gap-x-4 gap-y-7 sm:grid-cols-4 lg:grid-cols-6">{page.Items.map((item) => <MediaTile item={item} key={item.Id} />)}</div>
          <div className="flex justify-center">
            <Pagination aria-label="Library pagination">
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
          <p className="font-medium">No titles match these filters</p>
          <p className="mt-1 text-sm text-muted">Clear one or more filters to broaden the library.</p>
          <Button className="mt-4" onPress={() => { setSearchParams({}); }} variant="secondary"><RotateCcw aria-hidden="true" className="size-4" />Clear filters</Button>
        </div>
      )}
    </div>
  );
}

function LibrarySelect({ children, label, name, onChange, value }: { children: ReactNode; label: string; name: string; onChange: (value: string) => void; value: string }) {
  const id = `library-filter-${name}`;
  return (
    <NativeSelect fullWidth variant="secondary">
      <Label htmlFor={id}>{label}</Label>
      <NativeSelect.Trigger id={id} name={name} value={value} onChange={(event) => { onChange(event.currentTarget.value); }}>
        {children}
        <NativeSelect.Indicator />
      </NativeSelect.Trigger>
    </NativeSelect>
  );
}

function LibrarySkeleton() {
  return <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">{Array.from({ length: 12 }, (_, index) => <Skeleton className="aspect-[2/3] rounded-xl" key={index} />)}</div>;
}

function sortedUnique(values: string[]): string[] {
  return [...new Set(values)].sort((left, right) => left.localeCompare(right));
}
