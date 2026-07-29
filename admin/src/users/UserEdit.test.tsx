import { Toast } from '@heroui/react';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type {
  DataProvider,
  DeleteParams,
  DeleteResult,
  GetOneParams,
  GetOneResult,
  RaRecord,
  UpdateParams,
  UpdateResult,
} from 'ra-core';
import { Route, Routes } from 'react-router-dom';

import type { UserRecord } from '../api/types';
import { renderWithAdmin, strictTestDataProvider } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
import { UserEdit } from './UserEdit';
import { updateUserPassword, updateUserPolicy } from './userCommands';

vi.mock('./userCommands', () => ({
  updateUserPassword: vi.fn(),
  updateUserPolicy: vi.fn(),
}));

const passwordMock = vi.mocked(updateUserPassword);
const policyMock = vi.mocked(updateUserPolicy);

const record: UserRecord = {
  Id: 'user/id',
  id: 'user/id',
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

type GetOne = (resource: string, params: GetOneParams) => Promise<GetOneResult<UserRecord>>;
type UpdateUser = (resource: string, params: UpdateParams<UserRecord>) => Promise<UpdateResult<UserRecord>>;
type DeleteUser = (resource: string, params: DeleteParams<UserRecord>) => Promise<DeleteResult<UserRecord>>;

function providerWith({
  getOne,
  update,
  remove,
}: {
  getOne: GetOne;
  update: UpdateUser;
  remove: DeleteUser;
}): DataProvider {
  return {
    ...strictTestDataProvider,
    async getOne<RecordType extends RaRecord = UserRecord>(resource: string, params: GetOneParams) {
      return await getOne(resource, params) as unknown as GetOneResult<RecordType>;
    },
    async update<RecordType extends RaRecord = UserRecord>(
      resource: string,
      params: UpdateParams,
    ) {
      return await update(resource, params as UpdateParams<UserRecord>) as unknown as UpdateResult<RecordType>;
    },
    async delete<RecordType extends RaRecord = UserRecord>(
      resource: string,
      params: DeleteParams,
    ) {
      return await remove(resource, params as DeleteParams<UserRecord>) as unknown as DeleteResult<RecordType>;
    },
  };
}

function renderEdit({
  getOne = vi.fn<GetOne>().mockResolvedValue({ data: record }),
  update = vi.fn<UpdateUser>().mockResolvedValue({ data: record }),
  remove = vi.fn<DeleteUser>().mockResolvedValue({ data: record }),
}: Partial<{
  getOne: GetOne;
  update: UpdateUser;
  remove: DeleteUser;
}> = {}) {
  const view = renderWithAdmin(
    <>
      <Routes>
        <Route element={<UserEdit />} path="/admin/users/:id" />
        <Route element={<h1>User list route</h1>} path="/admin/users" />
      </Routes>
      <AdminNotifications />
    </>,
    {
      dataProvider: providerWith({ getOne, update, remove }),
      initialEntries: ['/admin/users/user%2Fid'],
    },
  );
  return { view, getOne, update, remove };
}

beforeEach(() => {
  passwordMock.mockReset();
  policyMock.mockReset();
  passwordMock.mockResolvedValue(undefined);
  policyMock.mockResolvedValue(undefined);
});

afterEach(() => {
  vi.restoreAllMocks();
});

it('shows a stable skeleton during the initial user request', () => {
  renderEdit({ getOne: () => new Promise(() => undefined) });

  expect(screen.getByRole('status', { name: 'Loading user editor' })).toBeVisible();
});

it('shows a safe initial error and retries the same deep link', async () => {
  const user = userEvent.setup();
  const getOne = vi.fn<GetOne>()
    .mockRejectedValueOnce(new Error('private database detail'))
    .mockResolvedValueOnce({ data: record });
  renderEdit({ getOne });

  expect(await screen.findByRole('heading', { name: 'Unable to load this content' })).toBeVisible();
  expect(screen.queryByText('private database detail')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Retry' }));

  expect(await screen.findByRole('heading', { level: 1, name: 'Edit Bob' })).toBeVisible();
  expect(getOne).toHaveBeenCalledTimes(2);
});

it('renders one H1, durable breadcrumbs, and the four unframed command sections', async () => {
  renderEdit();

  const heading = await screen.findByRole('heading', { level: 1, name: 'Edit Bob' });
  expect(screen.getAllByRole('heading', { level: 1 })).toEqual([heading]);
  const breadcrumb = screen.getByRole('navigation', { name: 'Breadcrumb' });
  expect(within(breadcrumb).getByRole('link', { name: 'Users' })).toHaveAttribute(
    'href',
    '/admin/users',
  );
  expect(screen.getByText('user/id')).toHaveClass('break-all');
  expect(screen.getAllByRole('heading', { level: 2 }).map((item) => item.textContent)).toEqual([
    'Identity',
    'Access policy',
    'Password',
    'Danger zone',
  ]);
});

it('retains the current record and identity draft when a manual reload fails', async () => {
  const user = userEvent.setup();
  const getOne = vi.fn<GetOne>()
    .mockResolvedValueOnce({ data: record })
    .mockRejectedValueOnce(new Error('offline'));
  renderEdit({ getOne });

  const name = await screen.findByRole('textbox', { name: 'Name' });
  await user.clear(name);
  await user.type(name, 'Robert draft');
  await user.click(screen.getByRole('button', { name: 'Reload user' }));

  expect(await screen.findByText('Showing the last available data')).toBeVisible();
  expect(name).toHaveValue('Robert draft');
  expect(screen.getByText('user/id')).toBeVisible();
});

it('renames through the provider and keeps unrelated commands enabled while pending', async () => {
  const user = userEvent.setup();
  let resolveUpdate: ((result: UpdateResult<UserRecord>) => void) | undefined;
  const update = vi.fn<UpdateUser>(() => new Promise((resolve) => {
    resolveUpdate = resolve;
  }));
  renderEdit({ update });

  const name = await screen.findByRole('textbox', { name: 'Name' });
  await user.clear(name);
  await user.type(name, 'Robert');
  await user.click(screen.getByRole('button', { name: 'Save identity' }));

  expect(update).toHaveBeenCalledWith('users', {
    id: 'user/id',
    data: { Name: 'Robert' },
    previousData: record,
  });
  expect(screen.getByRole('button', { name: 'Save identity' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Save access policy' })).toBeEnabled();
  expect(screen.getByRole('switch', { name: 'Administrator' })).toBeEnabled();
  expect(passwordMock).not.toHaveBeenCalled();
  expect(policyMock).not.toHaveBeenCalled();

  resolveUpdate?.({ data: { ...record, Name: 'Robert' } });
  await waitFor(() => {
    expect(screen.getByRole('button', { name: 'Save identity' })).toBeEnabled();
  });
});

it('keeps the identity draft and hides private details when rename fails', async () => {
  const user = userEvent.setup();
  const update = vi.fn<UpdateUser>().mockRejectedValue(
    Object.assign(new Error('private database constraint'), { status: 500 }),
  );
  renderEdit({ update });

  const name = await screen.findByRole('textbox', { name: 'Name' });
  await user.clear(name);
  await user.type(name, 'Robert draft');
  await user.click(screen.getByRole('button', { name: 'Save identity' }));

  expect(await screen.findByText('The server could not complete this command.')).toBeVisible();
  expect(screen.queryByText('private database constraint')).not.toBeInTheDocument();
  expect(name).toHaveValue('Robert draft');
});

it('validates password confirmation, clears secrets after success, and never notifies plaintext', async () => {
  const user = userEvent.setup();
  const successToast = vi.spyOn(Toast.toast, 'success').mockReturnValue('password-toast');
  renderEdit();
  const password = await screen.findByLabelText('New password');
  const confirmation = screen.getByLabelText('Confirm password');

  await user.type(password, 'new private password');
  await user.type(confirmation, 'different');
  await user.click(screen.getByRole('button', { name: 'Save password' }));
  expect(passwordMock).not.toHaveBeenCalled();
  expect(screen.getByText('Passwords do not match.')).toBeVisible();

  await user.clear(confirmation);
  await user.type(confirmation, 'new private password');
  await user.click(screen.getByRole('button', { name: 'Save password' }));

  expect(passwordMock).toHaveBeenCalledWith('user/id', {
    newPassword: 'new private password',
    resetPassword: false,
  });
  await waitFor(() => {
    expect(password).toHaveValue('');
    expect(confirmation).toHaveValue('');
  });
  await waitFor(() => {
    expect(successToast).toHaveBeenCalled();
  });
  expect(JSON.stringify(successToast.mock.calls)).not.toContain('new private password');
});

it('updates only supported policy flags and refreshes authoritative data', async () => {
  const user = userEvent.setup();
  const getOne = vi.fn<GetOne>().mockResolvedValue({ data: record });
  renderEdit({ getOne });

  await screen.findByRole('heading', { name: 'Edit Bob' });
  getOne.mockClear();
  await user.click(screen.getByRole('switch', { name: 'Administrator' }));
  await user.click(screen.getByRole('button', { name: 'Save access policy' }));

  expect(policyMock).toHaveBeenCalledWith('user/id', {
    isAdministrator: true,
    isDisabled: false,
  });
  await waitFor(() => {
    expect(getOne).toHaveBeenCalled();
  });
  expect(passwordMock).not.toHaveBeenCalled();
});

it('keeps the named delete confirmation open and explains a last-administrator conflict', async () => {
  const user = userEvent.setup();
  const remove = vi.fn<DeleteUser>().mockRejectedValue({ status: 409, category: 'conflict' });
  renderEdit({ remove });

  await screen.findByRole('heading', { name: 'Edit Bob' });
  await user.click(screen.getByRole('button', { name: 'Delete user' }));
  const dialog = screen.getByRole('dialog', { name: 'Delete Bob?' });
  expect(dialog).toHaveTextContent('Bob');
  expect(remove).not.toHaveBeenCalled();
  await waitFor(() => {
    expect(within(dialog).getByRole('button', { name: 'Cancel' })).toHaveFocus();
  });
  await user.click(within(dialog).getByRole('button', { name: 'Delete user' }));

  expect(remove).toHaveBeenCalledWith('users', { id: 'user/id', previousData: record });
  expect(await within(dialog).findByText(
    'The last enabled administrator cannot be deleted.',
  )).toBeVisible();
  expect(dialog).toBeVisible();
  expect(screen.queryByRole('heading', { name: 'User list route' })).not.toBeInTheDocument();
});

it('redirects to the users index after confirmed deletion', async () => {
  const user = userEvent.setup();
  const remove = vi.fn<DeleteUser>().mockResolvedValue({ data: record });
  renderEdit({ remove });

  await screen.findByRole('heading', { name: 'Edit Bob' });
  await user.click(screen.getByRole('button', { name: 'Delete user' }));
  await user.click(screen.getByRole('button', { name: 'Delete user' }));

  expect(await screen.findByRole('heading', { name: 'User list route' })).toBeVisible();
});
