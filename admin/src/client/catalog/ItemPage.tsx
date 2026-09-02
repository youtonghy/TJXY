/* eslint-disable @typescript-eslint/no-misused-promises */
import { Alert, Avatar, Breadcrumbs, Button, Card, Chip, Skeleton, ToggleButton, ToggleButtonGroup } from '@heroui/react';
import { Rating } from '@heroui-pro/react/rating';
import { Carousel } from '@heroui-pro/react/carousel';
import { CalendarDays, Check, ChevronDown, ChevronUp, Clock3, Heart, Info, LoaderCircle, RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link, useNavigate, useParams, useSearchParams } from 'react-router-dom';
import { useSystemLocale } from '../../settings/SystemLocaleProvider';
import { useTranslate } from '../../settings/i18n';
import { listRecentTaskJobs } from '../../tasks/taskApi';
import { useClientAuth } from '../auth/ClientAuthContext';
import { getChildren, getItem, getLibraries, getSimilarItems, toggleFavorite, togglePlayed, type Library, type MediaItem } from '../api/catalogApi';
import { ExternalPlayButton } from '../playback/ExternalPlayButton';
import { MediaImage } from '../ui/MediaImage';
import { MediaTile } from '../ui/MediaTile';

interface ItemBreadcrumbContext {
  ancestors: MediaItem[];
  itemId?: string;
  library?: Library;
}

interface RecommendationResult {
  failed: boolean;
  itemId: string;
  items?: MediaItem[];
}

const METADATA_REFRESH_DELAYS_MS = [2_500, 5_000, 10_000] as const;
type MetadataRefreshState = 'idle' | 'no-match' | 'exhausted';

