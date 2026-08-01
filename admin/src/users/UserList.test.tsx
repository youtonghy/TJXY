import { Toast } from '@heroui/react';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type {
  DataProvider,
  GetListParams,
  GetListResult,
  RaRecord,
} from 'ra-core';
import { useLocation } from 'react-router-dom';

import type { UserListMeta } from '../api/dataProvider';
import type { UserRecord } from '../api/types';
import { renderWithAdmin, strictTestDataProvider } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
import { UserList } from './UserList';

interface UserListResult {
  data: UserRecord[];
  total: number;
  meta: UserListMeta;
}

type UserListRequest = (resource: string, params: GetListParams) => Promise<UserListResult>;

const emptyResult: UserListResult = {
  data: [],
  total: 0,
  meta: { totalUsers: 0, administrators: 0, disabled: 0 },
};

afterEach(() => {
  vi.restoreAllMocks();
});

function userRecord(
  overrides: Partial<UserRecord> = {},
  policy: Partial<UserRecord['Policy']> = {},
): UserRecord {
  return {
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
      ...policy,
    },
    ...overrides,
  };
}

function providerWith(getList: UserListRequest): DataProvider {
  return {
    ...strictTestDataProvider,
    async getList<RecordType extends RaRecord = UserRecord>(
      resource: string,
      params: GetListParams,
    ) {
      return await getList(resource, params) as unknown as GetListResult<RecordType>;
    },
  };
}

function LocationProbe() {
  const location = useLocation();
  return <output aria-label="Current location">{`${location.pathname}${location.search}`}</output>;
}

function renderUsers(getList: UserListRequest, initialEntry = '/admin/users') {
  return renderWithAdmin(
    <>
      <UserList />
      <AdminNotifications />
      <LocationProbe />
    </>,
    {
      dataProvider: providerWith(getList),
      initialEntries: [initialEntry],
    },
  );
}

async function usersGrid() {
  const grid = await screen.findByRole('grid', { name: 'Users' });
  await within(grid).findAllByRole('rowheader');
  return grid;
}

it('shows a stable skeleton during the initial request', () => {
  const getList = vi.fn<UserListRequest>(() => new Promise(() => undefined));
  renderUsers(getList);

  expect(screen.getByRole('status', { name: 'Loading users' })).toBeVisible();
});

it('shows a safe initial error and retries the request', async () => {
  const user = userEvent.setup();
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('users-error-toast');
  const getList = vi.fn<UserListRequest>()
    .mockRejectedValueOnce(new Error('private upstream detail'))
    .mockResolvedValueOnce(emptyResult);
  renderUsers(getList);

  expect(await screen.findByRole('heading', { name: 'Unable to load this content' })).toBeVisible();
  expect(screen.queryByText('private upstream detail')).not.toBeInTheDocument();
  expect(dangerToast).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Retry' }));

  expect(await screen.findByText('No users match the current filters.')).toBeVisible();
  expect(getList).toHaveBeenCalledTimes(2);
});

it('renders an explicit successful empty state without removing the filters', async () => {
  const getList = vi.fn<UserListRequest>().mockResolvedValue(emptyResult);
  renderUsers(getList);

  expect(await screen.findByText('No users match the current filters.')).toBeVisible();
  expect(screen.getByRole('searchbox', { name: 'Search users' })).toBeVisible();
  expect(screen.getByRole('button', { name: /Access/ })).toBeVisible();
});

it('retains existing rows and shows a stale-data alert when a filtered refresh fails', async () => {
  const user = userEvent.setup();
  const ada = userRecord({ Id: 'ada', id: 'ada', Name: 'Ada' });
  const getList = vi.fn<UserListRequest>()
    .mockResolvedValueOnce({
      data: [ada],
      total: 1,
      meta: { totalUsers: 1, administrators: 0, disabled: 0 },
    })
    .mockRejectedValue(new Error('offline'));
  renderUsers(getList);

  const table = await usersGrid();
  await user.type(screen.getByRole('searchbox', { name: 'Search users' }), 'A');

  expect(await screen.findByText('Showing the last available data')).toBeVisible();
  expect(within(table).getByText('Ada')).toBeVisible();
});

