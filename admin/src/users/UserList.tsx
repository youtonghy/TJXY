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
import { useTranslate } from '../settings/i18n';
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
  const tr = useTranslate();
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

  const hasData = data !== undefined || retainedResult !== null;
  const isRefreshing = isFetching === true && hasData;
  const showsRetainedResult = data === undefined || isRefreshing;
  const records = showsRetainedResult
    ? retainedResult?.data ?? data ?? []
    : data;
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
            {tr('Create user', '创建用户')}
          </Link>
        )}
        description={tr('Manage administrator access, sign-in state, and user credentials.', '管理管理员权限、登录状态和用户凭据。')}
        title={tr('Users', '用户')}
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
            <p className="text-sm text-muted" role="status">{tr('Updating user results...', '正在更新用户结果…')}</p>
          )}
          <ResponsiveCollection
            ariaLabel={tr('Users collection', '用户集合')}
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
  const tr = useTranslate();
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
        <Label>{tr('Search users', '搜索用户')}</Label>
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
            placeholder={tr('Name or user ID', '姓名或用户 ID')}
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
        <Label>{tr('Access', '权限')}</Label>
        <Select.Trigger>
          <Select.Value />
          <Select.Indicator />
        </Select.Trigger>
        <Select.Popover>
          <ListBox>
            <ListBox.Item id="all" textValue={tr('All users', '所有用户')}>
              {tr('All users', '所有用户')}
              <ListBox.ItemIndicator />
            </ListBox.Item>
            <ListBox.Item id="administrator" textValue={tr('Administrators', '管理员')}>
              {tr('Administrators', '管理员')}
              <ListBox.ItemIndicator />
            </ListBox.Item>
            <ListBox.Item id="standard" textValue={tr('Standard users', '普通用户')}>
              {tr('Standard users', '普通用户')}
              <ListBox.ItemIndicator />
            </ListBox.Item>
            <ListBox.Item id="disabled" textValue={tr('Disabled', '已禁用')}>
              {tr('Disabled', '已禁用')}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          </ListBox>
        </Select.Popover>
      </Select>
    </div>
  );
}

