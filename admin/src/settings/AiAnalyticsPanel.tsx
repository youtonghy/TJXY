import { Alert, Button, Card, Chip, ProgressBar, Skeleton, Table } from '@heroui/react';
import { Activity, Coins, MessageSquareText, RefreshCw, Users } from 'lucide-react';
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';

import type { AiAnalytics, AiExecutionOutcome } from './aiSettingsApi';

export function AiAnalyticsPanel({ analytics, error, loading, onRetry }: {
  analytics: AiAnalytics | null;
  error: unknown;
  loading: boolean;
  onRetry: () => void;
}) {
  if (loading && analytics === null) return <AnalyticsSkeleton />;
  if (error !== null && analytics === null) {
    return (
      <Alert role="alert" status="danger">
        <Alert.Content><Alert.Title>AI 统计暂时无法加载</Alert.Title><Alert.Description>配置仍可正常编辑，请单独重试统计数据。</Alert.Description></Alert.Content>
        <Button onPress={onRetry} size="sm" variant="tertiary"><RefreshCw className="size-4" />重试统计</Button>
      </Alert>
    );
  }
  if (analytics === null) return null;
  const successRate = rate(analytics.summary.successfulRequests, analytics.summary.totalRequests);
  return (
    <section aria-labelledby="ai-analytics-heading" className="space-y-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div><h2 className="text-lg font-semibold" id="ai-analytics-heading">AI 运行统计</h2><p className="text-sm text-muted">今天按服务器本地时区统计；Token 仅采用上游返回的真实 usage。</p></div>
        <div className="flex items-center gap-2"><span className="text-xs text-muted">{analytics.window.timeZone}</span><Button aria-label="刷新 AI 统计" isIconOnly isPending={loading} onPress={onRetry} size="sm" variant="ghost"><RefreshCw className="size-4" /></Button></div>
      </div>
      {error !== null && <Alert role="alert" status="warning"><Alert.Content><Alert.Title>统计刷新失败</Alert.Title><Alert.Description>当前仍显示上一次成功加载的数据。</Alert.Description></Alert.Content></Alert>}
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <MetricCard icon={<MessageSquareText className="size-4" />} label="今日对话" value={`${formatNumber(analytics.summary.totalRequests)} 次`} />
        <MetricCard icon={<Users className="size-4" />} label="活跃用户" value={`${formatNumber(analytics.summary.activeUsers)} 位`} />
        <MetricCard icon={<Coins className="size-4" />} label="总 Token" value={analytics.summary.totalTokens === null ? '—' : formatNumber(analytics.summary.totalTokens)}>
          {analytics.summary.knownTokenRequests < analytics.summary.totalRequests && <span className="text-xs text-warning">Token 数据不完整</span>}
        </MetricCard>
        <MetricCard icon={<Activity className="size-4" />} label="成功率" value={`${String(successRate)}%`}><span className="text-xs text-muted">失败 {formatNumber(analytics.summary.failedRequests)} 次</span></MetricCard>
      </div>
      <UsageTrend analytics={analytics} />
      <div className="grid gap-4 xl:grid-cols-2">
        <UserRanking analytics={analytics} />
        <ModelRanking analytics={analytics} />
      </div>
      <FailureTable analytics={analytics} />
    </section>
  );
}

function MetricCard({ icon, label, value, children }: { icon: React.ReactNode; label: string; value: string; children?: React.ReactNode }) {
  return (
    <Card aria-label={label}>
      <Card.Content className="flex min-h-28 items-start justify-between gap-3 p-4">
        <div><p className="text-sm text-muted">{label}</p><p className="mt-2 text-2xl font-semibold tabular-nums">{value}</p><div className="mt-1 min-h-4">{children}</div></div>
        <span className="grid size-8 shrink-0 place-items-center rounded-md bg-default text-foreground">{icon}</span>
      </Card.Content>
    </Card>
  );
}

