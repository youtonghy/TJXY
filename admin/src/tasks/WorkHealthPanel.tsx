import { Button, Card } from '@heroui/react';
import { useLogoutIfAccessDenied } from 'ra-core';
import { useEffect, useRef, useState } from 'react';
import { apiRequest } from '../api/httpClient';
import { useTranslate } from '../settings/i18n';

type Space = number | null;
interface HealthResponse {
  Health: {
    SampledAt: string; Backend: string; PendingJobs: number; OldestPendingAt: string | null;
    RetentionCandidates: number | null; RetentionDays: number | null;
    AllocatedBytes: Space; FreeBytes: Space; WalBytes: Space; SpaceIsEstimated: boolean;
    Tables: { Name: string; Rows: number; DataBytes: Space; IndexBytes: Space; AllocatedBytes: Space }[];
  };
  Cleanup: { LastRunAt: string | null; Deleted: number; Compacted: number; Purged: number; Deferred: number; FailedRuns: number };
}

export function WorkHealthPanel() {
  const tr = useTranslate();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [result, setResult] = useState<HealthResponse | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const controller = useRef<AbortController | null>(null);
  useEffect(() => () => { controller.current?.abort(); }, []);
  const load = async () => {
    if (controller.current) return;
    const request = new AbortController(); controller.current = request;
    setBusy(true); setFailed(false);
    try {
      const response = await apiRequest<unknown>('/Admin/Tasks/Health', { signal: request.signal });
      if (!valid(response)) throw new Error('Invalid work health response');
      if (!request.signal.aborted) setResult(response);
    } catch (error: unknown) {
      if (!request.signal.aborted && !await logoutIfAccessDenied(error)) setFailed(true);
    } finally {
      controller.current = null;
      if (!request.signal.aborted) setBusy(false);
    }
  };
  const unavailable = tr('Unavailable', '未提供');
  const bytes = (value: Space) => value === null ? unavailable : `${(value / 1024 / 1024).toFixed(2)} MiB`;
  return <Card variant="secondary" className="gap-4 p-5 sm:p-6">
    <Card.Header><Card.Title render={(props) => <h2 {...props} />}>{tr('Queue and history capacity', '队列与历史容量')}</Card.Title>
      <Card.Description>{tr('Samples refresh at most every five minutes. Allocated space includes overhead and is not business data growth.', '采样最多每五分钟更新一次；分配空间包含存储开销，不等于业务净增长。')}</Card.Description></Card.Header>
    <Card.Content className="space-y-3">
      <Button variant="secondary" isPending={busy} onPress={() => { void load(); }}>{tr('Load capacity snapshot', '加载容量快照')}</Button>
      {failed && <p role="alert" className="text-sm text-danger">{tr('Capacity sampling failed. Try again later.', '容量采样失败，请稍后重试。')}</p>}
      {result && <>
        <p className="text-sm text-muted">{result.Health.Backend} · {new Date(result.Health.SampledAt).toLocaleString()} {result.Health.SpaceIsEstimated && tr('(space estimated)', '（空间为估算值）')}</p>
        <p className="text-sm">{tr('Pending', '等待任务')}: {result.Health.PendingJobs} · {tr('Oldest waiting since', '最早等待时间')}: {result.Health.OldestPendingAt ? new Date(result.Health.OldestPendingAt).toLocaleString() : '—'} · {tr('Retention candidates', '到期清理候选')}: {result.Health.RetentionCandidates ?? tr('Retention disabled', '保留清理未启用')}</p>
        <p className="text-sm">{tr('Database allocated', '数据库分配空间')}: {bytes(result.Health.AllocatedBytes)} · {tr('Reusable free space', '可复用空闲空间')}: {bytes(result.Health.FreeBytes)} · WAL: {bytes(result.Health.WalBytes)}</p>
        <div className="overflow-x-auto"><table className="w-full text-left text-sm"><thead><tr>
          {[tr('Table', '表'), tr('Rows', '记录数'), tr('Data', '数据'), tr('Indexes', '索引'), tr('Allocated', '分配空间')].map((heading) => <th className="px-2 py-1" key={heading}>{heading}</th>)}
        </tr></thead><tbody>{result.Health.Tables.map((table) => <tr key={table.Name}><td className="px-2 py-1 font-mono">{table.Name}</td><td className="px-2 py-1">{table.Rows}</td><td className="px-2 py-1">{bytes(table.DataBytes)}</td><td className="px-2 py-1">{bytes(table.IndexBytes)}</td><td className="px-2 py-1">{bytes(table.AllocatedBytes)}</td></tr>)}</tbody></table></div>
        <p className="text-sm text-muted">{tr('Cleanup since this process started', '本进程启动后的清理累计')}: {tr('Deleted jobs', '删除任务')} {result.Cleanup.Deleted} · {tr('Compacted jobs', '压缩任务')} {result.Cleanup.Compacted} · {tr('Purged publications', '删除旧发布')} {result.Cleanup.Purged} · {tr('Deferred batch entries', '批次延期记录')} {result.Cleanup.Deferred} · {tr('Failed runs', '执行失败')} {result.Cleanup.FailedRuns}</p>
      </>}
    </Card.Content>
  </Card>;
}
function record(value: unknown): value is Record<string, unknown> { return typeof value === 'object' && value !== null && !Array.isArray(value); }
function integer(value: unknown): value is number { return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0; }
function space(value: unknown): boolean { return value === null || integer(value); }
function date(value: unknown): boolean { return typeof value === 'string' && Number.isFinite(Date.parse(value)); }
function valid(value: unknown): value is HealthResponse {
  if (!record(value) || !record(value.Health) || !record(value.Cleanup)) return false;
  const h = value.Health; const c = value.Cleanup;
  return date(h.SampledAt) && typeof h.Backend === 'string' && integer(h.PendingJobs) && (h.OldestPendingAt === null || date(h.OldestPendingAt))
    && space(h.RetentionCandidates) && space(h.RetentionDays) && space(h.AllocatedBytes) && space(h.FreeBytes) && space(h.WalBytes) && typeof h.SpaceIsEstimated === 'boolean'
    && Array.isArray(h.Tables) && h.Tables.length <= 32 && h.Tables.every((table) => record(table) && typeof table.Name === 'string' && integer(table.Rows) && space(table.DataBytes) && space(table.IndexBytes) && space(table.AllocatedBytes))
    && (c.LastRunAt === null || date(c.LastRunAt)) && ['Deleted', 'Compacted', 'Purged', 'Deferred', 'FailedRuns'].every((key) => integer(c[key]));
}
