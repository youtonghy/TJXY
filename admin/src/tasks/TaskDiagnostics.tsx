import { Button, Card } from '@heroui/react';
import { useLogoutIfAccessDenied } from 'ra-core';
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../api/httpClient';
import { useTranslate } from '../settings/i18n';
import type { TaskJob } from './taskApi';
import { chooseNfo, getScanReport, listNfoChoices, listScanHistory, type ScanHistoryEntry, retryScanIssues, type NfoChoice, type ScanReport } from './taskDiagnosticsApi';

export function TaskDiagnostics({ jobs }: { jobs: TaskJob[] }) {
  const tr = useTranslate();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [choices, setChoices] = useState<NfoChoice[] | null>(null);
  const [choiceOffset, setChoiceOffset] = useState(0);
  const [report, setReport] = useState<ScanReport | null>(null);
  const [scanHistory, setScanHistory] = useState<ScanHistoryEntry[] | null>(null);
  const [historyOffset, setHistoryOffset] = useState(0);
  const [scan, setScan] = useState('');
  const [reportOffset, setReportOffset] = useState(0);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const operation = useRef(false);
  const mounted = useRef(true);
  const controller = useRef<AbortController | null>(null);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; controller.current?.abort(); };
  }, []);

  const run = async (action: (signal: AbortSignal) => Promise<void>) => {
    if (operation.current) return;
    operation.current = true;
    controller.current = new AbortController();
    setBusy(true); setError(''); setMessage('');
    try { await action(controller.current.signal); }
    catch (failure: unknown) {
      if (!mounted.current || await logoutIfAccessDenied(failure)) return;
      setError(failure instanceof ApiError && failure.status === 409
        ? tr('The source changed. Reload the candidates before choosing again.', '来源已变化，请刷新候选后重新选择。')
        : tr('Diagnostics could not be updated. Reload and try again.', '无法更新诊断，请刷新后重试。'));
    } finally {
      operation.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  const loadChoices = (offset: number) => run(async (signal) => {
    const result = await listNfoChoices(offset, signal);
    if (mounted.current) { setChoices(result); setChoiceOffset(offset); }
  });
  const loadReport = (id: string, offset: number) => run(async (signal) => {
    const result = await getScanReport(id, offset, signal);
    if (mounted.current) { setReport(result); setReportOffset(offset); setScan(id); }
  });
  const scans = scanHistory?.map((job) => ({ id: job.Id, createdAt: job.CreatedAt, status: job.State })) ?? jobs.filter((job) => job.taskKind === 'FullMediaScan' || job.taskKind === 'FullLibraryRootScan');
  const loadHistory = (offset: number) => run(async (signal) => {
    const page = await listScanHistory(offset, signal);
    if (mounted.current) { setScanHistory(page); setHistoryOffset(offset); setScan(page[0]?.Id ?? ''); setReport(null); }
  });
  const selectedScan = scan || (scans[0]?.id ?? '');

  return <Card variant="secondary" className="gap-4 p-5 sm:p-6">
    <Card.Header>
      <Card.Title render={(props) => <h2 {...props} />}>{tr('Scan results and NFO choices', '扫描结果与 NFO 选择')}</Card.Title>
      <Card.Description>{tr('Resolve source conflicts and retry affected items without rescanning the library.', '处理来源冲突并重试受影响项目，无需重新扫描整个媒体库。')}</Card.Description>
    </Card.Header>
    <Card.Content className="space-y-5">
      <div className="flex flex-wrap items-end gap-3">
        <Button variant="secondary" isDisabled={busy} onPress={() => { void loadChoices(0); }}>{tr('Load NFO choices', '加载 NFO 候选')}</Button>
        <Button variant="secondary" isDisabled={busy} onPress={() => { void loadHistory(0); }}>{tr('Load scan history', '加载扫描历史')}</Button>
        {scanHistory !== null && <>
          <Button size="sm" variant="ghost" isDisabled={busy || historyOffset === 0} onPress={() => { void loadHistory(Math.max(0, historyOffset - 50)); }}>{tr('Newer scans', '较新的扫描')}</Button>
          <Button size="sm" variant="ghost" isDisabled={busy || scanHistory.length < 50} onPress={() => { void loadHistory(historyOffset + 50); }}>{tr('Older scans', '更早的扫描')}</Button>
        </>}
        {scans.length > 0 && <>
          <label className="flex min-w-0 flex-col gap-1 text-sm">
            {tr('Scan report', '扫描报告')}
            <select className="max-w-full rounded-lg border border-border bg-field px-3 py-2" value={selectedScan} disabled={busy}
              onChange={(event) => { setScan(event.target.value); setReport(null); setReportOffset(0); }}>
              {scans.map((job) => <option key={job.id} value={job.id}>{job.createdAt ?? job.id} · {job.status}</option>)}
            </select>
          </label>
          <Button variant="secondary" isDisabled={busy} onPress={() => { void loadReport(selectedScan, 0); }}>{tr('Load report', '加载报告')}</Button>
        </>}
      </div>
      {error && <p role="alert" className="text-sm text-danger">{error}</p>}
      {message && <p role="status" className="text-sm">{message}</p>}
      {choices !== null && <section aria-label={tr('NFO choices', 'NFO 候选')} className="space-y-3">
        {choices.length === 0 && <p className="text-sm text-muted">{tr('No NFO choices on this page.', '本页没有待处理的 NFO 候选。')}</p>}
        {choices.map((choice) => <div className="space-y-2 rounded-lg border border-border p-3" key={`${choice.ItemId}:${choice.RootId}`}>
          <p className="break-all text-sm">{tr('Item', '项目')} {choice.ItemId}</p>
          <p className="text-sm text-muted">{tr('Conflicting fields', '冲突字段')}: {choice.ConflictFields.join(', ')}</p>
          {choice.Status === 'Selected' && <p role="status" className="text-sm">{tr('Choice saved; metadata work is queued or running.', '选择已保存，元数据任务正在等待或执行。')}</p>}
          <div className="flex flex-wrap gap-2">{choice.Candidates.map((candidate) => <Button key={candidate.Id} size="sm" variant="outline"
            isDisabled={busy || candidate.Digest.length !== 64}
            aria-label={`${tr('Choose', '选择')} ${candidate.Name}`}
            onPress={() => { void run(async (signal) => {
              await chooseNfo(choice, candidate.Id);
              const updated = await listNfoChoices(choiceOffset, signal);
              if (mounted.current) { setChoices(updated); setMessage(tr('Choice saved and metadata parsing submitted.', '选择已保存，元数据解析已提交。')); }
            }); }}>
            <span className="max-w-64 truncate">{candidate.Name}</span>
          </Button>)}</div>
        </div>)}
        <div className="flex gap-2">
          <Button size="sm" variant="ghost" isDisabled={busy || choiceOffset === 0} onPress={() => { void loadChoices(Math.max(0, choiceOffset - 50)); }}>{tr('Previous choices', '上一页候选')}</Button>
          <Button size="sm" variant="ghost" isDisabled={busy || choices.length < 50} onPress={() => { void loadChoices(choiceOffset + 50); }}>{tr('Next choices', '下一页候选')}</Button>
        </div>
      </section>}
      {report !== null && <section aria-label={tr('Scan results', '扫描结果')} className="space-y-3">
        {report.Counters === null ? <p className="text-sm text-muted">{tr('Detailed counts are unavailable for this older or unfinished scan.', '此旧版或尚未完成的扫描没有详细计数。')}</p>
          : <p className="text-sm">{tr('Succeeded', '成功')} {report.Counters.success} · {tr('Failed', '失败')} {report.Counters.failed} · {tr('Skipped', '跳过')} {report.Counters.skipped} · {tr('Needs selection', '待选择')} {report.Counters.needs_selection}</p>}
        <ul className="space-y-2 text-sm">{report.Issues.map((issue) => <li key={issue.ItemId} className="break-all">
          {issue.ItemId} · {issue.TaskKind} · {issue.NeedsSelection ? tr('Needs NFO selection', '需要选择 NFO') : tr('Failed', '失败')}
        </li>)}</ul>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="secondary" isDisabled={busy || !report.Issues.some((issue) => !issue.NeedsSelection && issue.TaskKind === 'ResolveMetadata')}
            onPress={() => { void run(async () => {
              const count = await retryScanIssues(scan, reportOffset);
              if (mounted.current) setMessage(tr(`${String(count)} failed items submitted.`, `已提交 ${String(count)} 个失败项目。`));
            }); }}>{tr('Retry failed items on this page', '重试本页失败项目')}</Button>
          <Button size="sm" variant="ghost" isDisabled={busy || reportOffset === 0} onPress={() => { void loadReport(scan, Math.max(0, reportOffset - 50)); }}>{tr('Previous results', '上一页结果')}</Button>
          <Button size="sm" variant="ghost" isDisabled={busy || !report.HasMore} onPress={() => { void loadReport(scan, reportOffset + 50); }}>{tr('Next results', '下一页结果')}</Button>
        </div>
      </section>}
    </Card.Content>
  </Card>;
}
