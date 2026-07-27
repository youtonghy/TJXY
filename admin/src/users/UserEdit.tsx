import { DeleteOutlineOutlined as DeleteOutlineIcon, SaveOutlined as SaveOutlinedIcon } from '@mui/icons-material';
import {
  Alert,
  Box,
  Button,
  Divider,
  FormControlLabel,
  Stack,
  Switch,
  TextField,
  Typography,
} from '@mui/material';
import { type SyntheticEvent, useRef, useState } from 'react';
import { Edit, useRecordContext, useRedirect, useRefresh } from 'react-admin';

import { dataProvider } from '../api/dataProvider';
import type { UserRecord } from '../api/types';
import { updateUserPassword, updateUserPolicy } from './userCommands';

interface PanelProps {
  record: UserRecord;
  onSaved: () => void | Promise<void>;
}

export function RenamePanel({ record, onSaved }: PanelProps) {
  const [name, setName] = useState(record.Name);
  const [state, run] = useCommandState();
  const submit = (event: SyntheticEvent<HTMLFormElement, SubmitEvent>) => {
    event.preventDefault();
    void run(async () => {
      await dataProvider.update<UserRecord>('users', {
        id: record.id,
        data: { Name: name },
        previousData: record,
      });
      await onSaved();
    });
  };
  return (
    <CommandPanel title="Name">
      <Stack component="form" onSubmit={submit} spacing={2}>
        <TextField label="Name" value={name} onChange={(event) => { setName(event.target.value); }} required fullWidth disabled={state.pending} />
        <CommandError error={state.error} />
        <Button type="submit" variant="contained" startIcon={<SaveOutlinedIcon />} disabled={state.pending || name.trim().length === 0}>Save name</Button>
      </Stack>
    </CommandPanel>
  );
}

export function PasswordPanel({ record, onSaved }: PanelProps) {
  const [newPassword, setNewPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [resetPassword, setResetPassword] = useState(false);
  const [validation, setValidation] = useState<string>();
  const [state, run] = useCommandState();
  const submit = (event: SyntheticEvent<HTMLFormElement, SubmitEvent>) => {
    event.preventDefault();
    if (!resetPassword && newPassword !== confirmation) {
      setValidation('Passwords do not match.');
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
      await onSaved();
    });
  };
  return (
    <CommandPanel title="Password">
      <Stack component="form" onSubmit={submit} spacing={2}>
        <TextField label="New password" type="password" autoComplete="new-password" value={newPassword} onChange={(event) => { setNewPassword(event.target.value); }} required={!resetPassword} disabled={state.pending || resetPassword} />
        <TextField label="Confirm password" type="password" autoComplete="new-password" value={confirmation} onChange={(event) => { setConfirmation(event.target.value); }} required={!resetPassword} disabled={state.pending || resetPassword} />
        <FormControlLabel control={<Switch checked={resetPassword} onChange={(event) => { setResetPassword(event.target.checked); }} disabled={state.pending} />} label="Reset to an empty password" />
        {validation === undefined ? null : <Alert severity="error">{validation}</Alert>}
        <CommandError error={state.error} />
        <Button type="submit" variant="contained" startIcon={<SaveOutlinedIcon />} disabled={state.pending || (!resetPassword && newPassword.length === 0)}>Save password</Button>
      </Stack>
    </CommandPanel>
  );
}

export function PolicyPanel({ record, onSaved }: PanelProps) {
  const [isAdministrator, setAdministrator] = useState(record.Policy.IsAdministrator);
  const [isDisabled, setDisabled] = useState(record.Policy.IsDisabled);
  const [state, run] = useCommandState();
  const submit = (event: SyntheticEvent<HTMLFormElement, SubmitEvent>) => {
    event.preventDefault();
    void run(async () => {
      await updateUserPolicy(record.id, { isAdministrator, isDisabled });
      await onSaved();
    });
  };
  return (
    <CommandPanel title="Access policy">
      <Stack component="form" onSubmit={submit} spacing={1.5}>
        <FormControlLabel control={<Switch checked={isAdministrator} onChange={(event) => { setAdministrator(event.target.checked); }} disabled={state.pending} />} label="Administrator" />
        <FormControlLabel control={<Switch checked={isDisabled} onChange={(event) => { setDisabled(event.target.checked); }} disabled={state.pending} />} label="Disabled" />
        <CommandError error={state.error} />
        <Button type="submit" variant="contained" startIcon={<SaveOutlinedIcon />} disabled={state.pending}>Save access policy</Button>
      </Stack>
    </CommandPanel>
  );
}

