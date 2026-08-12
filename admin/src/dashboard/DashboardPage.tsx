import { Alert, Button, Skeleton, Tabs } from '@heroui/react';
import { KPI } from '@heroui-pro/react/kpi';
import { KPIGroup } from '@heroui-pro/react/kpi-group';
import { CirclePlay, Eye, LibraryBig, RefreshCw, UsersRound } from 'lucide-react';
import { useLogoutIfAccessDenied } from 'ra-core';
import { useCallback, useEffect, useState } from 'react';

import { PageHeader } from '../ui/PageHeader';
import { useTranslate } from '../settings/i18n';
import { PlaybackTrendChart, TopItemsChart } from './DashboardCharts';
import {
  getDashboardSnapshot,
  getLoginHistory,
  getWatchHistory,
  type DashboardRange,
  type DashboardSnapshot,
} from './dashboardApi';
import { HistorySection, NowPlayingTable, type HistoryResult, type HistoryTab } from './DashboardTables';

const HISTORY_PAGE_SIZE = 25;

export function DashboardPage() {
  const tr = useTranslate();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [range, setRange] = useState<DashboardRange>('today');
  const [snapshot, setSnapshot] = useState<DashboardSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [historyTab, setHistoryTab] = useState<HistoryTab>('logins');
  const [historyPage, setHistoryPage] = useState(0);
  const [historyResult, setHistoryResult] = useState<HistoryResult | null>(null);
  const [historyLoading, setHistoryLoading] = useState(true);

  const refresh = useCallback(() => {
    setLoading(true);
    setHistoryLoading(true);
    setRefreshKey((value) => value + 1);
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    void getDashboardSnapshot(range, controller.signal)
      .then((value) => {
        if (controller.signal.aborted) return;
        setSnapshot(value);
        setError(null);
      })
      .catch(async (loadError: unknown) => {
        if (controller.signal.aborted) return;
        if (!await logoutIfAccessDenied(loadError)) setError(loadError);
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => { controller.abort(); };
  }, [logoutIfAccessDenied, range, refreshKey]);

  useEffect(() => {
    const controller = new AbortController();
    const startIndex = historyPage * HISTORY_PAGE_SIZE;
    const request = historyTab === 'logins'
      ? getLoginHistory(startIndex, HISTORY_PAGE_SIZE, controller.signal)
          .then((page) => ({ kind: 'logins' as const, page }))
      : getWatchHistory(startIndex, HISTORY_PAGE_SIZE, controller.signal)
          .then((page) => ({ kind: 'watching' as const, page }));
    void request
      .then((value) => {
        if (!controller.signal.aborted) setHistoryResult(value);
      })
      .catch(async (loadError: unknown) => {
        if (!controller.signal.aborted) await logoutIfAccessDenied(loadError);
      })
      .finally(() => {
        if (!controller.signal.aborted) setHistoryLoading(false);
      });
    return () => { controller.abort(); };
  }, [historyPage, historyTab, logoutIfAccessDenied, refreshKey]);

  return (
    <div className="space-y-6">
      <PageHeader
        actions={(
          <div className="flex w-full min-w-0 items-center gap-2 sm:w-auto">
            <RangeTabs
              range={range}
              onRangeChange={(nextRange) => {
                setLoading(true);
                setRange(nextRange);
              }}
            />
            <Button
              isIconOnly
              aria-label={tr('Refresh dashboard', '刷新仪表盘')}
              className="shrink-0"
              onPress={refresh}
              variant="tertiary"
            >
              <RefreshCw aria-hidden="true" className={`size-4 ${loading ? 'animate-spin' : ''}`} />
            </Button>
          </div>
        )}
        description={tr('Server-wide catalog, playback, and account activity.', '查看服务器整体媒体库、播放和账户活动。')}
        title={tr('Dashboard', '仪表盘')}
      />

      {error !== null && (
        <Alert status="danger">
          <Alert.Indicator />
          <Alert.Content><Alert.Title>{tr('Dashboard data unavailable', '无法加载仪表盘数据')}</Alert.Title><Alert.Description>{tr('Try refreshing after the server is ready.', '请在服务器就绪后刷新重试。')}</Alert.Description></Alert.Content>
        </Alert>
      )}

      {loading && snapshot === null ? <DashboardSkeleton /> : snapshot !== null && (
        <>
          <div className="flex flex-wrap items-baseline justify-between gap-2">
            <p className="text-sm text-muted">{rangeLabel(range, tr)} · {formatWindow(snapshot.summary.from, snapshot.summary.to)}</p>
            <p className="text-xs tabular-nums text-muted">{snapshot.summary.catalogTotal.toLocaleString()} {tr('catalog records', '条媒体记录')}</p>
          </div>
          <KpiGrid snapshot={snapshot} />
          <div className="grid gap-4 lg:grid-cols-2">
            <PlaybackTrendChart range={range} summary={snapshot.summary} />
            <TopItemsChart summary={snapshot.summary} />
          </div>
          <NowPlayingTable items={snapshot.nowPlaying} />
        </>
      )}

      <HistorySection
        loading={historyLoading}
        pageIndex={historyPage}
        pageSize={HISTORY_PAGE_SIZE}
        result={historyResult?.kind === historyTab ? historyResult : null}
        selectedTab={historyTab}
        onPageChange={(page) => {
          setHistoryLoading(true);
          setHistoryPage(page);
        }}
        onTabChange={(tab) => {
          setHistoryLoading(true);
          setHistoryTab(tab);
          setHistoryPage(0);
        }}
      />
    </div>
  );
}

function RangeTabs({ range, onRangeChange }: { range: DashboardRange; onRangeChange: (range: DashboardRange) => void }) {
  const tr = useTranslate();
  return (
    <Tabs
      className="min-w-0 flex-1 sm:flex-none"
      selectedKey={range}
      onSelectionChange={(key) => { if (key === 'today' || key === '7d' || key === '30d') onRangeChange(key); }}
    >
      <Tabs.ListContainer className="w-full sm:w-60"><Tabs.List aria-label={tr('Dashboard time range', '仪表盘时间范围')} className="grid w-full min-w-0 grid-cols-3">
        <Tabs.Tab className="whitespace-nowrap px-3" id="today">{tr('Today', '今天')}<Tabs.Indicator /></Tabs.Tab>
        <Tabs.Tab className="whitespace-nowrap px-3" id="7d">{tr('7 days', '7 天')}<Tabs.Indicator /></Tabs.Tab>
        <Tabs.Tab className="whitespace-nowrap px-3" id="30d">{tr('30 days', '30 天')}<Tabs.Indicator /></Tabs.Tab>
      </Tabs.List></Tabs.ListContainer>
    </Tabs>
  );
}

function KpiGrid({ snapshot }: { snapshot: DashboardSnapshot }) {
  const tr = useTranslate();
  const summary = snapshot.summary;
  return (
    <div aria-label={tr('Server KPIs', '服务器关键指标')} className="grid gap-3 lg:grid-cols-2" role="group">
      <KPIGroup aria-label={tr('Accounts and library', '账户与媒体库')}>
        <DashboardKpi detail={`${summary.usersDisabled.toLocaleString()} ${tr('disabled', '个已禁用')}`} icon={UsersRound} label={tr('Users', '用户')} value={summary.usersTotal} />
        <KPIGroup.Separator />
        <DashboardKpi
          detail={`${summary.movies.toLocaleString()} ${tr('movies', '部电影')} · ${summary.series.toLocaleString()} ${tr('series', '部剧集')} · ${summary.episodes.toLocaleString()} ${tr('episodes', '集')}`}
          icon={LibraryBig}
          label={tr('Catalog records', '媒体记录')}
          value={summary.catalogTotal}
        />
      </KPIGroup>
      <KPIGroup aria-label={tr('Selected period activity', '所选时段活动')}>
        <DashboardKpi detail={tr('In the selected period', '所选时段内')} icon={CirclePlay} label={tr('Playback starts', '播放次数')} value={summary.playCount} />
        <KPIGroup.Separator />
        <DashboardKpi detail={tr('Distinct accounts with playback', '发生过播放的不同账户')} icon={Eye} label={tr('Unique viewers', '独立观众')} value={summary.uniqueViewers} />
      </KPIGroup>
    </div>
  );
}

function DashboardKpi({ detail, icon: Icon, label, value }: { detail: string; icon: typeof UsersRound; label: string; value: number }) {
  return (
    <KPI>
      <KPI.Header>
        <KPI.Icon className="bg-accent/10 text-accent"><Icon aria-hidden="true" className="size-4" /></KPI.Icon>
        <KPI.Title>{label}</KPI.Title>
      </KPI.Header>
      <KPI.Content><KPI.Value maximumFractionDigits={0} value={value} /></KPI.Content>
      <KPI.Footer className="text-xs text-muted">{detail}</KPI.Footer>
    </KPI>
  );
}

function DashboardSkeleton() {
  const tr = useTranslate();
  return <div className="space-y-4" role="status" aria-label={tr('Loading dashboard', '正在加载仪表盘')}><div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">{Array.from({ length: 4 }, (_, index) => <Skeleton className="h-32 rounded-md" key={index} />)}</div><div className="grid gap-4 lg:grid-cols-2"><Skeleton className="h-80 rounded-md" /><Skeleton className="h-80 rounded-md" /></div></div>;
}

function rangeLabel(range: DashboardRange, tr: (english: string, chinese: string) => string): string {
  if (range === 'today') return tr('Today', '今天');
  return range === '7d' ? tr('Last 7 days', '最近 7 天') : tr('Last 30 days', '最近 30 天');
}

function formatWindow(from: string, to: string): string {
  const format = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' });
  return `${format.format(new Date(from))} – ${format.format(new Date(to))}`;
}
