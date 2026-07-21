import type { GetListParams } from 'react-admin';

import { apiRequest } from './httpClient';
import type { TjxyUser, UserRecord } from './types';
import { dataProvider } from './dataProvider';

vi.mock('./httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('./httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);

function user(id: string, name: string): TjxyUser {
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
  const params: GetListParams = {
    pagination: { page: 2, perPage: 2 },
    sort: { field: 'Name', order: 'ASC' },
    filter: {},
  };

  await expect(dataProvider.getList<UserRecord>('users', params)).resolves.toEqual({
    data: [expect.objectContaining({ Id: 'u3', id: 'u3', Name: 'Charlie' })],
    total: 3,
  });
  expect(requestMock).toHaveBeenCalledWith('/Users', {});
});

it('returns an empty page beyond the collection', async () => {
  requestMock.mockResolvedValue([user('u1', 'Alice')]);

  await expect(dataProvider.getList<UserRecord>('users', {
    pagination: { page: 3, perPage: 25 },
    sort: { field: 'Name', order: 'ASC' },
    filter: {},
  })).resolves.toEqual({ data: [], total: 1 });
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

it('rejects unsupported resources, filters, sorts, and bulk operations explicitly', async () => {
  await expect(dataProvider.getList('devices', { filter: {} })).rejects.toMatchObject({ status: 405 });
  await expect(dataProvider.getList('users', { filter: { IsDisabled: true } }))
    .rejects.toMatchObject({ status: 400 });
  await expect(dataProvider.getList('users', {
    filter: {}, sort: { field: 'Policy.IsDisabled', order: 'ASC' },
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
