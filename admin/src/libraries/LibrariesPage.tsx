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
          notify('Some library storage roots are offline. Restore the volumes and restart TJXY.', {
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
  }, [logoutIfAccessDenied, notify]);

  const { isMounted, loading, reload } = useAuthoritativeLoad(fetchLibraries, prepareLoadResult);

  const create = async (request: CreateLibraryRequest): Promise<boolean> => {
    if (createRef.current) return false;
    createRef.current = true;
    setCreatePending(true);
    try {
      await createLibrary(request);
      if (!isMounted()) return false;
      notify('Library created.', { type: 'success' });
      setCreateOpen(false);
      await reload();
      return true;
    } catch (error: unknown) {
      if (!isMounted()) return false;
      if (await logoutIfAccessDenied(error)) return false;
      if (isMounted()) notify('The library could not be created.', { type: 'error' });
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
                aria-label="Reload libraries"
                isDisabled={createPending}
                isIconOnly
                isPending={loading}
                onPress={() => { void reload(); }}
                size="sm"
                variant="ghost"
              >
                <RefreshCw aria-hidden="true" className={`size-4${loading ? ' animate-spin' : ''}`} />
              </Button>
              <Tooltip.Content>Reload libraries</Tooltip.Content>
            </Tooltip>
            <Button isDisabled={controlsLocked} onPress={() => { setCreateOpen(true); }} size="sm">
              <Plus aria-hidden="true" className="size-4" />
              Add library
            </Button>
          </>
        )}
        description="Configure catalog sources and scanning behavior."
        title="Libraries"
      />

      {loading && hasLoaded && (
        <p aria-live="polite" className="text-sm text-muted" role="status">Refreshing libraries...</p>
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
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label="Libraries" className="min-w-[48rem] table-fixed">
          <Table.Header>
            <Table.Column isRowHeader>Name</Table.Column>
            <Table.Column>Type</Table.Column>
            <Table.Column>Status</Table.Column>
            <Table.Column>Scan profile</Table.Column>
            <Table.Column>Effective policy</Table.Column>
            <Table.Column className="w-20 text-right">Roots</Table.Column>
            <Table.Column className="w-32 text-right">Actions</Table.Column>
          </Table.Header>
          <Table.Body>
            {libraries.map((library) => (
              <Table.Row id={library.id} key={library.id}>
                <Table.Cell><LibraryName library={library} /></Table.Cell>
                <Table.Cell>{collectionLabel(library.collectionType)}</Table.Cell>
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
  return (
    <ul aria-label="Libraries mobile" className="divide-y divide-border border-y border-border">
      {libraries.map((library) => (
        <li aria-label={library.name} className="space-y-4 py-5" key={library.id}>
          <LibraryName library={library} />
          <dl className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-3 text-sm">
            <MobileField label="Type">{collectionLabel(library.collectionType)}</MobileField>
            <MobileField label="Status"><LibraryStatus library={library} /></MobileField>
            <MobileField label="Scan profile">{library.scanProfile}</MobileField>
            <MobileField label="Policy"><PolicySummary library={library} /></MobileField>
            <MobileField label="Storage roots">{library.locations.length}</MobileField>
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
  if (!library.enabled) {
    return <StatusChip tone="neutral">Disabled</StatusChip>;
  }
  const offline = (library.unavailableLocations?.length ?? 0) > 0;
  if (offline) {
    return <StatusChip tone="warning"><span className="inline-flex items-center gap-1"><TriangleAlert aria-hidden="true" className="size-3.5" />Offline</span></StatusChip>;
  }
  return <StatusChip tone="success">Enabled</StatusChip>;
}

function PolicySummary({ library }: { library: LibraryOption }) {
  return (
    <span className="break-words text-sm text-muted">
      {optionLabel(objectScopeOptions, library.objectSelectionScope)} /{' '}
      {optionLabel(metadataPolicyOptions, library.metadataPolicy)} /{' '}
      {optionLabel(expansionPolicyOptions, library.expansionPolicy)} /{' '}
      {optionLabel(probePolicyOptions, library.probePolicy)}
    </span>
  );
}

function EditLibraryLink({ library }: { library: LibraryOption }) {
  return (
    <div className="flex justify-end">
      <Link
        aria-label={`Edit ${library.name}`}
        className={editLinkClassName}
        to={`/admin/libraries/${encodeURIComponent(library.id)}`}
      >
        <Pencil aria-hidden="true" className="size-4" />
        Edit
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
  return (
    <div aria-label="Loading libraries" className="space-y-3" role="status">
      <Skeleton className="h-11 w-full" />
      <Skeleton className="h-20 w-full" />
      <Skeleton className="h-20 w-full" />
    </div>
  );
}

function LibrariesEmptyState() {
  return (
    <div className="flex min-h-52 flex-col items-center justify-center gap-3 border-y border-border py-8 text-center">
      <FolderKanban aria-hidden="true" className="size-6 text-muted" />
      <div>
        <h2 className="text-base font-semibold text-foreground">No libraries are configured.</h2>
        <p className="mt-1 text-sm text-muted">Add a library to begin organizing catalog content.</p>
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
