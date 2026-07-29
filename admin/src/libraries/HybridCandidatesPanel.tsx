import {
  Alert,
  Button,
  FieldError,
  Input,
  Label,
  Pagination,
  Skeleton,
  Table,
  TextField,
  Tooltip,
} from '@heroui/react';
import { LoaderCircle, Pin, RefreshCw, Trash2, TriangleAlert } from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useCallback, useRef, useState, type ReactNode } from 'react';

import { AsyncContent } from '../ui/AsyncContent';
import { ResponsiveCollection } from '../ui/ResponsiveCollection';
import { useAuthoritativeLoad } from '../ui/useAuthoritativeLoad';
import type { HybridCandidate, HybridCandidatePage } from './hybridCandidateApi';
import {
  listHybridCandidates,
  pinHybridCandidate,
  unpinHybridCandidate,
} from './hybridCandidateApi';
import type { LibraryOption } from './libraryApi';
import { humanizeIdentifier } from './libraryUi';

const PAGE_SIZE = 50;
const EMPTY_PAGE: HybridCandidatePage = { items: [], totalRecordCount: 0, startIndex: 0 };
type CandidateLoadResult = { page: HybridCandidatePage } | { error: unknown };

export function HybridCandidatesPanel({ library }: { library: LibraryOption }) {
  if (!isUuid(library.id)) {
    return <UnavailableCandidates />;
  }
  return <ActiveCandidatesPanel key={library.id} library={library} />;
}

