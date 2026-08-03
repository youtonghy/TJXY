import { Card, Chip, Table } from '@heroui/react';
import { Segment } from '@heroui-pro/react/segment';
import { Star } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { useTranslate } from '../../settings/i18n';
import { MediaImage } from '../ui/MediaImage';
import {
  getServerRanking,
  getTmdbRanking,
  type ServerRankingItem,
  type TmdbRankingItem,
} from '../api/portalApi';

type RankingTab = 'movies' | 'series' | 'server';
type LoadStatus = 'loading' | 'ready' | 'error';

interface RankingState<T> {
  items: T[];
  status: LoadStatus;
}

const emptyTmdb: RankingState<TmdbRankingItem> = { items: [], status: 'loading' };
const emptyServer: RankingState<ServerRankingItem> = { items: [], status: 'loading' };

export function RankingsPage() {
  const tr = useTranslate();
  const [selected, setSelected] = useState<RankingTab>('movies');
  const [movies, setMovies] = useState<RankingState<TmdbRankingItem>>(emptyTmdb);
  const [series, setSeries] = useState<RankingState<TmdbRankingItem>>(emptyTmdb);
  const [server, setServer] = useState<RankingState<ServerRankingItem>>(emptyServer);
  useEffect(() => {
    void getTmdbRanking('Movie')
      .then((items) => { setMovies({ items, status: 'ready' }); })
      .catch(() => { setMovies({ items: [], status: 'error' }); });
    void getTmdbRanking('Series')
      .then((items) => { setSeries({ items, status: 'ready' }); })
      .catch(() => { setSeries({ items: [], status: 'error' }); });
    void getServerRanking()
      .then((items) => { setServer({ items, status: 'ready' }); })
      .catch(() => { setServer({ items: [], status: 'error' }); });
  }, []);
  return <div className="space-y-6"><div><p className="text-sm font-medium text-accent">{tr('What people are watching', '大家都在看')}</p><h1 className="mt-1 text-3xl font-semibold">{tr('Rankings', '排行榜')}</h1><p className="mt-1 text-muted">{tr('Top-rated TMDB movies, popular series, and yesterday’s activity on this server.', '查看 TMDB 高分电影、热门剧集与本站昨日播放排行。')}</p></div><Segment aria-label={tr('Ranking source', '排行来源')} selectedKey={selected} onSelectionChange={(key) => { setSelected(key.toString() as RankingTab); }}><Segment.Item id="movies">{tr('TMDB top-rated movies', 'TMDB 高分电影')}</Segment.Item><Segment.Item id="series">{tr('TMDB series', 'TMDB 剧集')}</Segment.Item><Segment.Item id="server">{tr('Yesterday on TJXY', 'TJXY 昨日排行')}</Segment.Item></Segment>{selected === 'movies' ? <TmdbPanel label={tr('TMDB top-rated movie rankings', 'TMDB 高分电影排行')} state={movies} /> : selected === 'series' ? <TmdbPanel label={tr('TMDB series rankings', 'TMDB 剧集排行')} state={series} /> : <ServerPanel state={server} />}</div>;
}

function TmdbPanel({ label, state }: { label: string; state: RankingState<TmdbRankingItem> }) {
  const tr = useTranslate();
  if (state.status === 'loading') return <StatusCard message={tr('Loading TMDB ranking…', '正在加载 TMDB 排行…')} />;
  if (state.status === 'error') return <StatusCard message={tr('TMDB ranking is unavailable. Check the TMDB setting and network connection.', 'TMDB 排行暂不可用，请检查 TMDB 设置与网络连接。')} />;
  if (!state.items.length) return <StatusCard message={tr('TMDB did not return any titles.', 'TMDB 暂未返回任何影片。')} />;
  return <TmdbGrid items={state.items} label={label} />;
}

function ServerPanel({ state }: { state: RankingState<ServerRankingItem> }) {
  const tr = useTranslate();
  if (state.status === 'loading') return <StatusCard message={tr('Loading yesterday’s ranking…', '正在加载昨日排行…')} />;
  if (state.status === 'error') return <StatusCard message={tr('Yesterday’s server ranking is unavailable.', '昨日本站排行暂不可用。')} />;
  if (!state.items.length) return <StatusCard message={tr('No playback activity was recorded yesterday.', '昨日没有播放记录。')} />;
  return <ServerGrid items={state.items} />;
}

