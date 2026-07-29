import type { GetListParams } from 'ra-core';

import { apiRequest } from './httpClient';
import type { TjxyUser, UserRecord } from './types';
import { dataProvider } from './dataProvider';

vi.mock('./httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('./httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);

function user(
  id: string,
  name: string,
  policy: Partial<TjxyUser['Policy']> = {},
): TjxyUser {
  return {
    Id: id,
    Name: name,
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
  };
}

beforeEach(() => {
  requestMock.mockReset();
});

it('maps Id to id, sorts stably, and pages the bounded Users collection', async () => {
  requestMock.mockResolvedValue([
    user('u3', 'Charlie'),
    user('u2', 'alice'),
    user('u1', 'Alice'),
  ]);
  const firstPage = await dataProvider.getList<UserRecord>('users', {
    pagination: { page: 1, perPage: 2 },
    sort: { field: 'Name', order: 'ASC' },
    filter: {},
  });
  const params: GetListParams = {
    pagination: { page: 2, perPage: 2 },
    sort: { field: 'Name', order: 'ASC' },
    filter: {},
  };

  expect(firstPage.data.map(({ Id }) => Id)).toEqual(['u1', 'u2']);
  await expect(dataProvider.getList<UserRecord>('users', params)).resolves.toEqual({
    data: [expect.objectContaining({ Id: 'u3', id: 'u3', Name: 'Charlie' })],
    total: 3,
    meta: { totalUsers: 3, administrators: 0, disabled: 0 },
  });
  expect(requestMock).toHaveBeenCalledWith('/Users', {});
});

it('returns an empty page beyond the collection', async () => {
  requestMock.mockResolvedValue([user('u1', 'Alice')]);

  await expect(dataProvider.getList<UserRecord>('users', {
    pagination: { page: 3, perPage: 25 },
    sort: { field: 'Name', order: 'ASC' },
    filter: {},
  })).resolves.toEqual({
    data: [],
    total: 1,
    meta: { totalUsers: 1, administrators: 0, disabled: 0 },
  });
});

it('searches names and ids case-insensitively after trimming the query', async () => {
  requestMock.mockResolvedValue([
    user('USER-ADA-01', 'Ada Lovelace'),
    user('user-grace-02', 'Grace Hopper'),
    user('user-alan-03', 'Alan Turing'),
  ]);

  const byName = await dataProvider.getList<UserRecord>('users', {
    filter: { q: '  lOvElAcE ' },
    pagination: { page: 1, perPage: 25 },
  });
  const byId = await dataProvider.getList<UserRecord>('users', {
    filter: { q: 'GRACE-02' },
    pagination: { page: 1, perPage: 25 },
  });

  expect(byName.data.map(({ Id }) => Id)).toEqual(['USER-ADA-01']);
  expect(byId.data.map(({ Id }) => Id)).toEqual(['user-grace-02']);
});

it.each([
  ['administrator', ['enabled-admin']],
  ['standard', ['enabled-standard']],
  ['disabled', ['disabled-admin', 'disabled-standard']],
  ['all', ['disabled-admin', 'disabled-standard', 'enabled-admin', 'enabled-standard']],
] as const)('applies the exclusive %s access filter', async (access, expectedIds) => {
  requestMock.mockResolvedValue([
    user('enabled-admin', 'Enabled Admin', { IsAdministrator: true }),
    user('enabled-standard', 'Enabled Standard'),
    user('disabled-admin', 'Disabled Admin', { IsAdministrator: true, IsDisabled: true }),
    user('disabled-standard', 'Disabled Standard', { IsDisabled: true }),
  ]);

  const result = await dataProvider.getList<UserRecord>('users', {
    filter: { access },
    pagination: { page: 1, perPage: 25 },
  });

  expect(result.data.map(({ Id }) => Id).sort()).toEqual([...expectedIds].sort());
  expect(result.meta).toEqual({ totalUsers: 4, administrators: 1, disabled: 2 });
});

it('composes search and access filters before pagination', async () => {
  requestMock.mockResolvedValue([
    user('admin-ada', 'Ada', { IsAdministrator: true }),
    user('admin-grace', 'Grace', { IsAdministrator: true }),
    user('user-ada', 'Ada Standard'),
  ]);

  const result = await dataProvider.getList<UserRecord>('users', {
    filter: { q: 'ada', access: 'administrator' },
    pagination: { page: 1, perPage: 25 },
  });

  expect(result.data.map(({ Id }) => Id)).toEqual(['admin-ada']);
  expect(result.total).toBe(1);
  expect(result.meta).toEqual({ totalUsers: 3, administrators: 2, disabled: 0 });
});

