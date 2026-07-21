import { ThemeProvider } from '@mui/material/styles';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { dataProvider } from '../api/dataProvider';
import type { UserRecord } from '../api/types';
import { theme } from '../theme';
import { updateUserPassword, updateUserPolicy } from './userCommands';
import { DeletePanel, PasswordPanel, PolicyPanel, RenamePanel } from './UserEdit';

vi.mock('../api/dataProvider', () => ({
  dataProvider: { update: vi.fn(), delete: vi.fn() },
}));
vi.mock('./userCommands', () => ({
  updateUserPassword: vi.fn(),
  updateUserPolicy: vi.fn(),
}));

const updateMock = vi.mocked(dataProvider.update);
const deleteMock = vi.mocked(dataProvider.delete);
const passwordMock = vi.mocked(updateUserPassword);
const policyMock = vi.mocked(updateUserPolicy);

const record: UserRecord = {
  Id: 'u2',
  id: 'u2',
  Name: 'Bob',
  ServerId: 'server-id',
  HasPassword: true,
  HasConfiguredPassword: true,
  Configuration: {},
  Policy: {
    IsAdministrator: false,
    IsDisabled: false,
    EnableMediaPlayback: true,
    EnableAudioPlaybackTranscoding: false,
    EnableVideoPlaybackTranscoding: false,
    EnablePlaybackRemuxing: false,
    AuthenticationProviderId: 'TJXY.LocalAuthentication',
    PasswordResetProviderId: 'TJXY.LocalPasswordReset',
  },
};

beforeEach(() => {
  updateMock.mockReset();
  deleteMock.mockReset();
  passwordMock.mockReset();
  policyMock.mockReset();
  updateMock.mockResolvedValue({ data: record });
  deleteMock.mockResolvedValue({ data: record });
  passwordMock.mockResolvedValue(undefined);
  policyMock.mockResolvedValue(undefined);
});

it('rename submits only the rename provider operation', async () => {
  const onSaved = vi.fn();
  render(<ThemeProvider theme={theme}><RenamePanel record={record} onSaved={onSaved} /></ThemeProvider>);
  const user = userEvent.setup();
  await user.clear(screen.getByRole('textbox', { name: 'Name' }));
  await user.type(screen.getByRole('textbox', { name: 'Name' }), 'Robert');
  await user.click(screen.getByRole('button', { name: 'Save name' }));

  expect(updateMock).toHaveBeenCalledWith('users', {
    id: 'u2', data: { Name: 'Robert' }, previousData: record,
  });
  expect(passwordMock).not.toHaveBeenCalled();
  expect(policyMock).not.toHaveBeenCalled();
  expect(onSaved).toHaveBeenCalledOnce();
});

it('prevents duplicate submissions while a command is pending', async () => {
  let resolveUpdate: ((value: { data: UserRecord }) => void) | undefined;
  updateMock.mockImplementation(() => new Promise((resolve) => {
    resolveUpdate = resolve;
  }));
  render(<ThemeProvider theme={theme}><RenamePanel record={record} onSaved={vi.fn()} /></ThemeProvider>);
  const user = userEvent.setup();
  const save = screen.getByRole('button', { name: 'Save name' });

  await Promise.all([user.click(save), user.click(save)]);

  expect(updateMock).toHaveBeenCalledOnce();
  resolveUpdate?.({ data: record });
});

it('password requires matching confirmation and clears inputs after success', async () => {
  const onSaved = vi.fn();
  render(<ThemeProvider theme={theme}><PasswordPanel record={record} onSaved={onSaved} /></ThemeProvider>);
  const user = userEvent.setup();
  await user.type(screen.getByLabelText(/^New password/), 'new password');
  await user.type(screen.getByLabelText(/^Confirm password/), 'different');
  await user.click(screen.getByRole('button', { name: 'Save password' }));
  expect(passwordMock).not.toHaveBeenCalled();
  expect(screen.getByText('Passwords do not match.')).toBeVisible();

  await user.clear(screen.getByLabelText(/^Confirm password/));
  await user.type(screen.getByLabelText(/^Confirm password/), 'new password');
  await user.click(screen.getByRole('button', { name: 'Save password' }));
  expect(passwordMock).toHaveBeenCalledWith('u2', {
    newPassword: 'new password', resetPassword: false,
  });
  expect(screen.getByLabelText(/^New password/)).toHaveValue('');
  expect(onSaved).toHaveBeenCalledOnce();
});

it('policy submits only supported access flags', async () => {
  const onSaved = vi.fn();
  render(<ThemeProvider theme={theme}><PolicyPanel record={record} onSaved={onSaved} /></ThemeProvider>);
  const user = userEvent.setup();
  await user.click(screen.getByRole('switch', { name: 'Administrator' }));
  await user.click(screen.getByRole('button', { name: 'Save access policy' }));

  expect(policyMock).toHaveBeenCalledWith('u2', {
    isAdministrator: true, isDisabled: false,
  });
  expect(passwordMock).not.toHaveBeenCalled();
  expect(updateMock).not.toHaveBeenCalled();
  expect(onSaved).toHaveBeenCalledOnce();
});

it('delete requires confirmation and preserves the record on conflict', async () => {
  const onDeleted = vi.fn();
  deleteMock.mockRejectedValue({ status: 409, category: 'conflict' });
  render(<ThemeProvider theme={theme}><DeletePanel record={record} onDeleted={onDeleted} /></ThemeProvider>);
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: 'Delete user' }));
  expect(deleteMock).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Confirm delete' }));

  expect(deleteMock).toHaveBeenCalledWith('users', { id: 'u2', previousData: record });
  expect(await screen.findByText('The last enabled administrator cannot be removed.')).toBeVisible();
  expect(onDeleted).not.toHaveBeenCalled();
});
