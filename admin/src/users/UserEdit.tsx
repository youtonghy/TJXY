import {
  Alert,
  Button,
  FieldError,
  Input,
  Label,
  Skeleton,
  Switch,
  TextField,
} from '@heroui/react';
import {
  KeyRound,
  RefreshCw,
  Save,
  ShieldCheck,
  Trash2,
  TriangleAlert,
  UserRound,
} from 'lucide-react';
import {
  EditBase,
  useDelete,
  useEditContext,
  useLogoutIfAccessDenied,
  useNotify,
  useRedirect,
  useRefresh,
  useUpdate,
} from 'ra-core';
import {
  useRef,
  useState,
  type ReactNode,
  type SyntheticEvent,
} from 'react';

import type { UserRecord } from '../api/types';
import { AsyncContent } from '../ui/AsyncContent';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { PageHeader } from '../ui/PageHeader';
import { updateUserPassword, updateUserPolicy } from './userCommands';

interface PanelProps {
  record: UserRecord;
  onSaved: () => void | Promise<void>;
}

export function UserEdit() {
  return (
    <EditBase<UserRecord>
      disableAuthentication
      mutationMode="pessimistic"
      queryOptions={{ onError: () => undefined }}
      redirect={false}
      redirectOnError={false}
      resource="users"
    >
      <UserEditView />
    </EditBase>
  );
}

function UserEditView() {
  const {
    error,
    isFetching,
    isPending,
    record,
    refetch,
  } = useEditContext<UserRecord>();
  const refresh = useRefresh();
  const retry = () => { void refetch(); };

  return (
    <AsyncContent
      empty={null}
      error={error ?? null}
      hasData={record !== undefined}
      isEmpty={false}
      isPending={isPending}
      loading={<UserEditSkeleton />}
      onRetry={retry}
    >
      {record !== undefined && (
        <div aria-busy={isFetching || undefined} className="space-y-6">
          <PageHeader
            actions={(
              <Button
                isDisabled={isFetching}
                onPress={retry}
                variant="tertiary"
              >
                <RefreshCw aria-hidden="true" className="size-4" />
                Reload user
              </Button>
            )}
            breadcrumbs={[
              { label: 'Users', to: '/admin/users' },
              { label: `Edit ${record.Name}` },
            ]}
            description="Manage identity, access, credentials, and account lifecycle."
            title={`Edit ${record.Name}`}
          />

          <div
            className="max-w-3xl divide-y divide-border"
            key={recordRevision(record)}
          >
            <RenamePanel record={record} onSaved={refresh} />
            <PolicyPanel record={record} onSaved={refresh} />
            <PasswordPanel record={record} onSaved={refresh} />
            <DeletePanel record={record} />
          </div>
        </div>
      )}
    </AsyncContent>
  );
}

export function RenamePanel({ record, onSaved }: PanelProps) {
  const [name, setName] = useState(record.Name);
  const [update] = useUpdate<UserRecord>();
  const notify = useNotify();
  const [state, run] = useSectionCommand();
  const normalizedName = name.trim();

  const submit = (event: SyntheticEvent<HTMLFormElement, SubmitEvent>) => {
    event.preventDefault();
    void run(async () => {
      await update(
        'users',
        {
          id: record.id,
          data: { Name: normalizedName },
          previousData: record,
        },
        { mutationMode: 'pessimistic', returnPromise: true },
      );
      notify('User identity updated.', { type: 'success' });
      await onSaved();
    });
  };

  return (
    <CommandSection
      description="Update the display name used throughout the administrator workspace."
      icon={<UserRound aria-hidden="true" className="size-4" />}
      title="Identity"
    >
      <p className="break-all text-xs text-muted">{record.Id}</p>
      <form className="mt-4 max-w-lg space-y-4" onSubmit={submit}>
        <TextField fullWidth isRequired name="Name">
          <Label>Name</Label>
          <Input
            autoComplete="off"
            disabled={state.pending}
            onChange={(event) => { setName(event.currentTarget.value); }}
            value={name}
          />
        </TextField>
        <CommandError error={state.error} />
        <Button
          aria-busy={state.pending}
          isDisabled={state.pending || normalizedName.length === 0}
          type="submit"
        >
          <Save aria-hidden="true" className="size-4" />
          <span className="inline-flex min-h-5 items-center">Save identity</span>
        </Button>
      </form>
    </CommandSection>
  );
}