it('computes metadata before filtering and slices the filtered result to 25 rows', async () => {
  requestMock.mockResolvedValue(Array.from({ length: 40 }, (_, index) => user(
    `u-${String(index).padStart(2, '0')}`,
    `User ${String(index).padStart(2, '0')}`,
    index < 3
      ? { IsAdministrator: true }
      : index < 8
        ? { IsDisabled: true }
        : {},
  )));

  const result = await dataProvider.getList<UserRecord>('users', {
    filter: { access: 'standard' },
    pagination: { page: 1, perPage: 25 },
    sort: { field: 'Name', order: 'ASC' },
  });

  expect(result.data).toHaveLength(25);
  expect(result.total).toBe(32);
  expect(result.meta).toEqual({ totalUsers: 40, administrators: 3, disabled: 5 });
});

it('gets one user with an encoded identifier', async () => {
  requestMock.mockResolvedValue(user('user/id', 'Alice'));

  const result = await dataProvider.getOne<UserRecord>('users', { id: 'user/id' });
  expect(result.data.Id).toBe('user/id');
  expect(result.data.id).toBe('user/id');
  expect(requestMock).toHaveBeenCalledWith('/Users/user%2Fid', {});
});

it('creates a user with only supported fields', async () => {
  requestMock.mockResolvedValue(user('u2', 'Bob'));

  const result = await dataProvider.create<{ Name: string; Password: string }, UserRecord>('users', {
    data: { Name: 'Bob', Password: 'bob password' },
  });
  expect(result.data.id).toBe('u2');
  expect(result.data.Name).toBe('Bob');
  expect(requestMock).toHaveBeenCalledWith('/Users/New', {
    method: 'POST',
    body: JSON.stringify({ Name: 'Bob', Password: 'bob password' }),
  });
});

it('renames with one command and refetches the authoritative record', async () => {
  const previous = { ...user('u2', 'Bob'), id: 'u2' };
  requestMock
    .mockResolvedValueOnce(undefined)
    .mockResolvedValueOnce(user('u2', 'Robert'));

  const result = await dataProvider.update<UserRecord>('users', {
    id: 'u2',
    data: { Name: 'Robert' },
    previousData: previous,
  });
  expect(result.data.id).toBe('u2');
  expect(result.data.Name).toBe('Robert');
  expect(requestMock).toHaveBeenNthCalledWith(1, '/Users?userId=u2', {
    method: 'POST',
    body: JSON.stringify({ Name: 'Robert' }),
  });
  expect(requestMock).toHaveBeenNthCalledWith(2, '/Users/u2', {});
});

it('deletes pessimistically and returns the prior record', async () => {
  const previous = { ...user('u2', 'Bob'), id: 'u2' };
  requestMock.mockResolvedValue(undefined);

  await expect(dataProvider.delete<UserRecord>('users', {
    id: 'u2', previousData: previous,
  })).resolves.toEqual({ data: previous });
  expect(requestMock).toHaveBeenCalledWith('/Users/u2', { method: 'DELETE' });
});

it('rejects unsupported resources, filter shapes, sorts, and bulk operations explicitly', async () => {
  await expect(dataProvider.getList('devices', { filter: {} })).rejects.toMatchObject({ status: 405 });
  for (const filter of [
    { IsDisabled: true },
    { q: 42 },
    { q: null },
    { access: true },
    { access: null },
    { access: 'owner' },
    [],
    null,
  ]) {
    await expect(dataProvider.getList('users', { filter }))
      .rejects.toMatchObject({ status: 400, category: 'validation' });
  }
  await expect(dataProvider.getList('users', {
    filter: {}, sort: { field: 'Policy.IsDisabled', order: 'ASC' },
  })).rejects.toMatchObject({ status: 400 });
  await expect(dataProvider.getList('users', {
    filter: {}, sort: { field: 'Name', order: 'SIDEWAYS' } as never,
  })).rejects.toMatchObject({ status: 400 });
  await expect(dataProvider.deleteMany('users', { ids: ['u1'] })).rejects.toMatchObject({
    status: 405,
  });
  expect(requestMock).not.toHaveBeenCalled();
});

it('rejects malformed user records instead of inventing ids', async () => {
  requestMock.mockResolvedValue([{ Name: 'Missing id' }]);

  await expect(dataProvider.getList('users', { filter: {} })).rejects.toMatchObject({
    category: 'invalid-response',
  });
});