function UserSummaries({ meta }: { meta: UserListMeta }) {
  const tr = useTranslate();
  return (
    <div aria-label={tr('User summary', '用户摘要')} className="flex flex-wrap gap-x-6 gap-y-2 text-sm text-muted" role="group">
      <p><strong className="font-semibold text-foreground">{meta.totalUsers}</strong> {tr(`total ${plural(meta.totalUsers, 'user')}`, '位用户')}</p>
      <p><strong className="font-semibold text-foreground">{meta.administrators}</strong> {tr(`enabled ${plural(meta.administrators, 'administrator')}`, '位已启用管理员')}</p>
      <p><strong className="font-semibold text-foreground">{meta.disabled}</strong> {tr(`disabled ${plural(meta.disabled, 'user')}`, '位已禁用用户')}</p>
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
  const tr = useTranslate();
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label={tr('Users', '用户')}>
          <Table.Header>
            <Table.Column isRowHeader>{tr('Name', '姓名')}</Table.Column>
            <Table.Column>{tr('Access', '权限')}</Table.Column>
            <Table.Column className="w-24 text-right">{tr('Actions', '操作')}</Table.Column>
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
  const tr = useTranslate();
  return (
    <ul aria-label={tr('Users mobile', '用户移动端列表')} className="divide-y divide-border border-y border-border">
      {records.map((record) => (
        <li aria-label={record.Name} className="space-y-4 py-4" key={record.id}>
          <dl className="grid grid-cols-[6rem_minmax(0,1fr)] gap-x-3 gap-y-3 text-sm">
            <MobileField label={tr('Name', '姓名')}><span className="font-semibold text-foreground">{record.Name}</span></MobileField>
            <MobileField label={tr('User ID', '用户 ID')}><span className="break-all text-muted">{record.Id}</span></MobileField>
            <MobileField label={tr('Access', '权限')}>
              {record.Policy.IsAdministrator ? tr('Administrator', '管理员') : tr('Standard', '普通用户')}
            </MobileField>
            <MobileField label={tr('Status', '状态')}>
              {record.Policy.IsDisabled ? tr('Disabled', '已禁用') : tr('Enabled', '已启用')}
            </MobileField>
          </dl>
          <div className="flex justify-end gap-1" aria-label={`${tr('Actions for', '操作：')} ${record.Name}`}>
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
  const tr = useTranslate();
  const createPath = useCreatePath();
  if (isDisabled) {
    return (
      <>
        <span aria-label={`${tr('View', '查看')} ${record.Name} ${tr('unavailable while updating', '在更新期间不可用')}`} className={actionLinkClassName}>
          <Eye aria-hidden="true" className="size-4" />
        </span>
        <span aria-label={`${tr('Edit', '编辑')} ${record.Name} ${tr('unavailable while updating', '在更新期间不可用')}`} className={actionLinkClassName}>
          <Pencil aria-hidden="true" className="size-4" />
        </span>
      </>
    );
  }
  return (
    <>
      <Tooltip>
        <Link
          aria-label={`${tr('View', '查看')} ${record.Name}`}
          className={actionLinkClassName}
          to={createPath({ id: record.id, resource: 'users', type: 'show' })}
        >
          <Eye aria-hidden="true" className="size-4" />
        </Link>
        <Tooltip.Content>{tr('View user', '查看用户')}</Tooltip.Content>
      </Tooltip>
      <Tooltip>
        <Link
          aria-label={`${tr('Edit', '编辑')} ${record.Name}`}
          className={actionLinkClassName}
          to={createPath({ id: record.id, resource: 'users', type: 'edit' })}
        >
          <Pencil aria-hidden="true" className="size-4" />
        </Link>
        <Tooltip.Content>{tr('Edit user', '编辑用户')}</Tooltip.Content>
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
  const tr = useTranslate();
  const totalPages = Math.max(1, Math.ceil(total / perPage));
  const first = (page - 1) * perPage + 1;
  const last = Math.min(page * perPage, total);
  return (
    <Pagination aria-label={tr('Users pagination', '用户分页')} className="flex flex-wrap items-center justify-between gap-3">
      <Pagination.Summary>{first}-{last} {tr('of', '共')} {total}</Pagination.Summary>
      <Pagination.Content>
        <Pagination.Item>
          <Pagination.Previous
            aria-label={tr('Previous page', '上一页')}
            isDisabled={isDisabled || page <= 1}
            onPress={() => { onPageChange(page - 1); }}
          >
            <Pagination.PreviousIcon />
            <span className="sr-only sm:not-sr-only">{tr('Previous', '上一页')}</span>
          </Pagination.Previous>
        </Pagination.Item>
        <Pagination.Item>
          <Pagination.Link aria-label={`${tr('Page', '第')} ${String(page)} ${tr('', '页')}`} isActive isDisabled>
            {page}
          </Pagination.Link>
        </Pagination.Item>
        <Pagination.Item>
          <Pagination.Next
            aria-label={tr('Next page', '下一页')}
            isDisabled={isDisabled || page >= totalPages}
            onPress={() => { onPageChange(page + 1); }}
          >
            <span className="sr-only sm:not-sr-only">{tr('Next', '下一页')}</span>
            <Pagination.NextIcon />
          </Pagination.Next>
        </Pagination.Item>
      </Pagination.Content>
    </Pagination>
  );
}

function UserListSkeleton() {
  const tr = useTranslate();
  return (
    <div aria-label={tr('Loading users', '正在加载用户')} className="space-y-3" role="status">
      <Skeleton className="h-11 w-full rounded-md" />
      <Skeleton className="h-16 w-full rounded-md" />
      <Skeleton className="h-16 w-full rounded-md" />
      <span className="sr-only">{tr('Loading users', '正在加载用户')}</span>
    </div>
  );
}

function UserEmptyState() {
  const tr = useTranslate();
  return (
    <div className="flex min-h-52 flex-col items-center justify-center gap-3 border-y border-border py-8 text-center">
      <UsersRound aria-hidden="true" className="size-6 text-muted" />
      <div>
        <h2 className="text-base font-semibold text-foreground">{tr('No users match the current filters.', '没有用户符合当前筛选条件。')}</h2>
        <p className="mt-1 text-sm text-muted">{tr('Change the search or access filter to see other users.', '请更改搜索内容或权限筛选条件。')}</p>
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