function StatusCard({ message }: { message: string }) {
  return <Card><Card.Content className="py-10 text-center text-sm text-muted">{message}</Card.Content></Card>;
}

function TmdbGrid({ items, label }: { items: TmdbRankingItem[]; label: string }) {
  const tr = useTranslate();
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label={label} className="min-w-[52rem] table-fixed">
          <Table.Header>
            <Table.Column className="w-20">{tr('Rank', '排名')}</Table.Column>
            <Table.Column isRowHeader>{tr('Title', '片名')}</Table.Column>
            <Table.Column className="w-28">{tr('Year', '年份')}</Table.Column>
            <Table.Column className="w-28">{tr('Rating', '评分')}</Table.Column>
          </Table.Header>
          <Table.Body>
            {items.map((item) => (
              <Table.Row id={String(item.TmdbId)} key={item.TmdbId}>
                <Table.Cell><span className="text-lg font-semibold text-accent">#{String(item.Rank)}</span></Table.Cell>
                <Table.Cell>
                  <div className="flex min-w-0 items-center gap-4 py-2">
                    {item.PosterUrl ? <img alt={tr(`Poster for ${item.Name}`, `${item.Name} 的海报`)} className="h-20 w-14 shrink-0 rounded-md object-cover" src={item.PosterUrl} /> : <div className="h-20 w-14 shrink-0 rounded-md bg-default" />}
                    <div className="min-w-0">
                      <p className="font-medium text-foreground">{item.LocalItemId ? <Link className="hover:underline" to={`/app/items/${item.LocalItemId}`}>{item.Name}</Link> : item.Name}</p>
                      <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted">{item.Overview ?? tr('No overview available.', '暂无简介。')}</p>
                    </div>
                  </div>
                </Table.Cell>
                <Table.Cell>{item.ProductionYear ?? '—'}</Table.Cell>
                <Table.Cell>{item.Rating === undefined ? '—' : <span className="inline-flex items-center gap-1"><Star aria-hidden="true" className="size-3 text-accent" />{item.Rating.toFixed(1)}</span>}</Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}

function ServerGrid({ items }: { items: ServerRankingItem[] }) {
  const tr = useTranslate();
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label={tr('Yesterday on TJXY rankings', 'TJXY 昨日排行')} className="min-w-[52rem] table-fixed">
          <Table.Header>
            <Table.Column className="w-20">{tr('Rank', '排名')}</Table.Column>
            <Table.Column isRowHeader>{tr('Title', '片名')}</Table.Column>
            <Table.Column className="w-28">{tr('Type', '类型')}</Table.Column>
            <Table.Column className="w-28">{tr('Plays', '播放次数')}</Table.Column>
            <Table.Column className="w-28">{tr('Viewers', '观看人数')}</Table.Column>
          </Table.Header>
          <Table.Body>
            {items.map((item) => (
              <Table.Row id={item.Id} key={item.Id}>
                <Table.Cell><span className="text-lg font-semibold text-accent">#{String(item.Rank)}</span></Table.Cell>
                <Table.Cell>
                  <div className="flex min-w-0 items-center gap-4 py-2">
                    <MediaImage alt={tr(`Poster for ${item.Name}`, `${item.Name} 的海报`)} className="h-20 w-14 shrink-0 rounded-md object-cover" itemId={item.Id} tag={item.PrimaryImageTag} />
                    <div className="min-w-0">
                      <Link className="font-medium text-foreground hover:underline" to={`/app/items/${item.Id}`}>{item.Name}</Link>
                      <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted">{item.Overview ?? tr('No overview available.', '暂无简介。')}</p>
                      <span className="mt-1 block text-xs text-muted">{item.ProductionYear ?? '—'}</span>
                    </div>
                  </div>
                </Table.Cell>
                <Table.Cell><Chip size="sm" variant="soft">{tr(item.ItemType, item.ItemType === 'Movie' ? '电影' : item.ItemType === 'Series' ? '剧集' : item.ItemType === 'Episode' ? '单集' : item.ItemType)}</Chip></Table.Cell>
                <Table.Cell><span className="font-semibold tabular-nums">{item.PlayCount}</span></Table.Cell>
                <Table.Cell><span className="tabular-nums text-muted">{item.UniqueViewers}</span></Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}
