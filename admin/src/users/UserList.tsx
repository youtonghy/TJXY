import {
  Input,
  Label,
  ListBox,
  Pagination,
  Select,
  Skeleton,
  Table,
  TextField,
  Tooltip,
} from '@heroui/react';
import { Eye, Pencil, Plus, Search, UsersRound } from 'lucide-react';
import {
  ListBase,
  useCreatePath,
  useListContext,
} from 'ra-core';
import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { Link } from 'react-router-dom';

import type {
  UserAccessFilter,
  UserListMeta,
} from '../api/dataProvider';
import type { UserRecord } from '../api/types';
import { isNonNegativeInteger, isRecord } from '../api/responseValidation';
import { AsyncContent } from '../ui/AsyncContent';
import { PageHeader } from '../ui/PageHeader';
import { ResponsiveCollection } from '../ui/ResponsiveCollection';
import { UserStatus } from './UserStatus';

const actionLinkClassName = [
  'inline-flex size-9 shrink-0 items-center justify-center rounded-md',
  'text-muted transition-colors hover:bg-default hover:text-foreground',
  'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus',
].join(' ');

export function UserList() {
  return (
    <ListBase<UserRecord>
      disableAuthentication
      perPage={25}
      queryOptions={{ onError: () => undefined }}
      resource="users"
      sort={{ field: 'Name', order: 'ASC' }}
    >
      <UserListView />
    </ListBase>
  );
}

function UserListView() {
  const listContext = useListContext<UserRecord>();
  const {
    data,
    error,
    isFetching,
    isPending,
    page,
    perPage,
    refetch,
    setFilters,
    setPage,
    total,
  } = listContext;
  const rawFilterValues: unknown = listContext.filterValues;
  const rawMeta: unknown = listContext.meta;
  const createPath = useCreatePath();
  const routeFilters = parseUserListFilterValues(rawFilterValues);
  const [retainedResult, setRetainedResult] = useState<{
    data: UserRecord[];
    meta: UserListMeta | null;
    page: number;
    total: number;
  } | null>(null);

  useEffect(() => {
    if (data === undefined || isFetching === true) return;
    // Query keys change with filters, so keep the last successful result for a stale-data fallback.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setRetainedResult({
      data,
      meta: parseUserListMeta(rawMeta),
      page,
      total,
    });
  }, [data, isFetching, page, rawMeta, total]);

  const applyFilters = (
    nextQuery: string,
    nextAccess: UserAccessFilter,
  ) => {
    const normalizedQuery = nextQuery.trim();
    const nextFilters = {
      ...(normalizedQuery.length > 0 ? { q: normalizedQuery } : {}),
      ...(nextAccess === 'all' ? {} : { access: nextAccess }),
    };
    setPage(1);
    setFilters(nextFilters);
  };

  const isRefreshing = isFetching === true && !isPending;
  const showsRetainedResult = data === undefined || isRefreshing;
  const records = showsRetainedResult
    ? retainedResult?.data ?? data ?? []
    : data;
  const hasData = data !== undefined || retainedResult !== null;
  const listMeta = showsRetainedResult
    ? retainedResult?.meta ?? null
    : parseUserListMeta(rawMeta);
  const visibleTotal = showsRetainedResult
    ? retainedResult?.total ?? records.length
    : total;
  const visiblePage = showsRetainedResult
    ? retainedResult?.page ?? page
    : page;
  const retry = () => { void refetch(); };

  return (
    <div className="space-y-5">
      <PageHeader
        actions={(
          <Link
            className="inline-flex min-h-9 items-center gap-2 rounded-md bg-accent px-3 text-sm font-semibold text-accent-foreground hover:bg-accent/90"
            to={createPath({ resource: 'users', type: 'create' })}
          >
            <Plus aria-hidden="true" className="size-4" />
            Create user
          </Link>
        )}
        description="Manage administrator access, sign-in state, and user credentials."
        title="Users"
      />

      {listMeta !== null && <UserSummaries meta={listMeta} />}

      <UserFilters
        initialAccess={routeFilters.access}
        initialQuery={routeFilters.q}
        key={JSON.stringify(routeFilters)}
        onFiltersChange={applyFilters}
      />

      <AsyncContent
        empty={<UserEmptyState />}
        error={error ?? null}
        hasData={hasData}
        isEmpty={hasData && records.length === 0}
        isPending={isPending}
        loading={<UserListSkeleton />}
        onRetry={retry}
      >
        <div
          aria-busy={isFetching === true ? true : undefined}
          className="space-y-4"
        >
          {isRefreshing && (
            <p className="text-sm text-muted" role="status">Updating user results...</p>
          )}
          <ResponsiveCollection
            ariaLabel="Users collection"
            desktop={<UserTable isRefreshing={isRefreshing} records={records} />}
            mobile={<UserMobileList isRefreshing={isRefreshing} records={records} />}
          />
          <UserPagination
            isDisabled={isFetching === true}
            onPageChange={setPage}
            page={visiblePage}
            perPage={perPage}
            total={visibleTotal}
          />
        </div>
      </AsyncContent>
    </div>
  );
}