export function PolicyPanel({ record, onSaved }: PanelProps) {
  const [isAdministrator, setAdministrator] = useState(record.Policy.IsAdministrator);
  const [isDisabled, setDisabled] = useState(record.Policy.IsDisabled);
  const notify = useNotify();
  const [state, run] = useSectionCommand();

  const submit = (event: SyntheticEvent<HTMLFormElement, SubmitEvent>) => {
    event.preventDefault();
    void run(async () => {
      await updateUserPolicy(record.id, { isAdministrator, isDisabled });
      notify('User access policy updated.', { type: 'success' });
      await onSaved();
    });
  };

  return (
    <CommandSection
      description="Grant administrator capabilities or block this account from signing in."
      icon={<ShieldCheck aria-hidden="true" className="size-4" />}
      title="Access policy"
    >
      <form className="max-w-lg space-y-4" onSubmit={submit}>
        <div className="space-y-3">
          <PolicySwitch
            isDisabled={state.pending}
            isSelected={isAdministrator}
            label="Administrator"
            onChange={setAdministrator}
          />
          <PolicySwitch
            isDisabled={state.pending}
            isSelected={isDisabled}
            label="Disabled"
            onChange={setDisabled}
          />
        </div>
        <CommandError error={state.error} />
        <Button aria-busy={state.pending} isDisabled={state.pending} type="submit">
          <Save aria-hidden="true" className="size-4" />
          <span className="inline-flex min-h-5 items-center">Save access policy</span>
        </Button>
      </form>
    </CommandSection>
  );
}