export function DeletePanel({ record, onDeleted }: { record: UserRecord; onDeleted: () => void | Promise<void> }) {
  const [confirming, setConfirming] = useState(false);
  const [state, run] = useCommandState();
  const remove = () => {
    void run(async () => {
      await dataProvider.delete<UserRecord>('users', { id: record.id, previousData: record });
      await onDeleted();
    });
  };
  return (
    <CommandPanel title="Delete user" tone="danger">
      <Stack spacing={2}>
        <Typography variant="body2" color="text.secondary">Deletion is permanent and may be rejected when this is the last enabled administrator.</Typography>
        <CommandError error={state.error} conflictMessage="The last enabled administrator cannot be removed." />
        {confirming ? (
          <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1}>
            <Button color="error" variant="contained" startIcon={<DeleteOutlineIcon />} onClick={remove} disabled={state.pending}>Confirm delete</Button>
            <Button onClick={() => { setConfirming(false); }} disabled={state.pending}>Cancel</Button>
          </Stack>
        ) : (
          <Button color="error" variant="outlined" startIcon={<DeleteOutlineIcon />} onClick={() => { setConfirming(true); }}>Delete user</Button>
        )}
      </Stack>
    </CommandPanel>
  );
}

function UserEditPanels() {
  const record = useRecordContext<UserRecord>();
  const redirect = useRedirect();
  const refresh = useRefresh();
  if (record === undefined) return null;
  return (
    <Stack spacing={2} sx={{ maxWidth: 760 }}>
      <RenamePanel record={record} onSaved={refresh} />
      <PasswordPanel record={record} onSaved={refresh} />
      <PolicyPanel record={record} onSaved={refresh} />
      <DeletePanel record={record} onDeleted={() => { redirect('list', 'users'); }} />
    </Stack>
  );
}

export function UserEdit() {
  return <Edit<UserRecord> title="Edit user" mutationMode="pessimistic" actions={false} component="div"><UserEditPanels /></Edit>;
}

function CommandPanel({ title, tone = 'default', children }: {
  title: string;
  tone?: 'default' | 'danger';
  children: React.ReactNode;
}) {
  return (
    <Box component="section" sx={{ p: { xs: 2, sm: 3 }, border: 1, borderColor: tone === 'danger' ? 'error.light' : 'divider', borderRadius: 1, bgcolor: 'background.paper' }}>
      <Typography component="h2" variant="h3">{title}</Typography>
      <Divider sx={{ my: 2 }} />
      <Box>{children}</Box>
    </Box>
  );
}

function CommandError({ error, conflictMessage }: { error?: unknown; conflictMessage?: string }) {
  if (error === undefined) return null;
  const status = statusOf(error);
  const message = status === 409 && conflictMessage !== undefined
    ? conflictMessage
    : status === 400
      ? 'Check the submitted values and try again.'
      : status === 403
        ? 'Administrator permission is required.'
        : status === 404
          ? 'This user no longer exists.'
          : 'The server could not complete this command.';
  return <Alert severity="error">{message}</Alert>;
}

function useCommandState(): [
  { pending: boolean; error?: unknown },
  (command: () => Promise<void>) => Promise<void>,
] {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>();
  const running = useRef(false);
  const run = async (command: () => Promise<void>) => {
    if (running.current) return;
    running.current = true;
    setPending(true);
    setError(undefined);
    try {
      await command();
    } catch (caught) {
      setError(caught);
    } finally {
      running.current = false;
      setPending(false);
    }
  };
  return [{ pending, error }, run];
}

function statusOf(error: unknown): number | undefined {
  if (typeof error !== 'object' || error === null || !('status' in error)) return undefined;
  return typeof error.status === 'number' ? error.status : undefined;
}