function UserFilters({
  initialAccess,
  initialQuery,
  onFiltersChange,
}: {
  initialAccess: UserAccessFilter;
  initialQuery: string;
  onFiltersChange: (
    query: string,
    access: UserAccessFilter,
  ) => void;
}) {
  const [query, setQuery] = useState(initialQuery);
  const [access, setAccess] = useState<UserAccessFilter>(initialAccess);
  const queryRef = useRef(initialQuery);
  const searchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => {
    if (searchTimerRef.current !== null) clearTimeout(searchTimerRef.current);
  }, []);

  const cancelScheduledSearch = () => {
    if (searchTimerRef.current === null) return;
    clearTimeout(searchTimerRef.current);
    searchTimerRef.current = null;
  };

  return (
    <div className="grid gap-3 sm:grid-cols-[minmax(0,22rem)_13rem] sm:items-end">
      <TextField fullWidth name="user-search" value={query}>
        <Label>Search users</Label>
        <div className="relative">
          <Search
            aria-hidden="true"
            className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted"
          />
          <Input
            className="pl-9"
            fullWidth
            onChange={(event) => {
              const nextQuery = event.currentTarget.value;
              setQuery(nextQuery);
              queryRef.current = nextQuery;
              cancelScheduledSearch();
              searchTimerRef.current = setTimeout(() => {
                searchTimerRef.current = null;
                onFiltersChange(nextQuery, access);
              }, 500);
            }}
            placeholder="Name or user ID"
            type="search"
          />
        </div>
      </TextField>

      <Select
        fullWidth
        onChange={(key) => {
          if (!isUserAccessFilter(key)) return;
          cancelScheduledSearch();
          setAccess(key);
          onFiltersChange(queryRef.current, key);
        }}
        value={access}
      >
        <Label>Access</Label>
        <Select.Trigger>
          <Select.Value />
          <Select.Indicator />
        </Select.Trigger>
        <Select.Popover>
          <ListBox>
            <ListBox.Item id="all" textValue="All users">
              All users
              <ListBox.ItemIndicator />
            </ListBox.Item>
            <ListBox.Item id="administrator" textValue="Administrators">
              Administrators
              <ListBox.ItemIndicator />
            </ListBox.Item>
            <ListBox.Item id="standard" textValue="Standard users">
              Standard users
              <ListBox.ItemIndicator />
            </ListBox.Item>
            <ListBox.Item id="disabled" textValue="Disabled">
              Disabled
              <ListBox.ItemIndicator />
            </ListBox.Item>
          </ListBox>
        </Select.Popover>
      </Select>
    </div>
  );
}

function UserSummaries({ meta }: { meta: UserListMeta }) {
  return (
    <div aria-label="User summary" className="flex flex-wrap gap-x-6 gap-y-2 text-sm text-muted" role="group">
      <p><strong className="font-semibold text-foreground">{meta.totalUsers}</strong> total {plural(meta.totalUsers, 'user')}</p>
      <p><strong className="font-semibold text-foreground">{meta.administrators}</strong> enabled {plural(meta.administrators, 'administrator')}</p>
      <p><strong className="font-semibold text-foreground">{meta.disabled}</strong> disabled {plural(meta.disabled, 'user')}</p>
    </div>
  );
}