export function ItemPage() {
  const { id } = useParams();
  const [searchParams] = useSearchParams();
  const libraryId = searchParams.get('libraryId') ?? undefined;
  const navigate = useNavigate();
  const { user } = useClientAuth();
  const isAdministrator = user?.Policy?.IsAdministrator === true;
  const tr = useTranslate();
  const [item, setItem] = useState<MediaItem>();
  const [children, setChildren] = useState<MediaItem[]>([]);
  const [recommendationResult, setRecommendationResult] = useState<RecommendationResult>();
  const [breadcrumbContext, setBreadcrumbContext] = useState<ItemBreadcrumbContext>({ ancestors: [] });
  const [failed, setFailed] = useState(false);
  const [metadataRefreshKey, setMetadataRefreshKey] = useState(0);
  const [metadataRefreshState, setMetadataRefreshState] = useState<MetadataRefreshState>('idle');
  const breadcrumbItemId = item?.Id;
  const breadcrumbParentId = item?.ParentId;
  const breadcrumbItemType = item?.Type;
  const recommendationItemId = item?.Type === 'Movie' || item?.Type === 'Series' ? item.Id : undefined;
  const recommendationItemType = item?.Type === 'Movie' || item?.Type === 'Series' ? item.Type : undefined;
  const isMetadataPartial = item?.MetadataState === 'Partial'
    && (item.Type === 'Movie' || item.Type === 'Series');

  useEffect(() => {
    if (!id) return;
    void Promise.resolve()
      .then(() => {
        setFailed(false);
        setItem(undefined);
        setChildren([]);
        setMetadataRefreshState('idle');
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
    if (
      !id
      || item?.Id !== id
      || item.MetadataState !== 'Partial'
      || (item.Type !== 'Movie' && item.Type !== 'Series')
    ) return;
    const controller = new AbortController();
    void (async () => {
      for (const delay of METADATA_REFRESH_DELAYS_MS) {
        await new Promise((resolve) => { setTimeout(resolve, delay); });
        if (controller.signal.aborted) return;
        try {
          const refreshed = await getItem(id);
          controller.signal.throwIfAborted();
          setItem(refreshed);
          if (refreshed.MetadataState !== 'Partial') {
            setMetadataRefreshState('idle');
            return;
          }
          if (isAdministrator) {
            const jobs = await listRecentTaskJobs(controller.signal);
            controller.signal.throwIfAborted();
            const latestMetadataJob = jobs.find((job) => job.taskKind === 'ResolveMetadata'
              && job.scopeType === 'CatalogItem'
              && job.scopeId === id);
            if (latestMetadataJob?.outcome === 'NoMetadataMatch') {
              setMetadataRefreshState('no-match');
              return;
            }
          }
        } catch {
          // A later bounded attempt may recover from a transient provider failure.
        }
      }
      if (!controller.signal.aborted) setMetadataRefreshState('exhausted');
    })();
    return () => {
      controller.abort();
    };
  }, [id, isAdministrator, item?.Id, item?.MetadataState, item?.Type, metadataRefreshKey]);

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

  useEffect(() => {
    if (!recommendationItemId || !recommendationItemType) return;
    let active = true;
    void getSimilarItems(recommendationItemId, 4)
      .then((items) => {
        if (!active) return;
        setRecommendationResult({
          failed: false,
          itemId: recommendationItemId,
          items: items.filter((candidate) => candidate.Id !== recommendationItemId && candidate.Type === recommendationItemType && candidate.UserData?.Played !== true).slice(0, 4),
        });
      })
      .catch(() => {
        if (active) setRecommendationResult({ failed: true, itemId: recommendationItemId });
      });
    return () => {
      active = false;
    };
  }, [recommendationItemId, recommendationItemType]);

  if (!id) return <NotFound />;
  if (failed) {
    return (
      <Alert status="danger" role="alert">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Title>{tr('Unable to load this title', '无法加载此影片')}</Alert.Title>
          <Alert.Description>{tr('Try again from the library.', '请返回媒体库后重试。')}</Alert.Description>
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
  const artworkRatio = item.Type === 'Audio' ? 'aspect-square' : 'aspect-[2/3]';
  const activeRecommendationResult = recommendationResult?.itemId === item.Id ? recommendationResult : undefined;

  return (
    <article className="space-y-8">
      <ItemBreadcrumb context={breadcrumbContext} item={item} onNavigate={navigate} />

      {isAdministrator && isMetadataPartial && metadataRefreshState === 'idle' && (
        <Alert status="accent" role="status">
          <Alert.Indicator><LoaderCircle className="size-4 animate-spin" /></Alert.Indicator>
          <Alert.Content>
            <Alert.Title>{tr('Metadata scan in progress', '正在扫描元数据')}</Alert.Title>
            <Alert.Description>{tr('This title has not been scanned yet. Scanning has started and the page will update automatically.', '暂未扫描本影片的元数据，扫描已开始，完成后页面会自动更新。')}</Alert.Description>
          </Alert.Content>
        </Alert>
      )}

      {isAdministrator && metadataRefreshState === 'no-match' && (
        <Alert status="warning" role="alert">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>{tr('No metadata match', '未匹配到元数据')}</Alert.Title>
            <Alert.Description>
              {tr('No provider result matched this title. Check the title and year, then retry.', '元数据提供方没有找到匹配结果，请检查影片名称和年份后重试。')}
            </Alert.Description>
          </Alert.Content>
          <MetadataRetryButton onRetry={() => {
            setMetadataRefreshState('idle');
            setMetadataRefreshKey((value) => value + 1);
          }} tr={tr} />
        </Alert>
      )}

      {isAdministrator && metadataRefreshState === 'exhausted' && (
        <Alert status="warning" role="alert">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>{tr('Metadata is still unavailable', '元数据仍不可用')}</Alert.Title>
            <Alert.Description>
              {tr('No match was found or the metadata service could not be reached.', '可能没有匹配结果，或当前无法连接元数据服务。')}
            </Alert.Description>
          </Alert.Content>
          <MetadataRetryButton onRetry={() => {
            setMetadataRefreshState('idle');
            setMetadataRefreshKey((value) => value + 1);
          }} tr={tr} />
        </Alert>
      )}

      <div className="grid gap-8 lg:grid-cols-[15rem_minmax(0,1fr)]">
        <div className={`mx-auto ${artworkRatio} w-full max-w-[15rem] overflow-hidden rounded-2xl bg-default shadow-sm lg:mx-0`}>
          <MediaImage
            alt={tr(`Poster for ${item.Name}`, `${item.Name} 的海报`)}
            className="h-full w-full object-cover"
            itemId={id}
            libraryId={libraryId}
            tag={item.ImageTags?.Primary}
          />
        </div>

        <div className="min-w-0 self-center">
          <div className="flex flex-wrap items-center gap-2">
            <Chip color="accent" variant="soft">
              {translateItemType(item.Type, tr)}
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
              <ExternalPlayButton
                isExternalPlaybackDisabled={item.HasMediaSources === false}
                itemId={id}
                itemTitle={item.Name}
                key={id}
                onPlay={() => navigate(`/app/play/${id}${libraryId ? `?libraryId=${encodeURIComponent(libraryId)}` : ''}`)}
              />
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
              {favorite ? tr('Unfavorite', '取消收藏') : tr('Favorite', '收藏')}
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
              {played ? tr('Mark unplayed', '标记为未看') : tr('Mark played', '标记为已看')}
            </Button>
          </div>
        </div>
      </div>

      {hasPlayableAction && item.HasMediaSources === false && (
        <Alert status="warning">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>{tr('No video source available', '没有可用的视频源')}</Alert.Title>
            <Alert.Description>
              {tr('Add a media file to this title before starting playback.', '开始播放前，请先为此影片添加媒体文件。')}
            </Alert.Description>
          </Alert.Content>
        </Alert>
      )}

      {seasons.length ? <SeasonBrowser seasons={seasons} /> : null}
      {episodes.length ? <EpisodeRail episodes={episodes} label={tr('Episodes', '剧集')} /> : null}

      <div className="grid gap-4 md:grid-cols-2">
        <MetadataCard item={item} />
        <TaxonomyCard item={item} />
      </div>

      {item.People?.length ? <PeopleCard people={item.People} /> : null}
      {item.Type === 'Movie' || item.Type === 'Series'
        ? <RecommendationRail failed={activeRecommendationResult?.failed === true} items={activeRecommendationResult?.items} />
        : null}
    </article>
  );
}

function MetadataRetryButton({ onRetry, tr }: { onRetry: () => void; tr: ReturnType<typeof useTranslate> }) {
  return (
    <Button size="sm" variant="secondary" onPress={onRetry}>
      <RefreshCw className="size-4" />
      {tr('Retry', '重试')}
    </Button>
  );
}

function RecommendationRail({ failed, items }: { failed: boolean; items?: MediaItem[] }) {
  const tr = useTranslate();
  const label = tr('Recommended for you', '为你推荐');
  const carouselLabel = tr('Recommended titles', '推荐影片');
  return (
    <section aria-label={label} className="space-y-3">
      <h2 className="text-lg font-semibold text-foreground">{label}</h2>
      {failed
        ? <Alert status="warning"><Alert.Indicator /><Alert.Content><Alert.Title>{tr('Recommendations are temporarily unavailable', '推荐暂时不可用')}</Alert.Title></Alert.Content></Alert>
        : items === undefined
          ? <RecommendationSkeleton label={tr('Loading recommendations', '正在加载推荐')} />
          : items.length === 0
            ? <p className="py-8 text-sm text-muted">{tr('No recommendations yet', '暂无推荐')}</p>
            : (
                <Carousel
                  aria-label={carouselLabel}
                  className="relative min-w-0"
                  opts={{ align: 'start', containScroll: 'trimSnaps', dragFree: true }}
                >
                  <Carousel.Previous />
                  <Carousel.Next />
                  <Carousel.Content className="-ml-3 pb-3">
                    {items.map((item) => (
                      <Carousel.Item className="basis-[8.5rem] pl-3 sm:basis-[10.5rem] lg:basis-[14rem]" key={item.Id}>
                        <MediaTile item={item} />
                      </Carousel.Item>
                    ))}
                  </Carousel.Content>
                </Carousel>
              )}
    </section>
  );
}

function RecommendationSkeleton({ label }: { label: string }) {
  return (
    <div aria-label={label} className="flex gap-4 overflow-hidden" role="status">
      {Array.from({ length: 6 }, (_, index) => (
        <div className="w-[8.5rem] shrink-0 space-y-2 sm:w-[10.5rem] lg:w-[14rem]" key={index}>
          <Skeleton className="aspect-[2/3] w-full rounded-xl" />
          <Skeleton className="h-4 w-3/4 rounded" />
        </div>
      ))}
    </div>
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
  const tr = useTranslate();
  const entries = [
    { id: 'home', label: tr('Home', '首页'), to: '/app/' },
    { id: 'libraries', label: tr('Libraries', '媒体库'), to: '/app/libraries' },
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
    <nav aria-label={tr('Item breadcrumb', '影片路径')} className="max-w-full overflow-x-auto pb-1">
      <Breadcrumbs aria-label={tr('Item breadcrumb', '影片路径')} className="min-w-max flex-nowrap">
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
  const tr = useTranslate();
  const { locale } = useSystemLocale();
  const facts = [
    item.RunTimeTicks ? { icon: Clock3, label: tr('Runtime', '片长'), value: formatRuntime(item.RunTimeTicks, locale) } : undefined,
    item.PremiereDate ? { icon: CalendarDays, label: tr('Premiere', '首映'), value: formatDate(item.PremiereDate, locale) } : undefined,
    item.EndDate ? { icon: CalendarDays, label: tr('Ended', '完结'), value: formatDate(item.EndDate, locale) } : undefined,
    item.OriginalLanguage ? { icon: Info, label: tr('Original language', '原始语言'), value: item.OriginalLanguage.toUpperCase() } : undefined,
  ].filter((fact): fact is { icon: typeof Clock3; label: string; value: string } => Boolean(fact));
  const rating = item.CommunityRating;

  return (
    <Card>
      <Card.Header>
        <Card.Title>{tr('Details', '详细信息')}</Card.Title>
        <Card.Description>{tr('Key information from the catalog record.', '媒体目录记录中的主要信息。')}</Card.Description>
      </Card.Header>
      <Card.Content className="grid gap-4 sm:grid-cols-2">
        {rating !== undefined ? (
          <div className="min-w-0">
            <p className="text-xs text-muted">{tr('Rating', '评分')}</p>
            <div className="mt-1 flex flex-wrap items-center gap-2">
              <Rating
                isReadOnly
                aria-label={tr(`${rating.toFixed(1)} out of 10${item.VoteCount ? ` from ${item.VoteCount.toLocaleString(locale)} votes` : ''}`, `评分 ${rating.toFixed(1)}，满分 10 分${item.VoteCount ? `，共 ${item.VoteCount.toLocaleString(locale)} 票` : ''}`)}
                size="sm"
                value={rating / 2}
              >
                {[1, 2, 3, 4, 5].map((value) => <Rating.Item key={value} value={value} />)}
              </Rating>
              <span className="text-sm font-medium tabular-nums">{rating.toFixed(1)}</span>
              {item.VoteCount ? <span className="text-xs text-muted">{tr(`${item.VoteCount.toLocaleString(locale)} votes`, `${item.VoteCount.toLocaleString(locale)} 票`)}</span> : null}
            </div>
          </div>
        ) : null}
        {facts.map(({ icon: Icon, label, value }) => (
          <div className="flex items-start gap-3" key={label}>
            <Icon className="mt-0.5 size-4 shrink-0 text-accent" />
            <div className="min-w-0">
              <p className="text-xs text-muted">{label}</p>
              <p className="mt-1 break-words text-sm font-medium">{value}</p>
            </div>
          </div>
        ))}
        {rating === undefined && !facts.length && <p className="text-sm text-muted">{tr('No additional details are available.', '暂无更多详细信息。')}</p>}
      </Card.Content>
    </Card>
  );
}

function TaxonomyCard({ item }: { item: MediaItem }) {
  const tr = useTranslate();
  const values = [
    [tr('Genres', '类型'), item.Genres],
    [tr('Studios', '制作公司'), item.Studios],
    [tr('Countries', '国家与地区'), item.Countries?.map((value) => value.Name)],
    [tr('Languages', '语言'), item.Languages?.map((value) => value.Name)],
  ] as const;
  const visible = values.filter(([, entries]) => entries?.length);
  return (
    <Card>
      <Card.Header>
        <Card.Title>{tr('Classification', '分类信息')}</Card.Title>
        <Card.Description>{tr('Genres, production, and language metadata.', '类型、制作与语言元数据。')}</Card.Description>
      </Card.Header>
      <Card.Content className="space-y-4">
        {visible.length ? visible.map(([label, entries]) => (
          <div className="space-y-2" key={label}>
            <p className="text-xs text-muted">{label}</p>
            <div className="flex flex-wrap gap-2">
              {entries?.map((entry) => <Chip key={entry} size="sm" variant="secondary">{entry}</Chip>)}
            </div>
          </div>
        )) : <p className="text-sm text-muted">{tr('No classification metadata is available.', '暂无分类元数据。')}</p>}
      </Card.Content>
    </Card>
  );
}

function PeopleCard({ people }: { people: NonNullable<MediaItem['People']> }) {
  const tr = useTranslate();
  const previewCount = useCreditPreviewCount();
  const [expanded, setExpanded] = useState(false);
  const visiblePeople = expanded ? people : people.slice(0, previewCount);
  return (
    <Card>
      <Card.Header>
        <Card.Title>{tr('Cast and crew', '演职人员')}</Card.Title>
        <Card.Description>{tr('People credited on this title.', '参与此影片的演职人员。')}</Card.Description>
      </Card.Header>
      <Card.Content className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {visiblePeople.map((person) => (
          <div className="flex min-w-0 items-center gap-3" key={`${person.Id}-${person.Role ?? ''}`}>
            <Avatar color={person.Type === 'Crew' ? 'accent' : 'default'} size="sm">
              <Avatar.Fallback>{initials(person.Name)}</Avatar.Fallback>
            </Avatar>
            <div className="min-w-0">
              <p className="truncate text-sm font-medium">{person.Name}</p>
              <p className="truncate text-xs text-muted">{[person.Role, translatePersonType(person.Type, tr)].filter(Boolean).join(' · ')}</p>
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
            {expanded ? tr('Show fewer credits', '收起演职人员') : tr(`View all ${String(people.length)} credits`, `查看全部 ${String(people.length)} 位演职人员`)}
          </Button>
        </Card.Footer>
      )}
    </Card>
  );
}

function SeasonBrowser({ seasons }: { seasons: MediaItem[] }) {
  const tr = useTranslate();
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
        <Card.Title>{tr('Seasons', '季')}</Card.Title>
        <Card.Description>{tr('Browse the episodes in each season.', '浏览每一季的剧集。')}</Card.Description>
      </Card.Header>
      <Card.Content>
        <div className="space-y-5">
          <div className="overflow-x-auto pb-1">
            <ToggleButtonGroup
              aria-label={tr('Seasons', '季')}
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
                  ? <EpisodeRail episodes={episodes} label={tr(`Episodes in ${selectedSeason.Name}`, `${selectedSeason.Name} 中的剧集`)} />
                  : <p className="py-6 text-sm text-muted">{tr('No episodes are available for this season.', '本季暂无可用剧集。')}</p>
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
  const tr = useTranslate();
  const { locale } = useSystemLocale();
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
                alt={tr(`Still for episode ${String(episode.IndexNumber ?? 'unknown')}: ${episode.Name}`, `第 ${String(episode.IndexNumber ?? '未知')} 集 ${episode.Name} 的剧照`)}
                className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.02]"
                itemId={episode.Id}
                tag={episode.ImageTags?.Primary}
              />
            </div>
            <div className="space-y-2 p-4">
              <div className="flex items-center gap-2 text-xs text-muted">
                <span className="font-semibold text-accent">E{episode.IndexNumber ?? '—'}</span>
                {episode.ProductionYear && <span>{episode.ProductionYear}</span>}
                {episode.RunTimeTicks && <span>{formatRuntime(episode.RunTimeTicks, locale)}</span>}
              </div>
              <p className="truncate text-sm font-semibold text-foreground">{episode.Name}</p>
              <p className="line-clamp-2 text-xs leading-5 text-muted">{episode.Overview ?? tr('No episode synopsis.', '暂无单集简介。')}</p>
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
  const tr = useTranslate();
  return <p className="text-muted">{tr('This title could not be found.', '找不到此影片。')}</p>;
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

function formatRuntime(ticks: number, locale = 'en-US') {
  const minutes = Math.round(ticks / 10_000_000 / 60);
  const hours = Math.floor(minutes / 60);
  const remainder = minutes % 60;
  if (locale === 'zh-CN') return hours ? `${String(hours)} 小时 ${String(remainder)} 分钟` : `${String(remainder)} 分钟`;
  return hours ? `${String(hours)}h ${String(remainder)}m` : `${String(remainder)}m`;
}

function formatDate(value: string, locale = 'en-US') {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }).format(date);
}

function translateItemType(type: string | undefined, tr: (english: string, chinese: string) => string) {
  if (type === 'Movie') return tr('Movie', '电影');
  if (type === 'Series') return tr('Series', '剧集');
  if (type === 'Season') return tr('Season', '季');
  if (type === 'Episode') return tr('Episode', '单集');
  return type ?? tr('Media', '媒体');
}

function translatePersonType(type: string | undefined, tr: (english: string, chinese: string) => string) {
  if (type === 'Crew') return tr('Crew', '幕后');
  if (type === 'Actor') return tr('Actor', '演员');
  return type;
}

function initials(name: string) {
  return name.split(/\s+/).map((part) => part[0]).filter(Boolean).slice(0, 2).join('').toUpperCase();
}