function ActiveCandidatesPanel({ library }: { library: LibraryOption }) {
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [pageIndex, setPageIndex] = useState(0);
  const [page, setPage] = useState<HybridCandidatePage>(EMPTY_PAGE);
  const [hasLoaded, setHasLoaded] = useState(false);
  const [loadError, setLoadError] = useState<NonNullable<unknown> | null>(null);
  const [authRedirecting, setAuthRedirecting] = useState(false);
  const [itemId, setItemId] = useState('');
  const [busyOperation, setBusyOperation] = useState<string | null>(null);
  const operationRef = useRef<string | null>(null);
  const awaitingRefreshRef = useRef(false);

  const finishOperation = useCallback(() => {
    awaitingRefreshRef.current = false;
    operationRef.current = null;
    setBusyOperation(null);
  }, []);

  const fetchPage = useCallback((signal: AbortSignal) => (
    fetchCandidatePage(library.id, pageIndex, signal)
  ), [library.id, pageIndex]);

  const prepareLoadResult = useCallback(async (result: CandidateLoadResult) => {
    if ('page' in result) {
      return () => {
        const lastValidPage = Math.max(0, Math.ceil(result.page.totalRecordCount / PAGE_SIZE) - 1);
        if (pageIndex > lastValidPage) {
          setPage(EMPTY_PAGE);
          setHasLoaded(false);
          setLoadError(null);
          setPageIndex(lastValidPage);
          return;
        }
        setPage(result.page);
        setHasLoaded(true);
        setLoadError(null);
        setAuthRedirecting(false);
        if (awaitingRefreshRef.current) finishOperation();
      };
    }
    if (await logoutIfAccessDenied(result.error)) {
      return () => {
        setAuthRedirecting(true);
        if (awaitingRefreshRef.current) finishOperation();
      };
    }
    return () => {
      setLoadError(result.error ?? new Error('Candidate loading failed.'));
      if (awaitingRefreshRef.current) finishOperation();
    };
  }, [finishOperation, logoutIfAccessDenied, pageIndex]);

  const { isMounted, loading, reload } = useAuthoritativeLoad(fetchPage, prepareLoadResult);

  const runMutation = async (
    operation: string,
    command: () => Promise<void>,
    successMessage: string,
  ): Promise<boolean> => {
    if (operationRef.current !== null) return false;
    operationRef.current = operation;
    setBusyOperation(operation);
    try {
      await command();
      if (!isMounted()) return false;
      notify(successMessage, { type: 'success' });
      awaitingRefreshRef.current = true;
      return true;
    } catch (error: unknown) {
      if (!isMounted()) return false;
      if (await logoutIfAccessDenied(error)) return false;
      if (isMounted()) notify('The background candidate change could not be completed.', { type: 'error' });
      return false;
    } finally {
      if (!awaitingRefreshRef.current) {
        operationRef.current = null;
        if (isMounted()) setBusyOperation(null);
      }
    }
  };

  const pin = async () => {
    const normalizedId = itemId.trim();
    if (!isUuid(normalizedId)) return;
    if (await runMutation(
      'pin',
      () => pinHybridCandidate(library.id, normalizedId),
      'Background candidate pinned.',
    )) {
      setItemId('');
      await reload();
    }
  };

  const unpin = async (candidate: HybridCandidate) => {
    if (await runMutation(
      `unpin-${candidate.id}`,
      () => unpinHybridCandidate(library.id, candidate.id),
      'Background candidate pin removed.',
    )) {
      if (pageIndex > 0 && page.items.length === 1) {
        setPageIndex((current) => Math.max(0, current - 1));
      } else {
        await reload();
      }
    }
  };

  if (authRedirecting) return null;

  const validItemId = isUuid(itemId.trim());
  const canCreatePin = library.enabled && library.expansionPolicy === 'background';
  const controlsLocked = loading || busyOperation !== null;
  const totalPages = Math.max(1, Math.ceil(page.totalRecordCount / PAGE_SIZE));
  const firstRecord = page.totalRecordCount === 0 ? 0 : page.startIndex + 1;
  const lastRecord = Math.min(page.startIndex + page.items.length, page.totalRecordCount);

  return (
    <section aria-labelledby="background-candidates-heading" className="space-y-5 border-t border-border py-7">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <h2 className="text-base font-semibold text-foreground" id="background-candidates-heading">Background candidates</h2>
          <p className="mt-1 text-sm text-muted">Manage future expansion preferences without changing existing catalog records.</p>
        </div>
        <Tooltip>
          <Button
            aria-label="Reload background candidates"
            isDisabled={busyOperation !== null}
            isIconOnly
            isPending={loading}
            onPress={() => { void reload(); }}
            size="sm"
            variant="ghost"
          >
            <RefreshCw aria-hidden="true" className={`size-4${loading ? ' animate-spin' : ''}`} />
          </Button>
          <Tooltip.Content>Reload background candidates</Tooltip.Content>
        </Tooltip>
      </div>

      {!canCreatePin && (
        <Alert status="accent">
          <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
          <Alert.Content>
            <Alert.Title>New pins are paused</Alert.Title>
            <Alert.Description>Enable the library and select Background expansion to add pins. Existing pins can still be removed.</Alert.Description>
          </Alert.Content>
        </Alert>
      )}

      <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
        <TextField fullWidth isInvalid={itemId.length > 0 && !validItemId} name="candidateItemId">
          <Label>Catalog item ID</Label>
          <Input
            disabled={busyOperation !== null}
            maxLength={36}
            onChange={(event) => { setItemId(event.currentTarget.value); }}
            value={itemId}
          />
          <FieldError>Enter a valid UUID.</FieldError>
        </TextField>
        <Button
          className="min-w-36"
          isDisabled={!canCreatePin || !validItemId || (busyOperation !== null && busyOperation !== 'pin')}
          isPending={busyOperation === 'pin'}
          onPress={() => { void pin(); }}
        >
          {busyOperation === 'pin' ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <Pin aria-hidden="true" className="size-4" />}
          <span className="inline-flex min-h-5 items-center">Pin candidate</span>
        </Button>
      </div>

      {loading && hasLoaded && (
        <p aria-live="polite" className="text-sm text-muted" role="status">Refreshing background candidates...</p>
      )}

      <AsyncContent
        empty={<CandidatesEmptyState />}
        error={loadError}
        hasData={hasLoaded}
        isEmpty={hasLoaded && page.items.length === 0}
        isPending={loading}
        loading={<CandidatesSkeleton />}
        onRetry={() => { void reload(); }}
      >
        <ResponsiveCollection
          ariaLabel="Pinned background candidates collection"
          desktop={<CandidatesTable busyOperation={busyOperation} items={page.items} onUnpin={unpin} />}
          mobile={<CandidatesMobileList busyOperation={busyOperation} items={page.items} onUnpin={unpin} />}
        />
      </AsyncContent>

      {hasLoaded && (
        <Pagination aria-label="Background candidates pagination" className="flex flex-wrap items-center justify-between gap-3">
          <Pagination.Summary>{firstRecord}-{lastRecord} of {page.totalRecordCount}</Pagination.Summary>
          <Pagination.Content>
            <Pagination.Item>
              <Pagination.Previous
                aria-label="Previous candidate page"
                isDisabled={controlsLocked || pageIndex === 0}
                onPress={() => { setPageIndex((current) => Math.max(0, current - 1)); }}
              >
                <Pagination.PreviousIcon />
                <span className="sr-only sm:not-sr-only">Previous</span>
              </Pagination.Previous>
            </Pagination.Item>
            <Pagination.Item>
              <Pagination.Link aria-label={`Candidate page ${String(pageIndex + 1)}`} isActive isDisabled>
                {pageIndex + 1}
              </Pagination.Link>
            </Pagination.Item>
            <Pagination.Item>
              <Pagination.Next
                aria-label="Next candidate page"
                isDisabled={controlsLocked || pageIndex + 1 >= totalPages}
                onPress={() => { setPageIndex((current) => Math.min(totalPages - 1, current + 1)); }}
              >
                <span className="sr-only sm:not-sr-only">Next</span>
                <Pagination.NextIcon />
              </Pagination.Next>
            </Pagination.Item>
          </Pagination.Content>
        </Pagination>
      )}
    </section>
  );
}

