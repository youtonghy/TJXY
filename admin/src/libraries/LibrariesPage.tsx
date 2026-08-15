import { Button, Skeleton, Table, Tooltip } from '@heroui/react';
import { FolderKanban, Pencil, Plus, RefreshCw, TriangleAlert } from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useCallback, useRef, useState, type ReactNode } from 'react';
import { Link } from 'react-router-dom';

import { AsyncContent } from '../ui/AsyncContent';
import { PageHeader } from '../ui/PageHeader';
import { ResponsiveCollection } from '../ui/ResponsiveCollection';
import { StatusChip } from '../ui/StatusChip';
import { useAuthoritativeLoad } from '../ui/useAuthoritativeLoad';
import { useTranslate } from '../settings/i18n';
import { LibraryCreateDialog } from './LibraryCreateDialog';
import type { CreateLibraryRequest, LibraryOption } from './libraryApi';
import { createLibrary, listLibraries } from './libraryApi';
import {
  collectionLabel,
  expansionPolicyOptions,
  metadataPolicyOptions,
  objectScopeOptions,
  optionLabel,
  probePolicyOptions,
} from './libraryUi';

type LibraryLoadResult = { records: LibraryOption[] } | { error: unknown };

const editLinkClassName = [
  'inline-flex min-h-9 items-center gap-2 rounded-md px-3 text-sm font-semibold',
  'text-accent hover:bg-accent/10',
  'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus',
].join(' ');

export function LibrariesPage() {
  const tr = useTranslate();
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [libraries, setLibraries] = useState<LibraryOption[]>([]);
  const [hasLoaded, setHasLoaded] = useState(false);
  const [loadError, setLoadError] = useState<NonNullable<unknown> | null>(null);
  const [authRedirecting, setAuthRedirecting] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  const [createPending, setCreatePending] = useState(false);
  const createRef = useRef(false);

  const prepareLoadResult = useCallback(async (result: LibraryLoadResult) => {
    if ('records' in result) {
      return () => {
        setLibraries(result.records);
        if (result.records.some((library) => library.enabled && (library.unavailableLocations?.length ?? 0) > 0)) {
          notify(tr('Some library storage roots are offline. Restore the volumes and restart TJXY.', '部分媒体库存储根目录离线。请恢复对应存储卷并重启 TJXY。'), {
            type: 'warning',
            autoHideDuration: 8000,
          });
        }
        setHasLoaded(true);
        setLoadError(null);
        setAuthRedirecting(false);
      };
    }
    if (await logoutIfAccessDenied(result.error)) {
      return () => { setAuthRedirecting(true); };
    }
    return () => { setLoadError(result.error ?? new Error('Library loading failed.')); };
  }, [logoutIfAccessDenied, notify, tr]);

  const { isMounted, loading, reload } = useAuthoritativeLoad(fetchLibraries, prepareLoadResult);

  const create = async (request: CreateLibraryRequest): Promise<boolean> => {
    if (createRef.current) return false;
    createRef.current = true;
    setCreatePending(true);
    try {
      await createLibrary(request);
      if (!isMounted()) return false;
      notify(tr('Library created.', '媒体库已创建。'), { type: 'success' });
      setCreateOpen(false);
      await reload();
      return true;
    } catch (error: unknown) {
      if (!isMounted()) return false;
      if (await logoutIfAccessDenied(error)) return false;
      if (isMounted()) notify(tr('The library could not be created.', '无法创建媒体库。'), { type: 'error' });
      return false;
    } finally {
      createRef.current = false;
      if (isMounted()) setCreatePending(false);
    }
  };

  if (authRedirecting) return null;

  const controlsLocked = loading || createPending;
  return (
    <div className="space-y-5">
      <PageHeader
        actions={(
          <>
            <Tooltip>
              <Button
                aria-label={tr('Reload libraries', '重新加载媒体库')}
                isDisabled={createPending}
                isIconOnly
                isPending={loading}
                onPress={() => { void reload(); }}
                size="sm"
                variant="ghost"
              >
                <RefreshCw aria-hidden="true" className={`size-4${loading ? ' animate-spin' : ''}`} />
              </Button>
              <Tooltip.Content>{tr('Reload libraries', '重新加载媒体库')}</Tooltip.Content>
            </Tooltip>
            <Button isDisabled={controlsLocked} onPress={() => { setCreateOpen(true); }} size="sm">
              <Plus aria-hidden="true" className="size-4" />
              {tr('Add library', '添加媒体库')}
            </Button>
          </>
        )}
        description={tr('Configure catalog sources and scanning behavior.', '配置媒体库来源和扫描行为。')}
        title={tr('Libraries', '媒体库')}
      />

      {loading && hasLoaded && (
        <p aria-live="polite" className="text-sm text-muted" role="status">{tr('Refreshing libraries...', '正在刷新媒体库…')}</p>
      )}

      <AsyncContent
        empty={<LibrariesEmptyState />}
        error={loadError}
        hasData={hasLoaded}
        isEmpty={hasLoaded && libraries.length === 0}
        isPending={loading}
        loading={<LibrariesSkeleton />}
        onRetry={() => { void reload(); }}
      >
        <ResponsiveCollection
          ariaLabel="Libraries collection"
          desktop={<LibrariesTable libraries={libraries} />}
          mobile={<LibrariesMobileList libraries={libraries} />}
        />
      </AsyncContent>

      <LibraryCreateDialog
        isOpen={createOpen}
        isPending={createPending}
        onClose={() => { setCreateOpen(false); }}
        onCreate={create}
      />
    </div>
  );
}