it('renders summaries, a named desktop table, complete mobile records, and encoded links', async () => {
  const longId = 'user/with/a/very-long-identifier-that-must-wrap-on-mobile';
  const ada = userRecord(
    { Id: longId, id: longId, Name: 'Ada' },
    { IsAdministrator: true },
  );
  const disabled = userRecord(
    { Id: 'disabled-id', id: 'disabled-id', Name: 'Disabled user' },
    { IsDisabled: true },
  );
  const getList = vi.fn<UserListRequest>().mockResolvedValue({
    data: [ada, disabled],
    total: 2,
    meta: { totalUsers: 3, administrators: 1, disabled: 1 },
  });
  renderUsers(getList);

  const summary = await screen.findByRole('group', { name: 'User summary' });
  expect(summary).toHaveTextContent('3 total users');
  expect(summary).toHaveTextContent('1 enabled administrator');
  expect(summary).toHaveTextContent('1 disabled user');
  expect(screen.getByRole('link', { name: 'Create user' })).toHaveAttribute(
    'href',
    '/admin/users/create',
  );

  const table = await usersGrid();
  expect(within(table).getByRole('columnheader', { name: 'Name' })).toBeVisible();
  expect(within(table).getByText('Administrator')).toBeVisible();
  expect(within(table).getByText('Enabled')).toBeVisible();
  expect(within(table).getByRole('link', { name: 'View Ada' })).toHaveAttribute(
    'href',
    '/admin/users/user%2Fwith%2Fa%2Fvery-long-identifier-that-must-wrap-on-mobile/show',
  );
  expect(within(table).getByRole('link', { name: 'Edit Ada' })).toHaveAttribute(
    'href',
    '/admin/users/user%2Fwith%2Fa%2Fvery-long-identifier-that-must-wrap-on-mobile',
  );

  const mobile = screen.getByRole('list', { name: 'Users mobile' });
  const adaRecord = within(mobile).getByRole('listitem', { name: 'Ada' });
  expect(within(adaRecord).getByText('User ID')).toBeVisible();
  expect(within(adaRecord).getByText('Access')).toBeVisible();
  expect(within(adaRecord).getByText('Status')).toBeVisible();
  expect(within(adaRecord).getByText(longId)).toHaveClass('break-all');
  expect(within(adaRecord).getByRole('link', { name: 'View Ada' })).toBeVisible();
  expect(within(adaRecord).getByRole('link', { name: 'Edit Ada' })).toBeVisible();
});

it('uses ListBase defaults and composes search and access while resetting page one', async () => {
  const user = userEvent.setup();
  const getList = vi.fn<UserListRequest>().mockResolvedValue({
    data: [userRecord({ Id: 'ada', id: 'ada', Name: 'Ada' })],
    total: 50,
    meta: { totalUsers: 50, administrators: 0, disabled: 0 },
  });
  renderUsers(getList, '/admin/users?page=2&perPage=25&sort=Name&order=ASC');

  await waitFor(() => {
    expect(getList).toHaveBeenCalledWith('users', expect.objectContaining({
      filter: {},
      pagination: { page: 2, perPage: 25 },
      sort: { field: 'Name', order: 'ASC' },
    }));
  });

  getList.mockClear();
  await user.type(screen.getByRole('searchbox', { name: 'Search users' }), 'Ada');
  await waitFor(() => {
    expect(getList).toHaveBeenCalledWith('users', expect.objectContaining({
      filter: { q: 'Ada' },
      pagination: { page: 1, perPage: 25 },
    }));
  }, { timeout: 2500 });

  getList.mockClear();
  await user.click(screen.getByRole('button', { name: /Access/ }));
  await user.click(await screen.findByRole('option', { name: 'Disabled' }));
  await waitFor(() => {
    expect(getList).toHaveBeenCalledWith('users', expect.objectContaining({
      filter: { q: 'Ada', access: 'disabled' },
      pagination: { page: 1, perPage: 25 },
    }));
  });
});

