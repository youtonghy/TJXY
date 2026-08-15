/* eslint-disable react-hooks/set-state-in-effect */
import { Alert, Button, Card, Input, Label, NumberField, ScrollShadow, Spinner, TextField, Tooltip } from '@heroui/react';
import { Segment } from '@heroui-pro/react/segment';
import { Download, FileClock, RefreshCw, Save } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNotify } from 'ra-core';

import { PageHeader } from '../ui/PageHeader';
import { useTranslate } from '../settings/i18n';
import { downloadLogFile, getLoggingSettings, getLogPage, listLogFiles, saveLoggingSettings, type LogFile, type LoggingSettings } from './logsApi';

const initialSettings: LoggingSettings = { mode: 'Error', retentionDays: 30, revision: 0, directory: '' };

export function LogsPage() {
  const tr = useTranslate();
  const notify = useNotify();
  const [settings, setSettings] = useState(initialSettings);
  const [files, setFiles] = useState<LogFile[]>([]);
  const [selectedDate, setSelectedDate] = useState('');
  const [lines, setLines] = useState<string[]>([]);
  const [hasOlder, setHasOlder] = useState(false);
  const [offset, setOffset] = useState(0);
  const [query, setQuery] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [loadingLog, setLoadingLog] = useState(false);
  const [error, setError] = useState(false);

  const loadPage = useCallback(async (date: string, before?: number) => {
    if (!date) { setLines([]); return; }
    setLoadingLog(true);
    try {
      const page = await getLogPage(date, before);
      setLines((current) => before === undefined ? page.lines : [...page.lines, ...current]);
      setOffset(page.offset);
      setHasOlder(page.hasOlder);
    } catch {
      notify(tr('admin.logs.loadFailed'), { type: 'error' });
    } finally { setLoadingLog(false); }
  }, [notify, tr]);

  const load = useCallback(async () => {
    setLoading(true); setError(false);
    try {
      const [loadedSettings, loadedFiles] = await Promise.all([getLoggingSettings(), listLogFiles()]);
      setSettings(loadedSettings); setFiles(loadedFiles);
      const date = loadedFiles[0]?.date ?? '';
      setSelectedDate(date);
      await loadPage(date);
    } catch { setError(true); }
    finally { setLoading(false); }
  }, [loadPage]);

  useEffect(() => { void load(); }, [load]);

  const save = async () => {
    setSaving(true);
    try {
      const saved = await saveLoggingSettings(settings);
      setSettings(saved);
      notify(tr('admin.logs.saved'), { type: 'success' });
    } catch { notify(tr('admin.logs.saveFailed'), { type: 'error' }); }
    finally { setSaving(false); }
  };

  const visibleLines = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return normalized ? lines.filter((line) => line.toLocaleLowerCase().includes(normalized)) : lines;
  }, [lines, query]);

  return (
    <div className="space-y-6">
      <PageHeader
        actions={<Tooltip><Button aria-label={tr('admin.logs.reload')} isIconOnly isPending={loading} onPress={() => { void load(); }} size="sm" variant="ghost"><RefreshCw className="size-4" /></Button><Tooltip.Content>{tr('admin.logs.reload')}</Tooltip.Content></Tooltip>}
        description={tr('admin.logs.subtitle')}
        title={tr('admin.logs.title')}
      />
      {error && <Alert status="danger"><Alert.Content><Alert.Title>{tr('admin.logs.loadFailed')}</Alert.Title><Alert.Description>{tr('admin.logs.loadFailedDescription')}</Alert.Description></Alert.Content></Alert>}

      <section aria-labelledby="logging-settings-title" className="space-y-4">
        <div><h2 className="text-base font-semibold" id="logging-settings-title">{tr('admin.logs.settings')}</h2><p className="text-sm text-muted">{tr('admin.logs.settingsDescription')}</p></div>
        <div className="grid gap-5 md:grid-cols-[minmax(0,1fr)_220px_auto] md:items-end">
          <div className="space-y-2"><Label>{tr('admin.logs.mode')}</Label><Segment isDisabled={loading || saving} onSelectionChange={(key) => { if (key === 'Error' || key === 'Debug') setSettings((current) => ({ ...current, mode: key })); }} selectedKey={settings.mode}><Segment.Item id="Error">Error</Segment.Item><Segment.Item id="Debug">Debug</Segment.Item></Segment><p className="text-xs text-muted">{settings.mode === 'Error' ? tr('admin.logs.errorDescription') : tr('admin.logs.debugDescription')}</p></div>
          <NumberField isDisabled={loading || saving} maxValue={365} minValue={1} onChange={(value) => { setSettings((current) => ({ ...current, retentionDays: Number.isFinite(value) ? value : 30 })); }} value={settings.retentionDays}>
            <Label>{tr('admin.logs.retention')}</Label><NumberField.Group><NumberField.DecrementButton /><NumberField.Input /><NumberField.IncrementButton /></NumberField.Group>
          </NumberField>
          <Button isDisabled={loading} isPending={saving} onPress={() => { void save(); }}><Save className="size-4" />{tr('admin.logs.save')}</Button>
        </div>
        <p className="break-all text-xs text-muted">{settings.directory}</p>
      </section>

      <section aria-labelledby="daily-logs-title" className="space-y-4">
        <div><h2 className="text-base font-semibold" id="daily-logs-title">{tr('admin.logs.daily')}</h2><p className="text-sm text-muted">{tr('admin.logs.dailyDescription')}</p></div>
        <div className="grid min-h-[480px] gap-4 lg:grid-cols-[220px_minmax(0,1fr)]">
          <div className="space-y-1 border-b border-border pb-3 lg:border-r lg:border-b-0 lg:pr-3">
            {files.map((file) => <Button className="w-full justify-between" key={file.date} onPress={() => { setSelectedDate(file.date); void loadPage(file.date); }} variant={file.date === selectedDate ? 'secondary' : 'ghost'}><span>{file.date}</span><span className="text-xs text-muted">{formatBytes(file.sizeBytes)}</span></Button>)}
            {!loading && files.length === 0 && <p className="py-8 text-center text-sm text-muted">{tr('admin.logs.empty')}</p>}
          </div>
          <div className="min-w-0 space-y-3">
            <div className="flex flex-wrap items-end gap-2">
              <TextField className="min-w-[220px] flex-1"><Label>{tr('admin.logs.search')}</Label><Input value={query} onChange={(event) => { setQuery(event.currentTarget.value); }} /></TextField>
              <Tooltip><Button aria-label={tr('admin.logs.refreshFile')} isDisabled={!selectedDate} isIconOnly isPending={loadingLog} onPress={() => { void loadPage(selectedDate); }} variant="ghost"><RefreshCw className="size-4" /></Button><Tooltip.Content>{tr('admin.logs.refreshFile')}</Tooltip.Content></Tooltip>
              <Tooltip><Button aria-label={tr('admin.logs.download')} isDisabled={!selectedDate} isIconOnly onPress={() => { void downloadLogFile(selectedDate).catch(() => { notify(tr('admin.logs.downloadFailed'), { type: 'error' }); }); }} variant="ghost"><Download className="size-4" /></Button><Tooltip.Content>{tr('admin.logs.download')}</Tooltip.Content></Tooltip>
            </div>
            <Card className="h-[420px] overflow-hidden">
              <Card.Content className="h-full p-0">
                <ScrollShadow className="h-full overflow-auto p-4">
                  {hasOlder && <Button className="mb-4" isPending={loadingLog} onPress={() => { void loadPage(selectedDate, offset); }} size="sm" variant="secondary">{tr('admin.logs.loadOlder')}</Button>}
                  {visibleLines.length > 0 ? <div className="space-y-1 font-mono text-xs leading-5">{visibleLines.map((line, index) => <LogLine key={`${String(index)}-${line.slice(0, 24)}`} line={line} />)}</div> : <div className="flex h-72 flex-col items-center justify-center gap-2 text-muted"><FileClock className="size-6" /><p className="text-sm">{tr('admin.logs.noMatchingLines')}</p></div>}
                </ScrollShadow>
              </Card.Content>
            </Card>
          </div>
        </div>
      </section>
      {loading && <Spinner aria-label={tr('admin.logs.loading')} />}
    </div>
  );
}

function LogLine({ line }: { line: string }) {
  let level = '';
  try { const parsed = JSON.parse(line) as { level?: unknown }; level = typeof parsed.level === 'string' ? parsed.level : ''; } catch { /* Preserve malformed lines verbatim. */ }
  const tone = level === 'ERROR' ? 'text-danger' : level === 'WARN' ? 'text-warning' : level === 'DEBUG' ? 'text-muted' : 'text-foreground';
  return <div className={`break-all whitespace-pre-wrap ${tone}`}>{line}</div>;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${String(bytes)} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
