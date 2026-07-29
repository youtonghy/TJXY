import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type {
  CreateParams,
  CreateResult,
  DataProvider,
  Identifier,
  RaRecord,
} from 'ra-core';
import { Route, Routes, useLocation } from 'react-router-dom';

import type { UserRecord } from '../api/types';
import { renderWithAdmin, strictTestDataProvider } from '../test/renderWithAdmin';
import { UserCreate } from './UserCreate';

const createdUser: UserRecord = {
  Id: 'user/id',
  id: 'user/id',
  Name: 'Ada',
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

function LocationProbe() {
  const location = useLocation();
  return <output aria-label="Current location">{location.pathname}</output>;
}

type CreateUser = (resource: string, params: CreateParams) => Promise<CreateResult<UserRecord>>;

function providerWith(createUser: CreateUser): DataProvider {
  return {
    ...strictTestDataProvider,
    /* eslint-disable @typescript-eslint/no-unnecessary-type-parameters -- DataProvider generic contract. */
    async create<
      RecordType extends Omit<RaRecord, 'id'> = Omit<UserRecord, 'id'>,
      ResultRecordType extends RaRecord = RecordType & { id: Identifier },
    >(resource: string, params: CreateParams) {
      return await createUser(resource, params) as unknown as CreateResult<ResultRecordType>;
    },
    /* eslint-enable @typescript-eslint/no-unnecessary-type-parameters */
  };
}

function renderCreate(create: CreateUser) {
  renderWithAdmin(
    <>
      <Routes>
        <Route element={<UserCreate />} path="/admin/users/create" />
        <Route element={<h1>Created user</h1>} path="/admin/users/:id/show" />
      </Routes>
      <LocationProbe />
    </>,
    {
      dataProvider: providerWith(create),
      initialEntries: ['/admin/users/create'],
    },
  );
}

it('validates required create fields before submitting', async () => {
  const user = userEvent.setup();
  const create = vi.fn<CreateUser>();
  renderCreate(create);

  expect(screen.getByRole('heading', { level: 1, name: 'Create user' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Users' })).toHaveAttribute('href', '/admin/users');
  await user.click(screen.getByRole('button', { name: 'Create user' }));

  expect(await screen.findByText('Name is required.')).toBeVisible();
  expect(screen.getByText('Password is required.')).toBeVisible();
  expect(create).not.toHaveBeenCalled();
});

it('prevents duplicate create submissions and redirects to the encoded show route', async () => {
  const user = userEvent.setup();
  let resolveCreate: ((value: { data: UserRecord }) => void) | undefined;
  const create = vi.fn<CreateUser>(() => new Promise<CreateResult<UserRecord>>((resolve) => {
    resolveCreate = resolve;
  }));
  renderCreate(create);

  await user.type(screen.getByRole('textbox', { name: 'Name' }), 'Ada');
  await user.type(screen.getByLabelText('Initial password'), 'initial password');
  const submit = screen.getByRole('button', { name: 'Create user' });
  await user.click(submit);

  expect(create).toHaveBeenCalledWith('users', expect.objectContaining({
    data: { Name: 'Ada', Password: 'initial password' },
  }));
  expect(submit).toBeDisabled();
  await user.click(submit);
  expect(create).toHaveBeenCalledOnce();

  resolveCreate?.({ data: createdUser });
  expect(await screen.findByRole('heading', { name: 'Created user' })).toBeVisible();
  expect(screen.getByLabelText('Current location')).toHaveTextContent(
    '/admin/users/user%2Fid/show',
  );
});

it('retains the create draft and replaces private server details with a safe error', async () => {
  const user = userEvent.setup();
  const create = vi.fn<CreateUser>().mockRejectedValue(
    Object.assign(new Error('database host and token'), { status: 500 }),
  );
  renderCreate(create);

  await user.type(screen.getByRole('textbox', { name: 'Name' }), 'Ada');
  await user.type(screen.getByLabelText('Initial password'), 'initial password');
  await user.click(screen.getByRole('button', { name: 'Create user' }));

  expect(await screen.findByText('The server could not create this user.')).toBeVisible();
  expect(screen.queryByText('database host and token')).not.toBeInTheDocument();
  expect(screen.getByRole('textbox', { name: 'Name' })).toHaveValue('Ada');
  expect(screen.getByLabelText('Initial password')).toHaveValue('initial password');
  await waitFor(() => {
    expect(screen.getByRole('button', { name: 'Create user' })).toBeEnabled();
  });
});