export function PasswordPanel({ record, onSaved }: PanelProps) {
  const [newPassword, setNewPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [resetPassword, setResetPassword] = useState(false);
  const [validation, setValidation] = useState<string>();
  const notify = useNotify();
  const [state, run] = useSectionCommand();

  const submit = (event: SyntheticEvent<HTMLFormElement, SubmitEvent>) => {
    event.preventDefault();
    if (!resetPassword && newPassword !== confirmation) {
      setValidation('Passwords do not match.');
      return;
    }
    if (!resetPassword && newPassword.length === 0) {
      setValidation('A new password is required.');
      return;
    }
    setValidation(undefined);
    void run(async () => {
      await updateUserPassword(record.id, {
        newPassword: resetPassword ? '' : newPassword,
        resetPassword,
      });
      setNewPassword('');
      setConfirmation('');
      setResetPassword(false);
      notify('User password updated.', { type: 'success' });
      await onSaved();
    });
  };

  return (
    <CommandSection
      description="Replace the current credential or explicitly reset the account to an empty password."
      icon={<KeyRound aria-hidden="true" className="size-4" />}
      title="Password"
    >
      <form className="max-w-lg space-y-4" noValidate onSubmit={submit}>
        <TextField
          fullWidth
          isInvalid={validation !== undefined}
          isRequired={!resetPassword}
          name="new-password"
        >
          <Label>New password</Label>
          <Input
            autoComplete="new-password"
            disabled={state.pending || resetPassword}
            onChange={(event) => {
              setNewPassword(event.currentTarget.value);
              setValidation(undefined);
            }}
            type="password"
            value={newPassword}
          />
        </TextField>
        <TextField
          fullWidth
          isInvalid={validation !== undefined}
          isRequired={!resetPassword}
          name="confirm-password"
        >
          <Label>Confirm password</Label>
          <Input
            autoComplete="new-password"
            disabled={state.pending || resetPassword}
            onChange={(event) => {
              setConfirmation(event.currentTarget.value);
              setValidation(undefined);
            }}
            type="password"
            value={confirmation}
          />
          <FieldError>{validation}</FieldError>
        </TextField>
        <PolicySwitch
          isDisabled={state.pending}
          isSelected={resetPassword}
          label="Reset to an empty password"
          onChange={(selected) => {
            setResetPassword(selected);
            setValidation(undefined);
          }}
        />
        <CommandError error={state.error} />
        <Button
          aria-busy={state.pending}
          isDisabled={state.pending || (!resetPassword && newPassword.length === 0)}
          type="submit"
        >
          <Save aria-hidden="true" className="size-4" />
          <span className="inline-flex min-h-5 items-center">Save password</span>
        </Button>
      </form>
    </CommandSection>
  );
}

export function DeletePanel({ record }: { record: UserRecord }) {
  const [remove, { isPending }] = useDelete<UserRecord>();
  const [error, setError] = useState<unknown>();
  const runningRef = useRef(false);
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const notify = useNotify();
  const redirect = useRedirect();
  const errorDescription = error === undefined
    ? undefined
    : commandErrorMessage(error, 'The last enabled administrator cannot be deleted.');

  const confirmDelete = async () => {
    if (runningRef.current) return;
    runningRef.current = true;
    setError(undefined);
    try {
      await remove(
        'users',
        { id: record.id, previousData: record },
        { mutationMode: 'pessimistic', returnPromise: true },
      );
      notify('User deleted.', { type: 'success' });
      redirect('list', 'users');
    } catch (caught) {
      if (!(await logoutIfAccessDenied(caught))) setError(caught);
      throw caught;
    } finally {
      runningRef.current = false;
    }
  };

  return (
    <CommandSection
      description="Permanently remove this account. The last enabled administrator is protected."
      icon={<Trash2 aria-hidden="true" className="size-4" />}
      title="Danger zone"
      tone="danger"
    >
      <ConfirmDialog
        confirmLabel="Delete user"
        description={(
          <>
            <strong className="font-semibold text-foreground">{record.Name}</strong> will lose access
            permanently. This cannot be undone.
          </>
        )}
        errorDescription={errorDescription}
        isPending={isPending}
        onConfirm={confirmDelete}
        title={`Delete ${record.Name}?`}
        trigger={(
          <Button variant="danger-soft">
            <Trash2 aria-hidden="true" className="size-4" />
            Delete user
          </Button>
        )}
      />
    </CommandSection>
  );
}

function PolicySwitch({
  isDisabled,
  isSelected,
  label,
  onChange,
}: {
  isDisabled: boolean;
  isSelected: boolean;
  label: string;
  onChange: (selected: boolean) => void;
}) {
  return (
    <Switch
      isDisabled={isDisabled}
      isSelected={isSelected}
      onChange={onChange}
    >
      <Switch.Content>
        <Switch.Control>
          <Switch.Thumb />
        </Switch.Control>
        {label}
      </Switch.Content>
    </Switch>
  );
}

function CommandSection({
  title,
  description,
  icon,
  tone = 'default',
  children,
}: {
  title: string;
  description: string;
  icon: ReactNode;
  tone?: 'default' | 'danger';
  children: ReactNode;
}) {
  return (
    <section className="py-7 first:pt-0">
      <div className="mb-5 flex items-start gap-3">
        <span className={tone === 'danger' ? 'mt-0.5 text-danger' : 'mt-0.5 text-accent'}>
          {icon}
        </span>
        <div>
          <h2 className={tone === 'danger' ? 'text-base font-semibold text-danger' : 'text-base font-semibold text-foreground'}>
            {title}
          </h2>
          <p className="mt-1 text-sm leading-6 text-muted">{description}</p>
        </div>
      </div>
      {children}
    </section>
  );
}

function CommandError({ error }: { error?: unknown }) {
  if (error === undefined) return null;
  return (
    <Alert role="alert" status="danger">
      <Alert.Indicator>
        <TriangleAlert aria-hidden="true" className="size-4" />
      </Alert.Indicator>
      <Alert.Content>
        <Alert.Title>The command could not be completed</Alert.Title>
        <Alert.Description>{commandErrorMessage(error)}</Alert.Description>
      </Alert.Content>
    </Alert>
  );
}

function useSectionCommand(): [
  { pending: boolean; error?: unknown },
  (command: () => Promise<void>) => Promise<boolean>,
] {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>();
  const runningRef = useRef(false);
  const logoutIfAccessDenied = useLogoutIfAccessDenied();

  const run = async (command: () => Promise<void>) => {
    if (runningRef.current) return false;
    runningRef.current = true;
    setPending(true);
    setError(undefined);
    try {
      await command();
      return true;
    } catch (caught) {
      if (!(await logoutIfAccessDenied(caught))) setError(caught);
      return false;
    } finally {
      runningRef.current = false;
      setPending(false);
    }
  };

  return [{ pending, error }, run];
}

function commandErrorMessage(error: unknown, conflictMessage?: string): string {
  const status = statusOf(error);
  if (status === 409 && conflictMessage !== undefined) return conflictMessage;
  if (status === 400) return 'Check the submitted values and try again.';
  if (status === 403) return 'Administrator permission is required.';
  if (status === 404) return 'This user no longer exists.';
  return 'The server could not complete this command.';
}

function statusOf(error: unknown): number | undefined {
  if (typeof error !== 'object' || error === null || !('status' in error)) return undefined;
  return typeof error.status === 'number' ? error.status : undefined;
}

function recordRevision(record: UserRecord): string {
  return [
    record.id,
    record.Name,
    record.Policy.IsAdministrator,
    record.Policy.IsDisabled,
    record.HasConfiguredPassword,
  ].join(':');
}

function UserEditSkeleton() {
  return (
    <div aria-label="Loading user editor" className="max-w-3xl space-y-4" role="status">
      <Skeleton className="h-8 w-48 rounded-md" />
      <Skeleton className="h-20 w-full rounded-md" />
      <Skeleton className="h-20 w-full rounded-md" />
      <span className="sr-only">Loading user editor</span>
    </div>
  );
}