function UsageTrend({ analytics }: { analytics: AiAnalytics }) {
  const data = analytics.daily.map((item) => ({ label: dayLabel(item.day), 请求: item.totalRequests, 失败: item.failedRequests }));
  return (
    <Card>
      <Card.Header><Card.Title>最近 14 天趋势</Card.Title><Card.Description>每日完成和失败的 AI 请求数量。</Card.Description></Card.Header>
      <Card.Content>
        <div aria-label="最近 14 天 AI 请求趋势" className="h-64 w-full" role="img">
          <ResponsiveContainer height="100%" width="100%">
            <BarChart data={data} margin={{ left: -20, right: 8, top: 8 }}>
              <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" vertical={false} />
              <XAxis dataKey="label" fontSize={11} stroke="var(--muted)" tickLine={false} />
              <YAxis allowDecimals={false} fontSize={11} stroke="var(--muted)" tickLine={false} />
              <Tooltip contentStyle={tooltipStyle} cursor={{ fill: 'var(--default)', opacity: 0.4 }} />
              <Bar dataKey="请求" fill="var(--accent)" maxBarSize={28} radius={[4, 4, 0, 0]} />
              <Bar dataKey="失败" fill="var(--danger)" maxBarSize={28} radius={[4, 4, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </Card.Content>
    </Card>
  );
}

function UserRanking({ analytics }: { analytics: AiAnalytics }) {
  return (
    <RankingCard description="按今天的对话轮次排序。" title="用户对话排行">
      {analytics.users.length === 0 ? <EmptyTable message="今天还没有用户发起 AI 对话。" /> : (
        <Table variant="secondary"><Table.ScrollContainer><Table.Content aria-label="用户对话排行" className="min-w-[34rem] table-fixed"><Table.Header><Table.Column isRowHeader>用户</Table.Column><Table.Column>对话</Table.Column><Table.Column>Token</Table.Column><Table.Column>成功率</Table.Column></Table.Header><Table.Body>{analytics.users.map((item) => <Table.Row id={item.userId} key={item.userId}><Table.Cell><span className="font-medium">{item.username}</span></Table.Cell><Table.Cell>{formatNumber(item.totalRequests)} 次</Table.Cell><Table.Cell>{tokenLabel(item.totalTokens)}</Table.Cell><Table.Cell><Rate value={rate(item.successfulRequests, item.totalRequests)} /></Table.Cell></Table.Row>)}</Table.Body></Table.Content></Table.ScrollContainer></Table>
      )}
    </RankingCard>
  );
}

function ModelRanking({ analytics }: { analytics: AiAnalytics }) {
  return (
    <RankingCard description="按今天的请求量比较模型使用情况。" title="模型用量排行">
      {analytics.models.length === 0 ? <EmptyTable message="今天还没有模型调用记录。" /> : (
        <Table variant="secondary"><Table.ScrollContainer><Table.Content aria-label="模型用量排行" className="min-w-[38rem] table-fixed"><Table.Header><Table.Column isRowHeader>模型</Table.Column><Table.Column>请求</Table.Column><Table.Column>Token</Table.Column><Table.Column>成功率</Table.Column></Table.Header><Table.Body>{analytics.models.map((item) => <Table.Row id={item.modelId} key={item.modelId}><Table.Cell><span className="block font-medium">{item.displayName}</span><span className="block truncate text-xs text-muted">{item.upstreamModelId}</span></Table.Cell><Table.Cell>{formatNumber(item.totalRequests)} 次</Table.Cell><Table.Cell>{tokenLabel(item.totalTokens)}</Table.Cell><Table.Cell><Rate value={rate(item.successfulRequests, item.totalRequests)} /></Table.Cell></Table.Row>)}</Table.Body></Table.Content></Table.ScrollContainer></Table>
      )}
    </RankingCard>
  );
}

function RankingCard({ title, description, children }: { title: string; description: string; children: React.ReactNode }) {
  return <Card><Card.Header><Card.Title>{title}</Card.Title><Card.Description>{description}</Card.Description></Card.Header><Card.Content>{children}</Card.Content></Card>;
}

function FailureTable({ analytics }: { analytics: AiAnalytics }) {
  return (
    <Card>
      <Card.Header className="flex-row items-start justify-between gap-3"><div><Card.Title>最近失败</Card.Title><Card.Description>仅记录安全分类，不保存提示词、回答或上游错误正文。</Card.Description></div><Chip color={analytics.recentFailures.length > 0 ? 'danger' : 'success'} size="sm" variant="soft">{analytics.recentFailures.length} 条</Chip></Card.Header>
      <Card.Content>
        {analytics.recentFailures.length === 0 ? <EmptyTable message="最近 14 天没有失败记录。" /> : (
          <Table variant="secondary"><Table.ScrollContainer><Table.Content aria-label="AI 最近失败记录" className="min-w-[48rem] table-fixed"><Table.Header><Table.Column>时间</Table.Column><Table.Column isRowHeader>用户</Table.Column><Table.Column>模型</Table.Column><Table.Column>分类</Table.Column><Table.Column>耗时</Table.Column></Table.Header><Table.Body>{analytics.recentFailures.map((item) => <Table.Row id={item.id} key={item.id}><Table.Cell>{formatDate(item.startedAt)}</Table.Cell><Table.Cell>{item.username}</Table.Cell><Table.Cell>{item.modelDisplayName}</Table.Cell><Table.Cell><Chip color="danger" size="sm" variant="soft">{outcomeLabel(item.outcome)}</Chip></Table.Cell><Table.Cell>{durationLabel(item.elapsedMs)}</Table.Cell></Table.Row>)}</Table.Body></Table.Content></Table.ScrollContainer></Table>
        )}
      </Card.Content>
    </Card>
  );
}

function Rate({ value }: { value: number }) {
  return <div className="w-28"><ProgressBar aria-label={`成功率 ${String(value)}%`} color={value >= 95 ? 'success' : value >= 70 ? 'accent' : 'danger'} size="sm" value={value}><ProgressBar.Track><ProgressBar.Fill /></ProgressBar.Track></ProgressBar><span className="mt-1 block text-xs tabular-nums text-muted">{value}%</span></div>;
}

function EmptyTable({ message }: { message: string }) { return <div className="rounded-md border border-dashed border-border px-4 py-10 text-center text-sm text-muted">{message}</div>; }
function AnalyticsSkeleton() { return <section aria-label="正在加载 AI 统计" className="space-y-4"><Skeleton className="h-8 w-48 rounded-md" /><div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">{Array.from({ length: 4 }, (_, index) => <Skeleton className="h-28 rounded-lg" key={index} />)}</div><Skeleton className="h-72 rounded-lg" /></section>; }
function rate(success: number, total: number): number { return total === 0 ? 0 : Math.round((success / total) * 1000) / 10; }
function formatNumber(value: number): string { return new Intl.NumberFormat('zh-CN').format(value); }
function tokenLabel(value: number | null): string { return value === null ? '未知' : `${formatNumber(value)} tokens`; }
function dayLabel(value: string): string { return new Intl.DateTimeFormat('zh-CN', { month: 'numeric', day: 'numeric' }).format(new Date(`${value}T00:00:00Z`)); }
function formatDate(value: string): string { return new Intl.DateTimeFormat('zh-CN', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(new Date(value)); }
function durationLabel(milliseconds: number): string { return milliseconds >= 1000 ? `${(milliseconds / 1000).toFixed(1)} 秒` : `${String(milliseconds)} ms`; }
function outcomeLabel(outcome: AiExecutionOutcome): string { return ({ upstream_rejected: '上游拒绝', upstream_invalid: '响应异常', upstream_timeout: '上游超时', tool_failed: '工具失败', persistence_failed: '保存失败', internal_error: '内部错误' })[outcome]; }

const tooltipStyle = { background: 'var(--surface)', border: '1px solid var(--border)', borderRadius: '0.5rem', color: 'var(--foreground)', fontSize: '0.75rem' };
