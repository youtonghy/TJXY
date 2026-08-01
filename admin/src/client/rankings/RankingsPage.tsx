import { Card, Chip, Table, Tabs } from '@heroui/react';
import { Star } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
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
  return <div className="space-y-6"><div><p className="text-sm font-medium text-accent">What people are watching</p><h1 className="mt-1 text-3xl font-semibold">Rankings</h1><p className="mt-1 text-muted">Daily TMDB popularity and yesterday’s activity on this server.</p></div><Tabs selectedKey={selected} onSelectionChange={(key) => { setSelected(key.toString() as RankingTab); }} variant="secondary"><Tabs.ListContainer><Tabs.List aria-label="Ranking source"><Tabs.Tab id="movies">TMDB movies<Tabs.Indicator /></Tabs.Tab><Tabs.Tab id="series">TMDB series<Tabs.Indicator /></Tabs.Tab><Tabs.Tab id="server">Yesterday on TJXY<Tabs.Indicator /></Tabs.Tab></Tabs.List></Tabs.ListContainer><Tabs.Panel className="pt-5" id={selected}>{selected === 'movies' ? <TmdbPanel label="TMDB movie rankings" state={movies} /> : selected === 'series' ? <TmdbPanel label="TMDB series rankings" state={series} /> : <ServerPanel state={server} />}</Tabs.Panel></Tabs></div>;
}

function TmdbPanel({ label, state }: { label: string; state: RankingState<TmdbRankingItem> }) {
  if (state.status === 'loading') return <StatusCard message="Loading TMDB ranking…" />;
  if (state.status === 'error') return <StatusCard message="TMDB ranking is unavailable. Check the TMDB setting and network connection." />;
  if (!state.items.length) return <StatusCard message="TMDB did not return any titles." />;
  return <TmdbGrid items={state.items} label={label} />;
}

function ServerPanel({ state }: { state: RankingState<ServerRankingItem> }) {
  if (state.status === 'loading') return <StatusCard message="Loading yesterday’s ranking…" />;
  if (state.status === 'error') return <StatusCard message="Yesterday’s server ranking is unavailable." />;
  if (!state.items.length) return <StatusCard message="No playback activity was recorded yesterday." />;
  return <ServerGrid items={state.items} />;
}

function StatusCard({ message }: { message: string }) {
  return <Card><Card.Content className="py-10 text-center text-sm text-muted">{message}</Card.Content></Card>;
}

function TmdbGrid({ items, label }: { items: TmdbRankingItem[]; label: string }) {
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label={label} className="min-w-[52rem] table-fixed">
          <Table.Header>
            <Table.Column className="w-20">Rank</Table.Column>
            <Table.Column isRowHeader>Title</Table.Column>
            <Table.Column className="w-28">Year</Table.Column>
            <Table.Column className="w-28">Rating</Table.Column>
          </Table.Header>
          <Table.Body>
            {items.map((item) => (
              <Table.Row id={String(item.TmdbId)} key={item.TmdbId}>
                <Table.Cell><span className="text-lg font-semibold text-accent">#{String(item.Rank)}</span></Table.Cell>
                <Table.Cell>
                  <div className="flex min-w-0 items-center gap-4 py-2">
                    {item.PosterUrl ? <img alt={`Poster for ${item.Name}`} className="h-20 w-14 shrink-0 rounded-md object-cover" src={item.PosterUrl} /> : <div className="h-20 w-14 shrink-0 rounded-md bg-default" />}
                    <div className="min-w-0">
                      <p className="font-medium text-foreground">{item.LocalItemId ? <Link className="hover:underline" to={`/app/items/${item.LocalItemId}`}>{item.Name}</Link> : item.Name}</p>
                      <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted">{item.Overview ?? 'No overview available.'}</p>
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
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label="Yesterday on TJXY rankings" className="min-w-[42rem] table-fixed">
          <Table.Header>
            <Table.Column className="w-20">Rank</Table.Column>
            <Table.Column isRowHeader>Title</Table.Column>
            <Table.Column className="w-28">Type</Table.Column>
            <Table.Column className="w-28">Plays</Table.Column>
            <Table.Column className="w-28">Viewers</Table.Column>
          </Table.Header>
          <Table.Body>
            {items.map((item) => (
              <Table.Row id={item.Id} key={item.Id}>
                <Table.Cell><span className="text-lg font-semibold text-accent">#{String(item.Rank)}</span></Table.Cell>
                <Table.Cell><Link className="font-medium text-foreground hover:underline" to={`/app/items/${item.Id}`}>{item.Name}</Link><span className="mt-1 block text-xs text-muted">{item.ProductionYear ?? '—'}</span></Table.Cell>
                <Table.Cell><Chip size="sm" variant="soft">{item.ItemType}</Chip></Table.Cell>
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
