import { Skeleton } from '@heroui/react';
import { Pencil, UserRound } from 'lucide-react';
import { ShowBase, useCreatePath, useShowContext } from 'ra-core';
import type { ReactNode } from 'react';
import { Link } from 'react-router-dom';

import type { UserRecord } from '../api/types';
import { AsyncContent } from '../ui/AsyncContent';
import { PageHeader } from '../ui/PageHeader';
import { UserStatus } from './UserStatus';

export function UserShow() {
  return (
    <ShowBase<UserRecord>
      disableAuthentication
      queryOptions={{ onError: () => undefined }}
      redirectOnError={false}
      resource="users"
    >
      <UserShowView />
    </ShowBase>
  );
}

function UserShowView() {
  const {
    error,
    isFetching,
    isPending,
    record,
    refetch,
  } = useShowContext<UserRecord>();
  const createPath = useCreatePath();
  const retry = () => { void refetch(); };

  return (
    <AsyncContent
      empty={<UserNotFound />}
      error={error ?? null}
      hasData={record !== undefined}
      isEmpty={false}
      isPending={isPending}
      loading={<UserShowSkeleton />}
      onRetry={retry}
    >
      {record !== undefined && (
        <div aria-busy={isFetching || undefined} className="space-y-6">
          <PageHeader
            actions={(
              <Link
                className="inline-flex min-h-9 items-center gap-2 rounded-md bg-accent px-3 text-sm font-semibold text-accent-foreground hover:bg-accent/90"
                to={createPath({ id: record.id, resource: 'users', type: 'edit' })}
              >
                <Pencil aria-hidden="true" className="size-4" />
                Edit user
              </Link>
            )}
            breadcrumbs={[
              { label: 'Users', to: '/admin/users' },
              { label: record.Name },
            ]}
            description="Account identity, access, and credential status."
            title={record.Name}
          />

          <section aria-label="User details" role="group">
            <dl className="max-w-3xl divide-y divide-border border-y border-border">
              <DetailRow label="Name">{record.Name}</DetailRow>
              <DetailRow label="User ID">
                <span className="break-all font-mono text-sm">{record.Id}</span>
              </DetailRow>
              <DetailRow label="Access">
                <UserStatus
                  isAdministrator={record.Policy.IsAdministrator}
                  isDisabled={record.Policy.IsDisabled}
                />
              </DetailRow>
              <DetailRow label="Password">
                {record.HasConfiguredPassword ? 'Configured' : 'Not configured'}
              </DetailRow>
            </dl>
          </section>
        </div>
      )}
    </AsyncContent>
  );
}

function DetailRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1 py-4 sm:grid-cols-[10rem_minmax(0,1fr)] sm:gap-6">
      <dt className="text-sm font-medium text-muted">{label}</dt>
      <dd className="min-w-0 text-sm text-foreground">{children}</dd>
    </div>
  );
}

function UserShowSkeleton() {
  return (
    <div aria-label="Loading user" className="max-w-3xl space-y-4" role="status">
      <Skeleton className="h-8 w-48 rounded-md" />
      <Skeleton className="h-16 w-full rounded-md" />
      <Skeleton className="h-16 w-full rounded-md" />
      <span className="sr-only">Loading user</span>
    </div>
  );
}

function UserNotFound() {
  return (
    <div className="flex min-h-52 flex-col items-center justify-center gap-3 border-y border-border py-8 text-center">
      <UserRound aria-hidden="true" className="size-6 text-muted" />
      <div>
        <h1 className="text-lg font-semibold text-foreground">User not found</h1>
        <p className="mt-1 text-sm text-muted">This account is no longer available.</p>
      </div>
    </div>
  );
}