function UserTable({
  isRefreshing,
  records,
}: {
  isRefreshing: boolean;
  records: UserRecord[];
}) {
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label="Users">
          <Table.Header>
            <Table.Column isRowHeader>Name</Table.Column>
            <Table.Column>Access</Table.Column>
            <Table.Column className="w-24 text-right">Actions</Table.Column>
          </Table.Header>
          <Table.Body>
            {records.map((record) => (
              <Table.Row id={record.id} key={record.id}>
                <Table.Cell>
                  <div className="min-w-0 py-1">
                    <p className="font-semibold text-foreground">{record.Name}</p>
                    <p className="break-all text-xs text-muted">{record.Id}</p>
                  </div>
                </Table.Cell>
                <Table.Cell>
                  <UserStatus
                    isAdministrator={record.Policy.IsAdministrator}
                    isDisabled={record.Policy.IsDisabled}
                  />
                </Table.Cell>
                <Table.Cell>
                  <div className="flex justify-end gap-1">
                    <UserActionLinks isDisabled={isRefreshing} record={record} />
                  </div>
                </Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}

function UserMobileList({
  isRefreshing,
  records,
}: {
  isRefreshing: boolean;
  records: UserRecord[];
}) {
  return (
    <ul aria-label="Users mobile" className="divide-y divide-border border-y border-border">
      {records.map((record) => (
        <li aria-label={record.Name} className="space-y-4 py-4" key={record.id}>
          <dl className="grid grid-cols-[6rem_minmax(0,1fr)] gap-x-3 gap-y-3 text-sm">
            <MobileField label="Name"><span className="font-semibold text-foreground">{record.Name}</span></MobileField>
            <MobileField label="User ID"><span className="break-all text-muted">{record.Id}</span></MobileField>
            <MobileField label="Access">
              {record.Policy.IsAdministrator ? 'Administrator' : 'Standard'}
            </MobileField>
            <MobileField label="Status">
              {record.Policy.IsDisabled ? 'Disabled' : 'Enabled'}
            </MobileField>
          </dl>
          <div className="flex justify-end gap-1" aria-label={`Actions for ${record.Name}`}>
            <UserActionLinks isDisabled={isRefreshing} record={record} />
          </div>
        </li>
      ))}
    </ul>
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

function UserActionLinks({
  isDisabled,
  record,
}: {
  isDisabled: boolean;
  record: UserRecord;
}) {
  const createPath = useCreatePath();
  if (isDisabled) {
    return (
      <>
        <span aria-label={`View ${record.Name} unavailable while updating`} className={actionLinkClassName}>
          <Eye aria-hidden="true" className="size-4" />
        </span>
        <span aria-label={`Edit ${record.Name} unavailable while updating`} className={actionLinkClassName}>
          <Pencil aria-hidden="true" className="size-4" />
        </span>
      </>
    );
  }
  return (
    <>
      <Tooltip>
        <Link
          aria-label={`View ${record.Name}`}
          className={actionLinkClassName}
          to={createPath({ id: record.id, resource: 'users', type: 'show' })}
        >
          <Eye aria-hidden="true" className="size-4" />
        </Link>
        <Tooltip.Content>View user</Tooltip.Content>
      </Tooltip>
      <Tooltip>
        <Link
          aria-label={`Edit ${record.Name}`}
          className={actionLinkClassName}
          to={createPath({ id: record.id, resource: 'users', type: 'edit' })}
        >
          <Pencil aria-hidden="true" className="size-4" />
        </Link>
        <Tooltip.Content>Edit user</Tooltip.Content>
      </Tooltip>
    </>
  );
}

function UserPagination({
  isDisabled,
  page,
  perPage,
  total,
  onPageChange,
}: {
  isDisabled: boolean;
  page: number;
  perPage: number;
  total: number;
  onPageChange: (page: number) => void;
}) {
  const totalPages = Math.max(1, Math.ceil(total / perPage));
  const first = (page - 1) * perPage + 1;
  const last = Math.min(page * perPage, total);
  return (
    <Pagination aria-label="Users pagination" className="flex flex-wrap items-center justify-between gap-3">
      <Pagination.Summary>{first}-{last} of {total}</Pagination.Summary>
      <Pagination.Content>
        <Pagination.Item>
          <Pagination.Previous
            aria-label="Previous page"
            isDisabled={isDisabled || page <= 1}
            onPress={() => { onPageChange(page - 1); }}
          >
            <Pagination.PreviousIcon />
            <span className="sr-only sm:not-sr-only">Previous</span>
          </Pagination.Previous>
        </Pagination.Item>
        <Pagination.Item>
          <Pagination.Link aria-label={`Page ${String(page)}`} isActive isDisabled>
            {page}
          </Pagination.Link>
        </Pagination.Item>
        <Pagination.Item>
          <Pagination.Next
            aria-label="Next page"
            isDisabled={isDisabled || page >= totalPages}
            onPress={() => { onPageChange(page + 1); }}
          >
            <span className="sr-only sm:not-sr-only">Next</span>
            <Pagination.NextIcon />
          </Pagination.Next>
        </Pagination.Item>
      </Pagination.Content>
    </Pagination>
  );
}

function UserListSkeleton() {
  return (
    <div aria-label="Loading users" className="space-y-3" role="status">
      <Skeleton className="h-11 w-full rounded-md" />
      <Skeleton className="h-16 w-full rounded-md" />
      <Skeleton className="h-16 w-full rounded-md" />
      <span className="sr-only">Loading users</span>
    </div>
  );
}

function UserEmptyState() {
  return (
    <div className="flex min-h-52 flex-col items-center justify-center gap-3 border-y border-border py-8 text-center">
      <UsersRound aria-hidden="true" className="size-6 text-muted" />
      <div>
        <h2 className="text-base font-semibold text-foreground">No users match the current filters.</h2>
        <p className="mt-1 text-sm text-muted">Change the search or access filter to see other users.</p>
      </div>
    </div>
  );
}

function parseUserListMeta(value: unknown): UserListMeta | null {
  if (!isRecord(value)) return null;
  const { totalUsers, administrators, disabled } = value;
  if (
    !isNonNegativeInteger(totalUsers)
    || !isNonNegativeInteger(administrators)
    || !isNonNegativeInteger(disabled)
  ) return null;
  return { totalUsers, administrators, disabled };
}

function isUserAccessFilter(value: unknown): value is UserAccessFilter {
  return value === 'all'
    || value === 'administrator'
    || value === 'standard'
    || value === 'disabled';
}

function parseUserListFilterValues(value: unknown): UserListFilterValues {
  if (!isRecord(value)) return { q: '', access: 'all' };
  return {
    q: typeof value.q === 'string' ? value.q : '',
    access: isUserAccessFilter(value.access) ? value.access : 'all',
  };
}

interface UserListFilterValues {
  q: string;
  access: UserAccessFilter;
}

function plural(count: number, singular: string): string {
  return count === 1 ? singular : `${singular}s`;
}