function CandidatesTable({
  busyOperation,
  items,
  onUnpin,
}: CandidateCollectionProps) {
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label="Pinned background candidates" className="min-w-[40rem] table-fixed">
          <Table.Header>
            <Table.Column isRowHeader>Name</Table.Column>
            <Table.Column className="w-24 text-right">Year</Table.Column>
            <Table.Column>Structure</Table.Column>
            <Table.Column>Selected</Table.Column>
            <Table.Column className="w-36 text-right">Actions</Table.Column>
          </Table.Header>
          <Table.Body>
            {items.map((candidate) => (
              <Table.Row id={candidate.id} key={candidate.id}>
                <Table.Cell><CandidateName candidate={candidate} /></Table.Cell>
                <Table.Cell><span className="block text-right tabular-nums">{candidate.productionYear ?? 'Unknown'}</span></Table.Cell>
                <Table.Cell>{humanizeIdentifier(candidate.structureState)}</Table.Cell>
                <Table.Cell>{formatCandidateDate(candidate.selectedAt)}</Table.Cell>
                <Table.Cell><UnpinButton busyOperation={busyOperation} candidate={candidate} onUnpin={onUnpin} /></Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}

function CandidatesMobileList({ busyOperation, items, onUnpin }: CandidateCollectionProps) {
  return (
    <ul aria-label="Pinned background candidates mobile" className="divide-y divide-border border-y border-border">
      {items.map((candidate) => (
        <li aria-label={candidate.name} className="space-y-4 py-4" key={candidate.id}>
          <CandidateName candidate={candidate} />
          <dl className="grid grid-cols-[6rem_minmax(0,1fr)] gap-x-3 gap-y-3 text-sm">
            <CandidateField label="Year">{candidate.productionYear ?? 'Unknown'}</CandidateField>
            <CandidateField label="Structure">{humanizeIdentifier(candidate.structureState)}</CandidateField>
            <CandidateField label="Selected">{formatCandidateDate(candidate.selectedAt)}</CandidateField>
          </dl>
          <UnpinButton busyOperation={busyOperation} candidate={candidate} onUnpin={onUnpin} />
        </li>
      ))}
    </ul>
  );
}

interface CandidateCollectionProps {
  busyOperation: string | null;
  items: HybridCandidate[];
  onUnpin: (candidate: HybridCandidate) => Promise<void>;
}

function CandidateName({ candidate }: { candidate: HybridCandidate }) {
  return (
    <div className="min-w-0">
      <p className="break-words font-semibold text-foreground">{candidate.name}</p>
      <p className="mt-1 break-all font-mono text-xs text-muted">{candidate.id}</p>
    </div>
  );
}

function UnpinButton({ busyOperation, candidate, onUnpin }: Omit<CandidateCollectionProps, 'items'> & { candidate: HybridCandidate }) {
  const operation = `unpin-${candidate.id}`;
  return (
    <div className="flex justify-end">
      <Button
        aria-label={`Remove pin for ${candidate.name}`}
        className="min-w-32"
        isDisabled={busyOperation !== null && busyOperation !== operation}
        isPending={busyOperation === operation}
        onPress={() => { void onUnpin(candidate); }}
        size="sm"
        variant="danger-soft"
      >
        {busyOperation === operation ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <Trash2 aria-hidden="true" className="size-4" />}
        <span className="inline-flex min-h-5 items-center">Remove pin</span>
      </Button>
    </div>
  );
}

function CandidateField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="font-medium text-muted">{label}</dt>
      <dd className="min-w-0 text-foreground">{children}</dd>
    </>
  );
}

function CandidatesSkeleton() {
  return (
    <div aria-label="Loading background candidates" className="space-y-3" role="status">
      <Skeleton className="h-10 w-full" />
      <Skeleton className="h-16 w-full" />
      <Skeleton className="h-16 w-full" />
    </div>
  );
}

function CandidatesEmptyState() {
  return <p className="border-y border-border py-8 text-center text-sm text-muted">No background candidates are pinned.</p>;
}

function UnavailableCandidates() {
  return (
    <section aria-labelledby="background-candidates-heading" className="space-y-4 border-t border-border py-7">
      <div>
        <h2 className="text-base font-semibold text-foreground" id="background-candidates-heading">Background candidates</h2>
        <p className="mt-1 text-sm text-muted">Candidate management requires a library with a compatible identifier.</p>
      </div>
      <Alert status="warning">
        <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
        <Alert.Content>
          <Alert.Title>Candidate management unavailable</Alert.Title>
          <Alert.Description>This library identifier cannot be used with background candidate commands.</Alert.Description>
        </Alert.Content>
      </Alert>
    </section>
  );
}

function formatCandidateDate(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(value));
}

function isUuid(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(value);
}

async function fetchCandidatePage(
  libraryId: string,
  pageIndex: number,
  signal: AbortSignal,
): Promise<CandidateLoadResult> {
  try {
    return {
      page: await listHybridCandidates(libraryId, pageIndex * PAGE_SIZE, PAGE_SIZE, signal),
    };
  } catch (error: unknown) {
    return { error };
  }
}
