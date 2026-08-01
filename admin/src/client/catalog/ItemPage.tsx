/* eslint-disable @typescript-eslint/no-misused-promises */
import { Alert, Avatar, Breadcrumbs, Button, Card, Chip, Skeleton, ToggleButton, ToggleButtonGroup } from '@heroui/react';
import { CalendarDays, Check, ChevronDown, ChevronUp, Clock3, Heart, Info, Play, Star } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { useClientAuth } from '../auth/ClientAuthContext';
import { getChildren, getItem, getLibraries, toggleFavorite, togglePlayed, type Library, type MediaItem } from '../api/catalogApi';
import { MediaImage } from '../ui/MediaImage';

interface ItemBreadcrumbContext {
  ancestors: MediaItem[];
  itemId?: string;
  library?: Library;
}

export function ItemPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const { user } = useClientAuth();
  const [item, setItem] = useState<MediaItem>();
  const [children, setChildren] = useState<MediaItem[]>([]);
  const [breadcrumbContext, setBreadcrumbContext] = useState<ItemBreadcrumbContext>({ ancestors: [] });
  const [failed, setFailed] = useState(false);
  const breadcrumbItemId = item?.Id;
  const breadcrumbParentId = item?.ParentId;
  const breadcrumbItemType = item?.Type;

  useEffect(() => {
    if (!id) return;
    void Promise.resolve()
      .then(() => {
        setFailed(false);
        setItem(undefined);
        setChildren([]);
        return getItem(id);
      })
      .then((nextItem) => {
        setItem(nextItem);
        if (nextItem.IsFolder) return getChildren(nextItem.Id).then(setChildren);
        return undefined;
      })
      .catch(() => {
        setFailed(true);
      });
  }, [id]);

  useEffect(() => {
    if (!breadcrumbItemId) return;
    let active = true;
    void loadItemBreadcrumbContext(breadcrumbItemId, breadcrumbParentId, breadcrumbItemType)
      .then((context) => {
        if (active) setBreadcrumbContext({ ...context, itemId: breadcrumbItemId });
      })
      .catch(() => {
        if (active) setBreadcrumbContext({ ancestors: [], itemId: breadcrumbItemId });
      });
    return () => {
      active = false;
    };
  }, [breadcrumbItemId, breadcrumbItemType, breadcrumbParentId]);

  if (!id) return <NotFound />;
  if (failed) {
    return (
      <Alert status="danger" role="alert">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Title>Unable to load this title</Alert.Title>
          <Alert.Description>Try again from the library.</Alert.Description>
        </Alert.Content>
      </Alert>
    );
  }
  if (!item) return <ItemSkeleton />;

  const favorite = item.UserData?.IsFavorite === true;
  const played = item.UserData?.Played === true;
  const seasons = children.filter((child) => child.Type === 'Season').sort(sortByIndex);
  const episodes = children.filter((child) => child.Type === 'Episode').sort(sortByIndex);
  const hasPlayableAction = item.Type !== 'Series' && item.Type !== 'Season';

  return (
    <article className="space-y-8">
      <ItemBreadcrumb context={breadcrumbContext} item={item} onNavigate={navigate} />

      <div className="grid gap-8 lg:grid-cols-[15rem_minmax(0,1fr)]">
        <div className="mx-auto aspect-[2/3] w-full max-w-[15rem] overflow-hidden rounded-2xl bg-default shadow-sm lg:mx-0">
          <MediaImage
            alt={`Poster for ${item.Name}`}
            className="h-full w-full object-cover"
            itemId={id}
            tag={item.ImageTags?.Primary}
          />
        </div>

        <div className="min-w-0 self-center">
          <div className="flex flex-wrap items-center gap-2">
            <Chip color="accent" variant="soft">
              {item.Type ?? 'Media'}
            </Chip>
            {item.ProductionYear && <Chip variant="secondary">{item.ProductionYear}</Chip>}
            {item.Status && <Chip color="success" variant="soft">{item.Status}</Chip>}
            {item.OfficialRating && <Chip variant="secondary">{item.OfficialRating}</Chip>}
          </div>
          <h1 className="mt-4 text-4xl font-semibold tracking-tight text-foreground">{item.Name}</h1>
          {item.OriginalTitle && item.OriginalTitle !== item.Name && (
            <p className="mt-2 text-sm text-muted">{item.OriginalTitle}</p>
          )}
          {item.Tagline && <p className="mt-4 text-lg italic text-accent">“{item.Tagline}”</p>}
          {item.Overview && <p className="mt-5 max-w-3xl text-base leading-7 text-muted">{item.Overview}</p>}

          <div className="mt-7 flex flex-wrap gap-3">
            {hasPlayableAction && (
              <Button onPress={() => navigate(`/app/play/${id}`)}>
                <Play className="size-4" />
                Play
              </Button>
            )}
            <Button
              variant="secondary"
              onPress={() => {
                if (user) {
                  void toggleFavorite(user.Id, id, !favorite).then(() => {
                    setItem({ ...item, UserData: { ...item.UserData, IsFavorite: !favorite } });
                  });
                }
              }}
            >
              <Heart className="size-4" />
              {favorite ? 'Unfavorite' : 'Favorite'}
            </Button>
            <Button
              variant="tertiary"
              onPress={() => {
                if (user) {
                  void togglePlayed(user.Id, id, !played).then(() => {
                    setItem({ ...item, UserData: { ...item.UserData, Played: !played } });
                  });
                }
              }}
            >
              <Check className="size-4" />
              {played ? 'Mark unplayed' : 'Mark played'}
            </Button>
          </div>
        </div>
      </div>

      {item.HasMediaSources === false && (
        <Alert status="warning">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>No video source available</Alert.Title>
            <Alert.Description>
              Add a media file to this title before starting playback.
            </Alert.Description>
          </Alert.Content>
        </Alert>
      )}

      {seasons.length ? <SeasonBrowser seasons={seasons} /> : null}
      {episodes.length ? <EpisodeRail episodes={episodes} label="Episodes" /> : null}

      <div className="grid gap-4 md:grid-cols-2">
        <MetadataCard item={item} />
        <TaxonomyCard item={item} />
      </div>

      {item.People?.length ? <PeopleCard people={item.People} /> : null}
    </article>
  );
}

