import { Card, Chip, Pagination, ProgressBar, Table } from '@heroui/react';
import { Segment } from '@heroui-pro/react/segment';

import type {
  DashboardPage,
  LoginHistoryItem,
  NowPlayingItem,
  WatchHistoryItem,
} from './dashboardApi';

export type HistoryTab = 'logins' | 'watching';
export type HistoryResult =
  | { kind: 'logins'; page: DashboardPage<LoginHistoryItem> }
  | { kind: 'watching'; page: DashboardPage<WatchHistoryItem> };

export function NowPlayingTable({ items }: { items: NowPlayingItem[] }) {
  return (
    <Card>
      <Card.Header className="flex-row items-start justify-between gap-4">
        <div>
          <Card.Title>Watching now</Card.Title>
          <Card.Description>Playback sessions active within the last minute.</Card.Description>
        </div>
        <Chip color={items.length > 0 ? 'success' : 'default'} size="sm" variant="soft">
          {items.length} active
        </Chip>
      </Card.Header>
      <Card.Content>
        {items.length === 0 ? <EmptyTable message="Nobody is watching right now." /> : (
          <Table variant="secondary">
            <Table.ScrollContainer>
              <Table.Content aria-label="Currently watching users" className="min-w-[48rem] table-fixed">
                <Table.Header>
                  <Table.Column isRowHeader>User</Table.Column>
                  <Table.Column>Title</Table.Column>
                  <Table.Column>Progress</Table.Column>
                  <Table.Column>Device</Table.Column>
                  <Table.Column>Last activity</Table.Column>
                </Table.Header>
                <Table.Body>
                  {items.map((item) => (
                    <Table.Row id={item.sessionId} key={item.sessionId}>
                      <Table.Cell><span className="font-medium text-foreground">{item.userName}</span></Table.Cell>
                      <Table.Cell><TitleCell name={item.itemName} type={item.itemType} /></Table.Cell>
                      <Table.Cell><PlaybackProgress item={item} /></Table.Cell>
                      <Table.Cell><span className="block truncate">{item.clientName} · {item.deviceName}</span></Table.Cell>
                      <Table.Cell>{formatDate(item.lastEventAt)}</Table.Cell>
                    </Table.Row>
                  ))}
                </Table.Body>
              </Table.Content>
            </Table.ScrollContainer>
          </Table>
        )}
      </Card.Content>
    </Card>
  );
}

export function HistorySection({
  selectedTab,
  result,
  loading,
  pageIndex,
  pageSize,
  onTabChange,
  onPageChange,
}: {
  selectedTab: HistoryTab;
  result: HistoryResult | null;
  loading: boolean;
  pageIndex: number;
  pageSize: number;
  onTabChange: (tab: HistoryTab) => void;
  onPageChange: (page: number) => void;
}) {
  return (
    <Card>
      <Card.Header>
        <Card.Title>Activity records</Card.Title>
        <Card.Description>Successful logins and playback attempts across the server.</Card.Description>
      </Card.Header>
      <Card.Content>
        <Segment
          aria-label="Activity record type"
          selectedKey={selectedTab}
          onSelectionChange={(key) => {
            if (key === 'logins' || key === 'watching') onTabChange(key);
          }}
        >
          <Segment.Item id="logins">Login records</Segment.Item>
          <Segment.Item id="watching">Watch history</Segment.Item>
        </Segment>
        <div className="pt-4" key={selectedTab}>
          {loading ? <TableLoading /> : <HistoryTable result={result} />}
          {result !== null && result.page.totalRecordCount > 0 && (
            <HistoryPagination
              pageIndex={pageIndex}
              pageSize={pageSize}
              total={result.page.totalRecordCount}
              onPageChange={onPageChange}
            />
          )}
        </div>
      </Card.Content>
    </Card>
  );
}

function HistoryTable({ result }: { result: HistoryResult | null }) {
  if (result === null || result.page.items.length === 0) return <EmptyTable message="No records found." />;
  return result.kind === 'logins'
    ? <LoginTable items={result.page.items} />
    : <WatchTable items={result.page.items} />;
}

