import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { DataProvider, GetOneParams, GetOneResult, RaRecord } from 'ra-core';
import { Route, Routes } from 'react-router-dom';

import type { UserRecord } from '../api/types';
import { renderWithAdmin, strictTestDataProvider } from '../test/renderWithAdmin';
import { UserShow } from './UserShow';

const record: UserRecord = {
  Id: 'user/id',
  id: 'user/id',
  Name: 'Ada',
  ServerId: 'server-id',
  HasPassword: true,
  HasConfiguredPassword: true,
  Configuration: {},
  Policy: {
    IsAdministrator: true,
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

function providerWith(getOne: GetOne): DataProvider {
  return {
    ...strictTestDataProvider,
    async getOne<RecordType extends RaRecord = UserRecord>(resource: string, params: GetOneParams) {
      return await getOne(resource, params) as unknown as GetOneResult<RecordType>;
    },
  };
}

function renderShow(getOne: GetOne) {
  return renderWithAdmin(
    <Routes>
      <Route element={<UserShow />} path="/admin/users/:id/show" />
    </Routes>,
    {
      dataProvider: providerWith(getOne),
      initialEntries: ['/admin/users/user%2Fid/show'],
    },
  );
}

it('shows a stable skeleton while the user is loading', () => {
  renderShow(() => new Promise(() => undefined));

  expect(screen.getByRole('status', { name: 'Loading user' })).toBeVisible();
});

it('shows a safe initial error and retries without leaving the deep link', async () => {
  const user = userEvent.setup();
  const getOne = vi.fn<GetOne>()
    .mockRejectedValueOnce(new Error('private upstream error'))
    .mockResolvedValueOnce({ data: record });
  renderShow(getOne);

  expect(await screen.findByRole('heading', { name: 'Unable to load this content' })).toBeVisible();
  expect(screen.queryByText('private upstream error')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Retry' }));

  expect(await screen.findByRole('heading', { level: 1, name: 'Ada' })).toBeVisible();
  expect(getOne).toHaveBeenCalledTimes(2);
});

it('renders one page heading, breadcrumbs, read-only fields, status, and an edit deep link', async () => {
  renderShow(vi.fn<GetOne>().mockResolvedValue({ data: record }));

  const heading = await screen.findByRole('heading', { level: 1, name: 'Ada' });
  expect(screen.getAllByRole('heading', { level: 1 })).toEqual([heading]);
  const breadcrumb = screen.getByRole('navigation', { name: 'Breadcrumb' });
  expect(within(breadcrumb).getByRole('link', { name: 'Users' })).toHaveAttribute(
    'href',
    '/admin/users',
  );
  expect(within(breadcrumb).getByText('Ada')).toHaveAttribute('aria-current', 'page');

  const details = screen.getByRole('group', { name: 'User details' });
  expect(details).toHaveTextContent('NameAda');
  expect(details).toHaveTextContent('User IDuser/id');
  expect(details).toHaveTextContent('Administrator');
  expect(details).toHaveTextContent('Enabled');
  expect(details).toHaveTextContent('PasswordConfigured');
  expect(screen.getByRole('link', { name: 'Edit user' })).toHaveAttribute(
    'href',
    '/admin/users/user%2Fid',
  );
});