function ItemBreadcrumb({
  context,
  item,
  onNavigate,
}: {
  context: ItemBreadcrumbContext;
  item: MediaItem;
  onNavigate: (to: string) => void;
}) {
  const entries = [
    { id: 'home', label: 'Home', to: '/app/' },
    { id: 'libraries', label: 'Libraries', to: '/app/libraries' },
    ...(context.itemId === item.Id && context.library
      ? [{ id: `library-${context.library.Id}`, label: context.library.Name, to: `/app/libraries/${context.library.Id}` }]
      : []),
    ...(context.itemId === item.Id ? context.ancestors : []).map((ancestor) => ({
      id: `item-${ancestor.Id}`,
      label: ancestor.Name,
      to: `/app/items/${ancestor.Id}`,
    })),
    { id: `current-${item.Id}`, label: item.Name },
  ];

  return (
    <nav aria-label="Item breadcrumb" className="max-w-full overflow-x-auto pb-1">
      <Breadcrumbs aria-label="Item breadcrumb" className="min-w-max flex-nowrap">
        {entries.map((entry, index) => {
          const isCurrent = index === entries.length - 1;
          return (
            <Breadcrumbs.Item
              isDisabled={isCurrent}
              key={entry.id}
              onPress={isCurrent || !entry.to ? undefined : () => { onNavigate(entry.to); }}
            >
              {entry.label}
            </Breadcrumbs.Item>
          );
        })}
      </Breadcrumbs>
    </nav>
  );
}

