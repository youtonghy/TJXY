/* eslint-disable @typescript-eslint/no-misused-promises */
import { Alert, Avatar, Button, Card, Chip, Skeleton, Tabs } from '@heroui/react';
import { ArrowLeft, CalendarDays, Check, Clock3, Heart, Info, Play, Star } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { useClientAuth } from '../auth/ClientAuthContext';
import { getChildren, getItem, toggleFavorite, togglePlayed, type MediaItem } from '../api/catalogApi';
import { MediaImage } from '../ui/MediaImage';

export function ItemPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const { user } = useClientAuth();
  const [item, setItem] = useState<MediaItem>();
  const [children, setChildren] = useState<MediaItem[]>([]);
  const [failed, setFailed] = useState(false);

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
      <Link className="inline-flex items-center gap-2 text-sm text-muted hover:text-foreground" to="/app/">
        <ArrowLeft className="size-4" />
        Back to home
      </Link>

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
            <Alert.Title>Demo metadata only</Alert.Title>
            <Alert.Description>
              This title includes real catalog information, but no video file has been added to the development library.
            </Alert.Description>
          </Alert.Content>
        </Alert>
      )}

      <div className="grid gap-4 md:grid-cols-2">
        <MetadataCard item={item} />
        <TaxonomyCard item={item} />
      </div>

      {item.People?.length ? <PeopleCard people={item.People} /> : null}
      {seasons.length ? <SeasonBrowser seasons={seasons} /> : null}
      {episodes.length ? <EpisodeList episodes={episodes} /> : null}
    </article>
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
  return (
    <Card>
      <Card.Header>
        <Card.Title>Cast and crew</Card.Title>
        <Card.Description>People credited on this title.</Card.Description>
      </Card.Header>
      <Card.Content className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {people.map((person) => (
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
        <Tabs selectedKey={selectedSeason?.Id} onSelectionChange={(key) => { setSelected(key.toString()); }} variant="secondary">
          <Tabs.ListContainer>
            <Tabs.List aria-label="Seasons">
              {orderedSeasons.map((season) => (
                <Tabs.Tab id={season.Id} key={season.Id}>
                  {season.Name}
                  <Tabs.Indicator />
                </Tabs.Tab>
              ))}
            </Tabs.List>
          </Tabs.ListContainer>
          {selectedSeason && (
            <Tabs.Panel className="pt-5" id={selectedSeason.Id}>
              <div className="space-y-5">
                <SeasonSummary season={selectedSeason} />
                {episodes
                  ? episodes.length
                    ? <EpisodeList episodes={episodes} compact />
                    : <p className="py-6 text-sm text-muted">No episodes are available for this season.</p>
                  : <EpisodeListSkeleton />}
              </div>
            </Tabs.Panel>
          )}
        </Tabs>
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

function EpisodeList({ episodes, compact = false }: { episodes: MediaItem[]; compact?: boolean }) {
  const content = (
    <div className="divide-y divide-border">
      {[...episodes].sort(sortByIndex).map((episode) => (
        <Link className="flex items-center gap-4 px-5 py-4 transition-colors hover:bg-default" key={episode.Id} to={`/app/items/${episode.Id}`}>
          <div className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-accent/10 text-sm font-semibold text-accent">
            {episode.IndexNumber ?? '—'}
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium">{episode.Name}</p>
            <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted">{episode.Overview ?? 'No episode synopsis.'}</p>
          </div>
          {episode.RunTimeTicks && <span className="shrink-0 text-xs text-muted">{formatRuntime(episode.RunTimeTicks)}</span>}
        </Link>
      ))}
    </div>
  );
  if (compact) return content;
  return (
    <Card>
      <Card.Header>
        <Card.Title>Episodes</Card.Title>
        <Card.Description>{episodes.length} episodes in this collection.</Card.Description>
      </Card.Header>
      <Card.Content className="p-0">{content}</Card.Content>
    </Card>
  );
}

function EpisodeListSkeleton() {
  return (
    <div className="space-y-3">
      {Array.from({ length: 3 }, (_, index) => <Skeleton className="h-16 rounded-lg" key={index} />)}
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

function sortByIndex(a: MediaItem, b: MediaItem) {
  return (a.IndexNumber ?? Number.MAX_SAFE_INTEGER) - (b.IndexNumber ?? Number.MAX_SAFE_INTEGER) || a.Name.localeCompare(b.Name);
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
