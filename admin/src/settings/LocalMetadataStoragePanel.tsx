import { Alert, Button, Card, Input, Label, Skeleton, TextField, Tooltip } from '@heroui/react';
import { Database, RefreshCw, Save, Trash2, TriangleAlert } from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useCallback, useRef, useState } from 'react';

import { ConfirmDialog } from '../ui/ConfirmDialog';
import { useAuthoritativeLoad } from '../ui/useAuthoritativeLoad';
import { useSystemLocale } from './SystemLocaleProvider';
import { interpolate, useTranslate } from './i18n';
import { cleanupLocalMetadata, getLocalMetadataStorage, saveLocalMetadataLocation, type LocalMetadataMetric, type LocalMetadataStorage } from './metadataSettingsApi';

type LoadResult = { storage: LocalMetadataStorage } | { error: unknown };
type Operation = 'save' | 'cleanup' | null;

export function LocalMetadataStoragePanel() {
  const t = useTranslate();
  const { locale } = useSystemLocale();
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [storage, setStorage] = useState<LocalMetadataStorage | null>(null);
  const [path, setPath] = useState('');
  const [error, setError] = useState<unknown>(null);
  const [operation, setOperation] = useState<Operation>(null);
  const operationRef = useRef<Operation>(null);
  const fetchStorage = useCallback(async (signal: AbortSignal): Promise<LoadResult> => {
    try { return { storage: await getLocalMetadataStorage(signal) }; } catch (loadError: unknown) { return { error: loadError }; }
  }, []);
  const prepare = useCallback((result: LoadResult) => Promise.resolve(() => {
    if ('error' in result) { setError(result.error); return; }
    setStorage(result.storage); setPath(result.storage.pendingPath ?? result.storage.currentPath); setError(null);
  }), []);
  const { isMounted, loading, reload } = useAuthoritativeLoad(fetchStorage, prepare);

  const run = async (next: Exclude<Operation, null>, callback: () => Promise<void>) => {
    if (operationRef.current !== null) return;
    operationRef.current = next; setOperation(next);
    try { await callback(); } finally { operationRef.current = null; if (isMounted()) setOperation(null); }
  };
  const save = async () => run('save', async () => {
    try { const next = await saveLocalMetadataLocation(path); if (!isMounted()) return; setStorage(next); setPath(next.pendingPath ?? next.currentPath); notify(t('admin.metadata.localSaved'), { type: 'success' }); }
    catch (saveError: unknown) { if (!isMounted() || await logoutIfAccessDenied(saveError)) return; notify(t('admin.metadata.localSaveFailed'), { type: 'error' }); }
  });
  const cleanup = async () => run('cleanup', async () => {
    try { const result = await cleanupLocalMetadata(); if (!isMounted()) return; setStorage(result.storage); notify(interpolate(t('admin.metadata.localCleanupSuccess'), { count: result.deleted.count.toLocaleString(locale) }), { type: result.failedCount === 0 ? 'success' : 'warning' }); }
    catch (cleanupError: unknown) { if (!isMounted() || await logoutIfAccessDenied(cleanupError)) return; throw cleanupError; }
  });

  if (loading && storage === null) return <Card><Card.Content><Skeleton className="h-40 w-full" /></Card.Content></Card>;
  if (storage === null) return <Alert status="danger"><Alert.Content><Alert.Title>{t('admin.metadata.localLoadFailed')}</Alert.Title><Alert.Description>{t('admin.metadata.localLoadFailedDescription')}</Alert.Description></Alert.Content><Button onPress={() => { void reload(); }} size="sm" variant="tertiary">{t('admin.metadata.retryRefresh')}</Button></Alert>;
  const metrics: [string, LocalMetadataMetric][] = [['total', storage.statistics.total], ['linked', storage.statistics.linked], ['orphaned', storage.statistics.orphaned], ['missing', storage.statistics.missing], ['unregistered', storage.statistics.unregistered]];
  const cleanableCount = storage.statistics.orphaned.count + storage.statistics.unregistered.count;
  const cleanableBytes = storage.statistics.orphaned.bytes + storage.statistics.unregistered.bytes;
  return (
    <Card>
      <Card.Header className="flex items-start justify-between gap-4">
        <div><Card.Title className="flex items-center gap-2"><Database className="size-4" />{t('admin.metadata.localTitle')}</Card.Title><Card.Description>{t('admin.metadata.localDescription')}</Card.Description></div>
        <Tooltip><Button aria-label={t('admin.metadata.localRefresh')} isIconOnly isPending={loading} onPress={() => { void reload(); }} size="sm" variant="ghost"><RefreshCw className={`size-4${loading ? ' animate-spin' : ''}`} /></Button><Tooltip.Content>{t('admin.metadata.localRefresh')}</Tooltip.Content></Tooltip>
      </Card.Header>
      <Card.Content className="space-y-5">
        {error !== null && <Alert status="warning"><Alert.Indicator><TriangleAlert className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{t('admin.metadata.staleTitle')}</Alert.Title><Alert.Description>{t('admin.metadata.localStale')}</Alert.Description></Alert.Content></Alert>}
        {storage.restartRequired && <Alert status="warning"><Alert.Indicator><TriangleAlert className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{t('admin.metadata.localRestart')}</Alert.Title><Alert.Description>{t('admin.metadata.localRestartDescription')}</Alert.Description></Alert.Content></Alert>}
        <dl className="grid gap-3 sm:grid-cols-2 xl:grid-cols-5">{metrics.map(([key, metric]) => <div key={key} className="min-w-0 border-l-2 border-default pl-3"><dt className="text-sm text-muted">{t(`admin.metadata.local.${key}`)}</dt><dd className="text-xl font-semibold tabular-nums">{metric.count.toLocaleString(locale)}</dd><dd className="text-xs text-muted">{formatBytes(metric.bytes, locale)}</dd></div>)}</dl>
        <TextField isDisabled={!storage.locationEditable || operation !== null} onChange={setPath} value={path}><Label>{t('admin.metadata.localLocation')}</Label><Input className="font-mono text-sm" /></TextField>
        <div className="space-y-1 text-xs text-muted"><p className="break-all">{t('admin.metadata.localCurrent')}: {storage.currentPath}</p>{storage.pendingPath && <p className="break-all">{t('admin.metadata.localPending')}: {storage.pendingPath}</p>}{storage.source === 'Environment' && <p>{t('admin.metadata.localEnvironment')}</p>}</div>
      </Card.Content>
      <Card.Footer className="flex flex-wrap justify-between gap-3">
        <ConfirmDialog cancelLabel={t('admin.metadata.localCancel')} closeLabel={t('admin.metadata.localClose')} confirmLabel={t('admin.metadata.localCleanupConfirm')} description={interpolate(t('admin.metadata.localCleanupDescription'), { count: cleanableCount.toLocaleString(locale), bytes: formatBytes(cleanableBytes, locale) })} errorDescription={t('admin.metadata.localCleanupFailed')} errorTitle={t('admin.metadata.localCleanupErrorTitle')} isPending={operation !== null} onConfirm={cleanup} title={t('admin.metadata.localCleanupTitle')} trigger={<Button isDisabled={operation !== null || cleanableCount === 0} variant="danger-soft"><Trash2 className="size-4" />{t('admin.metadata.localCleanup')}</Button>} />
        <Button isDisabled={!storage.locationEditable || path.trim().length === 0 || operation !== null} isPending={operation === 'save'} onPress={() => { void save(); }} variant="primary"><Save className="size-4" />{t('admin.metadata.localSave')}</Button>
      </Card.Footer>
    </Card>
  );
}

function formatBytes(bytes: number, locale: string): string {
  if (bytes < 1024) return `${bytes.toLocaleString(locale)} B`;
  const units = ['KB', 'MB', 'GB', 'TB']; let value = bytes; let unit = -1;
  do { value /= 1024; unit += 1; } while (value >= 1024 && unit < units.length - 1);
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(value)} ${units[unit] ?? 'TB'}`;
}