function MetadataCard({ item }: { item: MediaItem }) {
  const facts = [
    item.CommunityRating !== undefined
      ? { icon: Star, label: 'Rating', value: `${item.CommunityRating.toFixed(1)}${item.VoteCount ? ` · ${item.VoteCount.toLocaleString()} votes` : ''}` }
      : undefined,
    item.RunTimeTicks ? { icon: Clock3, label: 'Runtime', value: formatRuntime(item.RunTimeTicks) } : undefined,
    item.PremiereDate ? { icon: CalendarDays, label: 'Premiere', value: formatDate(item.PremiereDate) } : undefined,
    item.EndDate ? { icon: CalendarDays, label: 'Ended', value: formatDate(item.EndDate) } : undefined,
    item.OriginalLanguage ? { icon: Info, label: 'Original language', value: item.OriginalLanguage.toUpperCase() } : undefined,
  ].filter((fact): fact is { icon: typeof Star; label: string; value: string } => Boolean(fact));

  return (
    <Card>
      <Card.Header>
        <Card.Title>Details</Card.Title>
        <Card.Description>Key information from the catalog record.</Card.Description>
      </Card.Header>
      <Card.Content className="grid gap-4 sm:grid-cols-2">
        {facts.map(({ icon: Icon, label, value }) => (
          <div className="flex items-start gap-3" key={label}>
            <Icon className="mt-0.5 size-4 shrink-0 text-accent" />
            <div className="min-w-0">
              <p className="text-xs text-muted">{label}</p>
              <p className="mt-1 break-words text-sm font-medium">{value}</p>
            </div>
          </div>
        ))}
        {!facts.length && <p className="text-sm text-muted">No additional details are available.</p>}
      </Card.Content>
    </Card>
  );
}

function TaxonomyCard({ item }: { item: MediaItem }) {
  const values = [
    ['Genres', item.Genres],
    ['Studios', item.Studios],
    ['Countries', item.Countries?.map((value) => value.Name)],
    ['Languages', item.Languages?.map((value) => value.Name)],
  ] as const;
  const visible = values.filter(([, entries]) => entries?.length);
  return (
    <Card>
      <Card.Header>
        <Card.Title>Classification</Card.Title>
        <Card.Description>Genres, production, and language metadata.</Card.Description>
      </Card.Header>
      <Card.Content className="space-y-4">
        {visible.length ? visible.map(([label, entries]) => (
          <div className="space-y-2" key={label}>
            <p className="text-xs text-muted">{label}</p>
            <div className="flex flex-wrap gap-2">
              {entries?.map((entry) => <Chip key={entry} size="sm" variant="secondary">{entry}</Chip>)}
            </div>
          </div>
        )) : <p className="text-sm text-muted">No classification metadata is available.</p>}
      </Card.Content>
    </Card>
  );
}

function PeopleCard({ people }: { people: NonNullable<MediaItem['People']> }) {
  const previewCount = useCreditPreviewCount();
  const [expanded, setExpanded] = useState(false);
  const visiblePeople = expanded ? people : people.slice(0, previewCount);
  return (
    <Card>
      <Card.Header>
        <Card.Title>Cast and crew</Card.Title>
        <Card.Description>People credited on this title.</Card.Description>
      </Card.Header>
      <Card.Content className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {visiblePeople.map((person) => (
          <div className="flex min-w-0 items-center gap-3" key={`${person.Id}-${person.Role ?? ''}`}>
            <Avatar color={person.Type === 'Crew' ? 'accent' : 'default'} size="sm">
              <Avatar.Fallback>{initials(person.Name)}</Avatar.Fallback>
            </Avatar>
            <div className="min-w-0">
              <p className="truncate text-sm font-medium">{person.Name}</p>
              <p className="truncate text-xs text-muted">{[person.Role, person.Type].filter(Boolean).join(' · ')}</p>
            </div>
          </div>
        ))}
      </Card.Content>
      {people.length > previewCount && (
        <Card.Footer>
          <Button
            onPress={() => {
              setExpanded((current) => !current);
            }}
            size="sm"
            variant="ghost"
          >
            {expanded
              ? <ChevronUp aria-hidden="true" className="size-4" />
              : <ChevronDown aria-hidden="true" className="size-4" />}
            {expanded ? 'Show fewer credits' : `View all ${String(people.length)} credits`}
          </Button>
        </Card.Footer>
      )}
    </Card>
  );
}

