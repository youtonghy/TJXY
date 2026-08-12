import { Card, Chip, Pagination, ProgressBar, Table } from '@heroui/react';
import { Segment } from '@heroui-pro/react/segment';
import { useTranslate } from '../settings/i18n';

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
  const tr = useTranslate();
  return (
    <Card>
      <Card.Header className="flex-row items-start justify-between gap-4">
        <div>
          <Card.Title>{tr('Watching now', '正在观看')}</Card.Title>
          <Card.Description>{tr('Playback sessions active within the last minute.', '最近一分钟内仍活跃的播放会话。')}</Card.Description>
        </div>
        <Chip color={items.length > 0 ? 'success' : 'default'} size="sm" variant="soft">
          {items.length} {tr('active', '个活跃')}
        </Chip>
      </Card.Header>
      <Card.Content>
        {items.length === 0 ? <EmptyTable message={tr('Nobody is watching right now.', '当前无人观看。')} /> : (
          <Table variant="secondary">
            <Table.ScrollContainer>
              <Table.Content aria-label={tr('Currently watching users', '当前观看用户')} className="min-w-[48rem] table-fixed">
                <Table.Header>
                  <Table.Column isRowHeader>{tr('User', '用户')}</Table.Column>
                  <Table.Column>{tr('Title', '标题')}</Table.Column>
                  <Table.Column>{tr('Progress', '进度')}</Table.Column>
                  <Table.Column>{tr('Device', '设备')}</Table.Column>
                  <Table.Column>{tr('Last activity', '最近活动')}</Table.Column>
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
  const tr = useTranslate();
  return (
    <Card>
      <Card.Header>
        <Card.Title>{tr('Activity records', '活动记录')}</Card.Title>
        <Card.Description>{tr('Successful logins and playback attempts across the server.', '服务器上的成功登录和播放记录。')}</Card.Description>
      </Card.Header>
      <Card.Content>
        <Segment
          aria-label={tr('Activity record type', '活动记录类型')}
          selectedKey={selectedTab}
          onSelectionChange={(key) => {
            if (key === 'logins' || key === 'watching') onTabChange(key);
          }}
        >
          <Segment.Item id="logins">{tr('Login records', '登录记录')}</Segment.Item>
          <Segment.Item id="watching">{tr('Watch history', '观看历史')}</Segment.Item>
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
  const tr = useTranslate();
  if (result === null || result.page.items.length === 0) return <EmptyTable message={tr('No records found.', '未找到记录。')} />;
  return result.kind === 'logins'
    ? <LoginTable items={result.page.items} />
    : <WatchTable items={result.page.items} />;
}

function LoginTable({ items }: { items: LoginHistoryItem[] }) {
  const tr = useTranslate();
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label={tr('Login records', '登录记录')} className="min-w-[46rem] table-fixed">
          <Table.Header>
            <Table.Column isRowHeader>{tr('User', '用户')}</Table.Column>
            <Table.Column>{tr('Application', '应用')}</Table.Column>
            <Table.Column>{tr('Device', '设备')}</Table.Column>
            <Table.Column>{tr('Signed in', '登录时间')}</Table.Column>
            <Table.Column>{tr('Status', '状态')}</Table.Column>
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
  const tr = useTranslate();
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label={tr('Watch history', '观看历史')} className="min-w-[48rem] table-fixed">
          <Table.Header>
            <Table.Column isRowHeader>{tr('Title', '标题')}</Table.Column>
            <Table.Column>{tr('User', '用户')}</Table.Column>
            <Table.Column>{tr('Started', '开始时间')}</Table.Column>
            <Table.Column>{tr('Progress', '进度')}</Table.Column>
            <Table.Column>{tr('Status', '状态')}</Table.Column>
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
                    {item.stoppedAt === null ? tr('In progress', '播放中') : tr('Stopped', '已停止')}
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
  const tr = useTranslate();
  const value = progressValue(item.positionTicks, item.runtimeTicks);
  return (
    <div className="w-32">
      <ProgressBar aria-label={`${item.itemName} ${tr('playback progress', '播放进度')}`} color="success" size="sm" value={value}>
        <ProgressBar.Track><ProgressBar.Fill /></ProgressBar.Track>
      </ProgressBar>
      <span className="mt-1 block text-xs tabular-nums text-muted">{value}%</span>
    </div>
  );
}

function HistoryPagination({ pageIndex, pageSize, total, onPageChange }: { pageIndex: number; pageSize: number; total: number; onPageChange: (page: number) => void }) {
  const tr = useTranslate();
  const first = pageIndex * pageSize + 1;
  const last = Math.min(total, first + pageSize - 1);
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  return (
    <Pagination aria-label={tr('Activity history pagination', '活动历史分页')} className="mt-4 flex flex-wrap items-center justify-between gap-3">
      <Pagination.Summary>{first}-{last} {tr('of', '共')} {total}</Pagination.Summary>
      <Pagination.Content>
        <Pagination.Item><Pagination.Previous isDisabled={pageIndex === 0} onPress={() => { onPageChange(pageIndex - 1); }}><Pagination.PreviousIcon /></Pagination.Previous></Pagination.Item>
        <Pagination.Item><Pagination.Link isActive isDisabled>{pageIndex + 1}</Pagination.Link></Pagination.Item>
        <Pagination.Item><Pagination.Next isDisabled={pageIndex + 1 >= totalPages} onPress={() => { onPageChange(pageIndex + 1); }}><Pagination.NextIcon /></Pagination.Next></Pagination.Item>
      </Pagination.Content>
    </Pagination>
  );
}

function LoginStatus({ item }: { item: LoginHistoryItem }) {
  const tr = useTranslate();
  return (
    <Chip color={item.status === 'Active' ? 'success' : 'default'} size="sm" variant="soft">
      {item.status === 'Active' ? tr('Active', '活跃') : tr(item.status, item.status)}
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
  const tr = useTranslate();
  return <div className="grid min-h-36 place-items-center text-sm text-muted" role="status">{tr('Loading activity…', '正在加载活动记录…')}</div>;
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
