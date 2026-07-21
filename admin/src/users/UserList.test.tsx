import { ThemeProvider } from '@mui/material/styles';
import { render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';

import type { UserRecord } from '../api/types';
import { theme } from '../theme';
import { UserList } from './UserList';

let records: UserRecord[] = [];

vi.mock('react-admin', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react-admin')>();
  return {
    ...actual,
    List: ({ children }: { children: ReactNode }) => <>{children}</>,
    useListContext: () => ({ data: records, isPending: false }),
    ShowButton: ({ record }: { record: UserRecord }) => <button type="button">View {record.Name}</button>,
    EditButton: ({ record }: { record: UserRecord }) => <button type="button">Edit {record.Name}</button>,
  };
});

const user = (overrides: Partial<UserRecord>): UserRecord => ({
  Id: 'user-id',
  id: 'user-id',
  Name: 'Taylor',
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
  ...overrides,
});

beforeEach(() => {
  records = [];
});

it('renders an explicit empty state', () => {
  render(<ThemeProvider theme={theme}><UserList /></ThemeProvider>);
  expect(screen.getByText('No users found.')).toBeVisible();
});

it('renders user access state and named actions in a table', () => {
  records = [user({
    Id: 'admin-id',
    id: 'admin-id',
    Name: 'Admin',
    Policy: { ...user({}).Policy, IsAdministrator: true, IsDisabled: true },
  })];

  render(<ThemeProvider theme={theme}><UserList /></ThemeProvider>);

  expect(screen.getByRole('table', { name: 'Users' })).toBeVisible();
  expect(screen.getByText('Admin')).toBeVisible();
  expect(screen.getByText('Administrator')).toBeVisible();
  expect(screen.getByText('Disabled')).toBeVisible();
  expect(screen.getByRole('button', { name: 'View Admin' })).toBeEnabled();
  expect(screen.getByRole('button', { name: 'Edit Admin' })).toBeEnabled();
});