function SeasonBrowser({ seasons }: { seasons: MediaItem[] }) {
  const orderedSeasons = [...seasons].sort(sortByIndex);
  const [selected, setSelected] = useState(orderedSeasons[0]?.Id ?? '');
  const selectedSeason = orderedSeasons.find((season) => season.Id === selected) ?? orderedSeasons[0];
  const [episodes, setEpisodes] = useState<MediaItem[]>();
  useEffect(() => {
    if (!selectedSeason) return;
    void getChildren(selectedSeason.Id).then((nextEpisodes) => {
      setEpisodes(nextEpisodes.filter((item) => item.Type === 'Episode').sort(sortByIndex));
    });
  }, [selectedSeason]);
  return (
    <Card>
      <Card.Header>
        <Card.Title>Seasons</Card.Title>
        <Card.Description>Browse the episodes in each season.</Card.Description>
      </Card.Header>
      <Card.Content>
        <div className="space-y-5">
          <div className="overflow-x-auto pb-1">
            <ToggleButtonGroup
              aria-label="Seasons"
              className="min-w-max"
              disallowEmptySelection
              onSelectionChange={(keys) => {
                const next = [...keys][0];
                if (next !== undefined) setSelected(next.toString());
              }}
              selectedKeys={new Set(selectedSeason ? [selectedSeason.Id] : [])}
              selectionMode="single"
              size="md"
            >
              {orderedSeasons.map((season) => (
                <ToggleButton className="min-w-32" id={season.Id} key={season.Id}>
                  {season.Name}
                </ToggleButton>
              ))}
            </ToggleButtonGroup>
          </div>
          {selectedSeason && (
            <div aria-live="polite" className="space-y-5">
              <SeasonSummary season={selectedSeason} />
              {episodes
                ? episodes.length
                  ? <EpisodeRail episodes={episodes} label={`Episodes in ${selectedSeason.Name}`} />
                  : <p className="py-6 text-sm text-muted">No episodes are available for this season.</p>
                : <EpisodeListSkeleton />}
            </div>
          )}
        </div>
      </Card.Content>
    </Card>
  );
}

function SeasonSummary({ season }: { season: MediaItem }) {
  return (
    <div className="flex flex-wrap items-center gap-3 text-sm text-muted">
      <Chip size="sm" variant="secondary">{season.Name}</Chip>
      {season.ProductionYear && <span>{season.ProductionYear}</span>}
      {season.Overview && <span className="basis-full max-w-3xl leading-6">{season.Overview}</span>}
    </div>
  );
}

function EpisodeRail({ episodes, label }: { episodes: MediaItem[]; label: string }) {
  return (
    <ul
      aria-label={label}
      className="-mx-2 flex snap-x snap-mandatory gap-4 overflow-x-auto px-2 pb-3 scrollbar-thin"
    >
      {[...episodes].sort(sortByIndex).map((episode) => (
        <li className="w-[17rem] shrink-0 snap-start sm:w-[20rem]" key={episode.Id}>
          <Link
            className="group block h-full overflow-hidden rounded-lg border border-border bg-surface transition-colors hover:border-accent"
            to={`/app/items/${episode.Id}`}
          >
            <div className="aspect-video overflow-hidden bg-default">
              <MediaImage
                alt={`Still for episode ${String(episode.IndexNumber ?? 'unknown')}: ${episode.Name}`}
                className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.02]"
                itemId={episode.Id}
                tag={episode.ImageTags?.Primary}
              />
            </div>
            <div className="space-y-2 p-4">
              <div className="flex items-center gap-2 text-xs text-muted">
                <span className="font-semibold text-accent">E{episode.IndexNumber ?? '—'}</span>
                {episode.ProductionYear && <span>{episode.ProductionYear}</span>}
                {episode.RunTimeTicks && <span>{formatRuntime(episode.RunTimeTicks)}</span>}
              </div>
              <p className="truncate text-sm font-semibold text-foreground">{episode.Name}</p>
              <p className="line-clamp-2 text-xs leading-5 text-muted">{episode.Overview ?? 'No episode synopsis.'}</p>
            </div>
          </Link>
        </li>
      ))}
    </ul>
  );
}