function LoginTable({ items }: { items: LoginHistoryItem[] }) {
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label="Login records" className="min-w-[46rem] table-fixed">
          <Table.Header>
            <Table.Column isRowHeader>User</Table.Column>
            <Table.Column>Application</Table.Column>
            <Table.Column>Device</Table.Column>
            <Table.Column>Signed in</Table.Column>
            <Table.Column>Status</Table.Column>
          </Table.Header>
          <Table.Body>
            {items.map((item) => (
              <Table.Row id={item.sessionId} key={item.sessionId}>
                <Table.Cell><span className="font-medium text-foreground">{item.userName}</span></Table.Cell>
                <Table.Cell>{item.clientName} {item.clientVersion}</Table.Cell>
                <Table.Cell>{item.deviceName}</Table.Cell>
                <Table.Cell>{formatDate(item.createdAt)}</Table.Cell>
                <Table.Cell><LoginStatus item={item} /></Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}

function WatchTable({ items }: { items: WatchHistoryItem[] }) {
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label="Watch history" className="min-w-[48rem] table-fixed">
          <Table.Header>
            <Table.Column isRowHeader>Title</Table.Column>
            <Table.Column>User</Table.Column>
            <Table.Column>Started</Table.Column>
            <Table.Column>Progress</Table.Column>
            <Table.Column>Status</Table.Column>
          </Table.Header>
          <Table.Body>
            {items.map((item) => (
              <Table.Row id={item.sessionId} key={item.sessionId}>
                <Table.Cell><TitleCell name={item.itemName} type={item.itemType} /></Table.Cell>
                <Table.Cell>{item.userName}</Table.Cell>
                <Table.Cell>{formatDate(item.startedAt)}</Table.Cell>
                <Table.Cell>{formatProgress(item.positionTicks, item.runtimeTicks)}</Table.Cell>
                <Table.Cell>
                  <Chip color={item.stoppedAt === null ? 'accent' : 'default'} size="sm" variant="soft">
                    {item.stoppedAt === null ? 'In progress' : 'Stopped'}
                  </Chip>
                </Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}

function PlaybackProgress({ item }: { item: NowPlayingItem }) {
  const value = progressValue(item.positionTicks, item.runtimeTicks);
  return (
    <div className="w-32">
      <ProgressBar aria-label={`${item.itemName} playback progress`} color="success" size="sm" value={value}>
        <ProgressBar.Track><ProgressBar.Fill /></ProgressBar.Track>
      </ProgressBar>
      <span className="mt-1 block text-xs tabular-nums text-muted">{value}%</span>
    </div>
  );
}

function HistoryPagination({ pageIndex, pageSize, total, onPageChange }: { pageIndex: number; pageSize: number; total: number; onPageChange: (page: number) => void }) {
  const first = pageIndex * pageSize + 1;
  const last = Math.min(total, first + pageSize - 1);
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  return (
    <Pagination aria-label="Activity history pagination" className="mt-4 flex flex-wrap items-center justify-between gap-3">
      <Pagination.Summary>{first}-{last} of {total}</Pagination.Summary>
      <Pagination.Content>
        <Pagination.Item><Pagination.Previous isDisabled={pageIndex === 0} onPress={() => { onPageChange(pageIndex - 1); }}><Pagination.PreviousIcon /></Pagination.Previous></Pagination.Item>
        <Pagination.Item><Pagination.Link isActive isDisabled>{pageIndex + 1}</Pagination.Link></Pagination.Item>
        <Pagination.Item><Pagination.Next isDisabled={pageIndex + 1 >= totalPages} onPress={() => { onPageChange(pageIndex + 1); }}><Pagination.NextIcon /></Pagination.Next></Pagination.Item>
      </Pagination.Content>
    </Pagination>
  );
}

function LoginStatus({ item }: { item: LoginHistoryItem }) {
  return (
    <Chip color={item.status === 'Active' ? 'success' : 'default'} size="sm" variant="soft">
      {item.status}
    </Chip>
  );
}

function TitleCell({ name, type }: { name: string; type: string }) {
  return <span className="block min-w-0"><span className="block truncate font-medium text-foreground">{name}</span><span className="block text-xs text-muted">{type}</span></span>;
}

function EmptyTable({ message }: { message: string }) {
  return <div className="rounded-md border border-dashed border-border px-4 py-10 text-center text-sm text-muted">{message}</div>;
}

function TableLoading() {
  return <div className="grid min-h-36 place-items-center text-sm text-muted" role="status">Loading activity…</div>;
}

function progressValue(position: number, runtime: number | null): number {
  if (runtime === null || runtime <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((position / runtime) * 100)));
}

function formatProgress(position: number, runtime: number | null): string {
  const percent = progressValue(position, runtime);
  return runtime === null ? formatTicks(position) : `${String(percent)}% · ${formatTicks(position)}`;
}

function formatTicks(ticks: number): string {
  const totalSeconds = Math.max(0, Math.floor(ticks / 10_000_000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  return hours > 0 ? `${String(hours)}h ${String(minutes)}m` : `${String(minutes)}m`;
}

function formatDate(value: string): string {
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value));
}
