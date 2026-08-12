import { Card } from '@heroui/react';
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';

import type { DashboardRange, DashboardSummary } from './dashboardApi';
import { useTranslate } from '../settings/i18n';

export function PlaybackTrendChart({ summary, range }: { summary: DashboardSummary; range: DashboardRange }) {
  const tr = useTranslate();
  const data = summary.trend.map((point) => ({
    label: trendLabel(point.bucketStart, range),
    [tr('Plays', '播放次数')]: point.playCount,
    [tr('Viewers', '观众数')]: point.uniqueViewers,
  }));
  return (
    <Card>
      <Card.Header>
        <Card.Title>{tr('Playback activity', '播放活动')}</Card.Title>
        <Card.Description>{tr('Starts and unique viewers in the selected period.', '所选时段内的播放次数和独立观众数。')}</Card.Description>
      </Card.Header>
      <Card.Content>
        <div className="h-64 w-full" role="img" aria-label={tr('Playback activity chart', '播放活动图表')}>
          <ResponsiveContainer height="100%" width="100%">
            <BarChart data={data} margin={{ left: -18, right: 8, top: 8 }}>
              <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" vertical={false} />
              <XAxis dataKey="label" fontSize={11} stroke="var(--muted)" tickLine={false} />
              <YAxis allowDecimals={false} fontSize={11} stroke="var(--muted)" tickLine={false} />
              <Tooltip
                contentStyle={tooltipStyle}
                cursor={{ fill: 'var(--default)', opacity: 0.45 }}
              />
              <Bar dataKey={tr('Plays', '播放次数')} fill="var(--accent)" maxBarSize={26} radius={[4, 4, 0, 0]} />
              <Bar dataKey={tr('Viewers', '观众数')} fill="var(--success)" maxBarSize={26} radius={[4, 4, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </div>
        <div className="mt-3 flex items-center gap-5 text-xs text-muted">
          <ChartLegend color="var(--accent)" label={tr('Plays', '播放次数')} />
          <ChartLegend color="var(--success)" label={tr('Unique viewers', '独立观众')} />
        </div>
      </Card.Content>
    </Card>
  );
}

export function TopItemsChart({ summary }: { summary: DashboardSummary }) {
  const tr = useTranslate();
  const data = summary.topItems.map((item) => ({
    name: truncate(item.name, 18),
    [tr('Plays', '播放次数')]: item.playCount,
  }));
  return (
    <Card>
      <Card.Header>
        <Card.Title>{tr('Most played', '播放最多')}</Card.Title>
        <Card.Description>{tr('Titles ranked by playback starts.', '按播放次数排列的媒体内容。')}</Card.Description>
      </Card.Header>
      <Card.Content>
        {data.length === 0 ? (
          <ChartEmptyState />
        ) : (
          <div className="h-64 w-full" role="img" aria-label={tr('Most played titles chart', '播放最多的内容图表')}>
            <ResponsiveContainer height="100%" width="100%">
              <BarChart data={data} layout="vertical" margin={{ left: 4, right: 16, top: 8 }}>
                <CartesianGrid horizontal={false} stroke="var(--border)" strokeDasharray="3 3" />
                <XAxis allowDecimals={false} fontSize={11} stroke="var(--muted)" tickLine={false} type="number" />
                <YAxis dataKey="name" fontSize={11} stroke="var(--muted)" tickLine={false} type="category" width={118} />
                <Tooltip contentStyle={tooltipStyle} cursor={{ fill: 'var(--default)', opacity: 0.45 }} />
                <Bar dataKey={tr('Plays', '播放次数')} fill="var(--accent)" maxBarSize={18} radius={[0, 4, 4, 0]} />
              </BarChart>
            </ResponsiveContainer>
          </div>
        )}
      </Card.Content>
    </Card>
  );
}

function ChartLegend({ color, label }: { color: string; label: string }) {
  return <span className="inline-flex items-center gap-1.5"><span className="size-2.5 rounded-sm" style={{ backgroundColor: color }} />{label}</span>;
}

function ChartEmptyState() {
  const tr = useTranslate();
  return (
    <div className="grid h-64 place-items-center rounded-md border border-dashed border-border text-center">
      <div><p className="text-sm font-medium text-foreground">{tr('No playback yet', '暂无播放记录')}</p><p className="mt-1 text-xs text-muted">{tr('Activity will appear after a title starts.', '开始播放媒体内容后，活动会显示在这里。')}</p></div>
    </div>
  );
}

function trendLabel(value: string, range: DashboardRange): string {
  const date = new Date(value);
  return new Intl.DateTimeFormat(undefined, range === 'today'
    ? { hour: 'numeric' }
    : { month: 'short', day: 'numeric' }).format(date);
}

function truncate(value: string, maximum: number): string {
  return value.length <= maximum ? value : `${value.slice(0, maximum - 1)}…`;
}

const tooltipStyle = {
  background: 'var(--surface)',
  border: '1px solid var(--border)',
  borderRadius: '0.5rem',
  color: 'var(--foreground)',
  fontSize: '0.75rem',
};
