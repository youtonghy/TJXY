import { ApiError, apiRequest } from '../api/httpClient';
import type { AuthenticationResult, TjxyUser } from '../api/types';
import { authProvider } from './authProvider';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);

function user(overrides: Partial<TjxyUser> = {}): TjxyUser {
  return {
    Name: 'Admin',
    ServerId: 'server-id',
    Id: 'admin-id',
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
    ...overrides,
  };
}

function authentication(authenticatedUser = user()): AuthenticationResult {
  return {
    User: authenticatedUser,
    SessionInfo: {
      Id: 'session-id',
      UserId: authenticatedUser.Id,
      UserName: authenticatedUser.Name,
      Client: 'TJXY Admin',
      DeviceId: 'device-id',
      DeviceName: 'Browser',
      ApplicationVersion: '0.1.0',
      ServerId: 'server-id',
      IsActive: true,
      PlayableMediaTypes: [],
      SupportedCommands: [],
    },
    AccessToken: 'issued-token',
    ServerId: 'server-id',
  };
}

beforeEach(() => {
  requestMock.mockReset();
});

it('logs in and retains a token only after current-user administrator verification', async () => {
  requestMock
    .mockResolvedValueOnce(authentication())
    .mockResolvedValueOnce(user());

  await authProvider.login({ username: 'Admin', password: 'correct horse' });

  expect(requestMock).toHaveBeenNthCalledWith(1, '/Users/AuthenticateByName', {
    auth: 'identity',
    method: 'POST',
    body: JSON.stringify({ Username: 'Admin', Pw: 'correct horse' }),
  });
  expect(requestMock).toHaveBeenNthCalledWith(2, '/Users/Me');
  expect(sessionStorage.getItem('tjxy.admin.token')).toBe('issued-token');
});

it.each([
  ['non-administrator', user({ Policy: { ...user().Policy, IsAdministrator: false } })],
  ['disabled administrator', user({ Policy: { ...user().Policy, IsDisabled: true } })],
])('rejects a %s and clears the temporary token', async (_label, currentUser) => {
  requestMock
    .mockResolvedValueOnce(authentication(currentUser))
    .mockResolvedValueOnce(currentUser);

  await expect(authProvider.login({ username: currentUser.Name, password: 'password' }))
    .rejects.toMatchObject({ status: 403, category: 'authorization' });
  expect(sessionStorage.getItem('tjxy.admin.token')).toBeNull();
});

it('clears a temporary token when current-user verification fails', async () => {
  requestMock
    .mockResolvedValueOnce(authentication())
    .mockRejectedValueOnce(new ApiError(401, 'authentication', 'Session invalid.'));

  await expect(authProvider.login({ username: 'Admin', password: 'wrong' })).rejects.toMatchObject({
    status: 401,
  });
  expect(sessionStorage.getItem('tjxy.admin.token')).toBeNull();
});

it('validates a persisted session through current-user on reload', async () => {
  sessionStorage.setItem('tjxy.admin.token', 'persisted-token');
  requestMock.mockResolvedValueOnce(user());

  await expect(authProvider.checkAuth({})).resolves.toBeUndefined();
  expect(requestMock).toHaveBeenCalledWith('/Users/Me');
});

it('rejects checkAuth without a token before making a request', async () => {
  await expect(authProvider.checkAuth({})).rejects.toMatchObject({
    status: 401,
    category: 'authentication',
  });
  expect(requestMock).not.toHaveBeenCalled();
});

it('clears authentication on 401 but preserves it on 403', async () => {
  sessionStorage.setItem('tjxy.admin.token', 'token');
  await expect(authProvider.checkError(new ApiError(403, 'authorization', 'Forbidden.')))
    .resolves.toBeUndefined();
  expect(sessionStorage.getItem('tjxy.admin.token')).toBe('token');

  await expect(authProvider.checkError(new ApiError(401, 'authentication', 'Invalid.')))
    .rejects.toMatchObject({ status: 401 });
  expect(sessionStorage.getItem('tjxy.admin.token')).toBeNull();
});

it('returns administrator identity and permissions from current server state', async () => {
  sessionStorage.setItem('tjxy.admin.token', 'token');
  requestMock.mockResolvedValue(user());

  await expect(authProvider.getIdentity?.()).resolves.toEqual({
    id: 'admin-id',
    fullName: 'Admin',
  });
  await expect(authProvider.getPermissions?.({})).resolves.toBe('administrator');
});

it('logout clears token and device identity without a server request', async () => {
  sessionStorage.setItem('tjxy.admin.token', 'token');
  sessionStorage.setItem('tjxy.admin.deviceId', 'device-id');

  await expect(authProvider.logout({})).resolves.toBeUndefined();

  expect(sessionStorage.getItem('tjxy.admin.token')).toBeNull();
  expect(sessionStorage.getItem('tjxy.admin.deviceId')).toBeNull();
  expect(requestMock).not.toHaveBeenCalled();
});
