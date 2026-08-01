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

export function PlaybackTrendChart({ summary, range }: { summary: DashboardSummary; range: DashboardRange }) {
  const data = summary.trend.map((point) => ({
    label: trendLabel(point.bucketStart, range),
    Plays: point.playCount,
    Viewers: point.uniqueViewers,
  }));
  return (
    <Card>
      <Card.Header>
        <Card.Title>Playback activity</Card.Title>
        <Card.Description>Starts and unique viewers in the selected period.</Card.Description>
      </Card.Header>
      <Card.Content>
        <div className="h-64 w-full" role="img" aria-label="Playback activity chart">
          <ResponsiveContainer height="100%" width="100%">
            <BarChart data={data} margin={{ left: -18, right: 8, top: 8 }}>
              <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" vertical={false} />
              <XAxis dataKey="label" fontSize={11} stroke="var(--muted)" tickLine={false} />
              <YAxis allowDecimals={false} fontSize={11} stroke="var(--muted)" tickLine={false} />
              <Tooltip
                contentStyle={tooltipStyle}
                cursor={{ fill: 'var(--default)', opacity: 0.45 }}
              />
              <Bar dataKey="Plays" fill="var(--accent)" maxBarSize={26} radius={[4, 4, 0, 0]} />
              <Bar dataKey="Viewers" fill="var(--success)" maxBarSize={26} radius={[4, 4, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </div>
        <div className="mt-3 flex items-center gap-5 text-xs text-muted">
          <ChartLegend color="var(--accent)" label="Plays" />
          <ChartLegend color="var(--success)" label="Unique viewers" />
        </div>
      </Card.Content>
    </Card>
  );
}

export function TopItemsChart({ summary }: { summary: DashboardSummary }) {
  const data = summary.topItems.map((item) => ({
    name: truncate(item.name, 18),
    Plays: item.playCount,
  }));
  return (
    <Card>
      <Card.Header>
        <Card.Title>Most played</Card.Title>
        <Card.Description>Titles ranked by playback starts.</Card.Description>
      </Card.Header>
      <Card.Content>
        {data.length === 0 ? (
          <ChartEmptyState />
        ) : (
          <div className="h-64 w-full" role="img" aria-label="Most played titles chart">
            <ResponsiveContainer height="100%" width="100%">
              <BarChart data={data} layout="vertical" margin={{ left: 4, right: 16, top: 8 }}>
                <CartesianGrid horizontal={false} stroke="var(--border)" strokeDasharray="3 3" />
                <XAxis allowDecimals={false} fontSize={11} stroke="var(--muted)" tickLine={false} type="number" />
                <YAxis dataKey="name" fontSize={11} stroke="var(--muted)" tickLine={false} type="category" width={118} />
                <Tooltip contentStyle={tooltipStyle} cursor={{ fill: 'var(--default)', opacity: 0.45 }} />
                <Bar dataKey="Plays" fill="var(--accent)" maxBarSize={18} radius={[0, 4, 4, 0]} />
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
  return (
    <div className="grid h-64 place-items-center rounded-md border border-dashed border-border text-center">
      <div><p className="text-sm font-medium text-foreground">No playback yet</p><p className="mt-1 text-xs text-muted">Activity will appear after a title starts.</p></div>
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