it('cancels a pending search update when the access filter changes', async () => {
  const user = userEvent.setup();
  const getList = vi.fn<UserListRequest>().mockResolvedValue({
    data: [userRecord({ Id: 'ada', id: 'ada', Name: 'Ada' })],
    total: 1,
    meta: { totalUsers: 1, administrators: 0, disabled: 0 },
  });
  renderUsers(getList);

  await usersGrid();
  getList.mockClear();
  await user.type(screen.getByRole('searchbox', { name: 'Search users' }), 'Ada');
  await user.click(screen.getByRole('button', { name: /Access/ }));
  await user.click(await screen.findByRole('option', { name: 'Disabled' }));

  await waitFor(() => {
    expect(getList).toHaveBeenCalledWith('users', expect.objectContaining({
      filter: { q: 'Ada', access: 'disabled' },
      pagination: { page: 1, perPage: 25 },
    }));
  });
  await new Promise((resolve) => setTimeout(resolve, 600));
  expect(getList).not.toHaveBeenCalledWith('users', expect.objectContaining({
    filter: { q: 'Ada' },
  }));
});

it('renders HeroUI pagination with a result range and changes pages', async () => {
  const user = userEvent.setup();
  const records = Array.from({ length: 25 }, (_, index) => userRecord({
    Id: `user-${String(index)}`,
    id: `user-${String(index)}`,
    Name: `User ${String(index)}`,
  }));
  const getList = vi.fn<UserListRequest>().mockResolvedValue({
    data: records,
    total: 60,
    meta: { totalUsers: 60, administrators: 0, disabled: 0 },
  });
  renderUsers(getList);

  expect(await screen.findByText('1-25 of 60')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Previous page' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Page 1' })).toBeDisabled();
  const next = screen.getByRole('button', { name: 'Next page' });
  expect(next).toBeEnabled();
  getList.mockClear();
  await user.click(next);

  await waitFor(() => {
    expect(getList).toHaveBeenCalledWith('users', expect.objectContaining({
      pagination: { page: 2, perPage: 25 },
    }));
  });
});

it('marks placeholder rows as updating and disables stale actions until the next page resolves', async () => {
  const user = userEvent.setup();
  const firstPage = Array.from({ length: 25 }, (_, index) => userRecord({
    Id: `first-${String(index)}`,
    id: `first-${String(index)}`,
    Name: index === 0 ? 'First page user' : `First ${String(index)}`,
  }));
  const secondPage = [userRecord({
    Id: 'second-page',
    id: 'second-page',
    Name: 'Second page user',
  })];
  let resolveSecondPage: ((value: UserListResult) => void) | undefined;
  const getList = vi.fn<UserListRequest>()
    .mockResolvedValueOnce({
      data: firstPage,
      total: 26,
      meta: { totalUsers: 26, administrators: 0, disabled: 0 },
    })
    .mockImplementationOnce(() => new Promise<UserListResult>((resolve) => {
      resolveSecondPage = resolve;
    }));
  renderUsers(getList);

  const table = await usersGrid();
  expect(within(table).getByText('First page user')).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Next page' }));

  const updating = await screen.findByText('Updating user results...');
  expect(updating).toHaveAttribute('role', 'status');
  expect(screen.getByText('1-25 of 26')).toBeVisible();
  expect(screen.queryByRole('link', { name: 'View First page user' })).not.toBeInTheDocument();
  expect(within(table).getByLabelText('View First page user unavailable while updating')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Next page' })).toBeDisabled();

  resolveSecondPage?.({
    data: secondPage,
    total: 26,
    meta: { totalUsers: 26, administrators: 0, disabled: 0 },
  });
  await waitFor(() => {
    expect(within(table).getByText('Second page user')).toBeVisible();
  });
  expect(screen.queryByText('Updating user results...')).not.toBeInTheDocument();
  expect(screen.getByText('26-26 of 26')).toBeVisible();
  expect(within(table).getByRole('link', { name: 'View Second page user' })).toBeVisible();
});