function LibrariesTable({ libraries }: { libraries: LibraryOption[] }) {
  const tr = useTranslate();
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label={tr('Libraries', '媒体库')} className="min-w-[48rem] table-fixed">
          <Table.Header>
            <Table.Column isRowHeader>{tr('Name', '名称')}</Table.Column>
            <Table.Column>{tr('Type', '类型')}</Table.Column>
            <Table.Column>{tr('Status', '状态')}</Table.Column>
            <Table.Column>{tr('Scan profile', '扫描配置')}</Table.Column>
            <Table.Column>{tr('Effective policy', '生效策略')}</Table.Column>
            <Table.Column className="w-20 text-right">{tr('Roots', '根目录')}</Table.Column>
            <Table.Column className="w-32 text-right">{tr('Actions', '操作')}</Table.Column>
          </Table.Header>
          <Table.Body>
            {libraries.map((library) => (
              <Table.Row id={library.id} key={library.id}>
                <Table.Cell><LibraryName library={library} /></Table.Cell>
                <Table.Cell>{tr(collectionLabel(library.collectionType))}</Table.Cell>
                <Table.Cell><LibraryStatus library={library} /></Table.Cell>
                <Table.Cell>{library.scanProfile}</Table.Cell>
                <Table.Cell><PolicySummary library={library} /></Table.Cell>
                <Table.Cell><span className="block text-right tabular-nums">{library.locations.length}</span></Table.Cell>
                <Table.Cell><EditLibraryLink library={library} /></Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}

function LibrariesMobileList({ libraries }: { libraries: LibraryOption[] }) {
  const tr = useTranslate();
  return (
    <ul aria-label={tr('Libraries mobile', '媒体库移动端列表')} className="divide-y divide-border border-y border-border">
      {libraries.map((library) => (
        <li aria-label={library.name} className="space-y-4 py-5" key={library.id}>
          <LibraryName library={library} />
          <dl className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-3 text-sm">
            <MobileField label={tr('Type', '类型')}>{tr(collectionLabel(library.collectionType))}</MobileField>
            <MobileField label={tr('Status', '状态')}><LibraryStatus library={library} /></MobileField>
            <MobileField label={tr('Scan profile', '扫描配置')}>{library.scanProfile}</MobileField>
            <MobileField label={tr('Policy', '策略')}><PolicySummary library={library} /></MobileField>
            <MobileField label={tr('Storage roots', '存储根目录')}>{library.locations.length}</MobileField>
          </dl>
          <div className="flex justify-end"><EditLibraryLink library={library} /></div>
        </li>
      ))}
    </ul>
  );
}

function LibraryName({ library }: { library: LibraryOption }) {
  return (
    <div className="min-w-0">
      <p className="break-words font-semibold text-foreground">{library.name}</p>
      <p className="mt-1 break-all font-mono text-xs text-muted">{library.id}</p>
    </div>
  );
}

function LibraryStatus({ library }: { library: LibraryOption }) {
  const tr = useTranslate();
  if (!library.enabled) {
    return <StatusChip tone="neutral">{tr('Disabled', '已禁用')}</StatusChip>;
  }
  const offline = (library.unavailableLocations?.length ?? 0) > 0;
  if (offline) {
    return <StatusChip tone="warning"><span className="inline-flex items-center gap-1"><TriangleAlert aria-hidden="true" className="size-3.5" />{tr('Offline', '离线')}</span></StatusChip>;
  }
  return <StatusChip tone="success">{tr('Enabled', '已启用')}</StatusChip>;
}

function PolicySummary({ library }: { library: LibraryOption }) {
  const tr = useTranslate();
  return (
    <span className="break-words text-sm text-muted">
      {tr(optionLabel(objectScopeOptions, library.objectSelectionScope))} /{' '}
      {tr(optionLabel(metadataPolicyOptions, library.metadataPolicy))} /{' '}
      {tr(optionLabel(expansionPolicyOptions, library.expansionPolicy))} /{' '}
      {tr(optionLabel(probePolicyOptions, library.probePolicy))}
    </span>
  );
}

function EditLibraryLink({ library }: { library: LibraryOption }) {
  const tr = useTranslate();
  return (
    <div className="flex justify-end">
      <Link
        aria-label={`${tr('Edit', '编辑')} ${library.name}`}
        className={editLinkClassName}
        to={`/admin/libraries/${encodeURIComponent(library.id)}`}
      >
        <Pencil aria-hidden="true" className="size-4" />
        {tr('Edit', '编辑')}
      </Link>
    </div>
  );
}

function MobileField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="font-medium text-muted">{label}</dt>
      <dd className="min-w-0 text-foreground">{children}</dd>
    </>
  );
}

function LibrariesSkeleton() {
  const tr = useTranslate();
  return (
    <div aria-label={tr('Loading libraries', '正在加载媒体库')} className="space-y-3" role="status">
      <Skeleton className="h-11 w-full" />
      <Skeleton className="h-20 w-full" />
      <Skeleton className="h-20 w-full" />
    </div>
  );
}

function LibrariesEmptyState() {
  const tr = useTranslate();
  return (
    <div className="flex min-h-52 flex-col items-center justify-center gap-3 border-y border-border py-8 text-center">
      <FolderKanban aria-hidden="true" className="size-6 text-muted" />
      <div>
        <h2 className="text-base font-semibold text-foreground">{tr('No libraries are configured.', '尚未配置媒体库。')}</h2>
        <p className="mt-1 text-sm text-muted">{tr('Add a library to begin organizing catalog content.', '添加媒体库后即可开始整理媒体内容。')}</p>
      </div>
    </div>
  );
}

async function fetchLibraries(signal: AbortSignal): Promise<LibraryLoadResult> {
  try {
    return { records: await listLibraries(signal) };
  } catch (error: unknown) {
    return { error };
  }
}