function EpisodeListSkeleton() {
  return (
    <div className="flex gap-4 overflow-hidden">
      {Array.from({ length: 3 }, (_, index) => (
        <Skeleton className="aspect-video w-[17rem] shrink-0 rounded-lg sm:w-[20rem]" key={index} />
      ))}
    </div>
  );
}

function ItemSkeleton() {
  return (
    <div className="space-y-8">
      <Skeleton className="h-5 w-28 rounded" />
      <div className="grid gap-8 lg:grid-cols-[15rem_minmax(0,1fr)]">
        <Skeleton className="aspect-[2/3] w-full max-w-[15rem] rounded-2xl" />
        <div className="space-y-4">
          <Skeleton className="h-6 w-24 rounded" />
          <Skeleton className="h-12 w-3/4 rounded" />
          <Skeleton className="h-20 w-full rounded" />
        </div>
      </div>
    </div>
  );
}

function NotFound() {
  return <p className="text-muted">This title could not be found.</p>;
}

async function loadItemBreadcrumbContext(
  itemId: string,
  itemParentId?: string,
  itemType?: string,
): Promise<ItemBreadcrumbContext> {
  const libraries = await getLibraries();
  const libraryIds = new Set(libraries.map((library) => library.Id));
  const visited = new Set([itemId]);
  const ancestors: MediaItem[] = [];
  let parentId = itemParentId;

  while (parentId && !libraryIds.has(parentId) && ancestors.length < 8) {
    if (visited.has(parentId)) break;
    visited.add(parentId);
    const parent = await getItem(parentId);
    ancestors.push(parent);
    parentId = parent.ParentId;
  }

  const directLibrary = parentId ? libraries.find((library) => library.Id === parentId) : undefined;
  const matchingLibraries = libraries.filter((library) => library.CollectionType === collectionTypeFor(itemType));
  return {
    ancestors: ancestors.reverse(),
    library: directLibrary ?? (matchingLibraries.length === 1 ? matchingLibraries[0] : undefined),
  };
}

function collectionTypeFor(itemType?: string) {
  if (itemType === 'Movie') return 'movies';
  if (itemType === 'Series' || itemType === 'Season' || itemType === 'Episode') return 'tvshows';
  return undefined;
}

function sortByIndex(a: MediaItem, b: MediaItem) {
  return (a.IndexNumber ?? Number.MAX_SAFE_INTEGER) - (b.IndexNumber ?? Number.MAX_SAFE_INTEGER) || a.Name.localeCompare(b.Name);
}

function useCreditPreviewCount() {
  const getCount = () => {
    if (window.matchMedia('(min-width: 1024px)').matches) return 6;
    if (window.matchMedia('(min-width: 640px)').matches) return 4;
    return 2;
  };
  const [count, setCount] = useState(getCount);

  useEffect(() => {
    const desktop = window.matchMedia('(min-width: 1024px)');
    const tablet = window.matchMedia('(min-width: 640px)');
    const update = () => {
      setCount(desktop.matches ? 6 : tablet.matches ? 4 : 2);
    };
    desktop.addEventListener('change', update);
    tablet.addEventListener('change', update);
    return () => {
      desktop.removeEventListener('change', update);
      tablet.removeEventListener('change', update);
    };
  }, []);

  return count;
}

function formatRuntime(ticks: number) {
  const minutes = Math.round(ticks / 10_000_000 / 60);
  const hours = Math.floor(minutes / 60);
  const remainder = minutes % 60;
  return hours ? `${String(hours)}h ${String(remainder)}m` : `${String(remainder)}m`;
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(date);
}

function initials(name: string) {
  return name.split(/\s+/).map((part) => part[0]).filter(Boolean).slice(0, 2).join('').toUpperCase();
}
