import { ApiError, apiRequest } from '../api/httpClient';
import type { TjxyUser } from '../api/types';
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

beforeEach(() => {
  requestMock.mockReset();
  sessionStorage.clear();
  localStorage.clear();
});

it('reuses the shared client session for administrator verification', async () => {
  sessionStorage.setItem('tjxy.web.token', 'issued-token');
  requestMock.mockResolvedValueOnce(user());

  await authProvider.login({});

  expect(requestMock).toHaveBeenCalledWith('/Users/Me');
  expect(sessionStorage.getItem('tjxy.web.token')).toBe('issued-token');
});

it('does not authenticate credentials from the administrator provider', async () => {
  await expect(authProvider.login({ username: 'Admin', password: 'password' })).rejects.toMatchObject({
    status: 401,
    category: 'authentication',
  });
  expect(requestMock).not.toHaveBeenCalled();
});

it('validates a persisted session through current-user on reload', async () => {
  sessionStorage.setItem('tjxy.web.token', 'persisted-token');
  requestMock.mockResolvedValueOnce(user());

  await expect(authProvider.checkAuth({})).resolves.toBeUndefined();
  expect(requestMock).toHaveBeenCalledWith('/Users/Me');
});

it('checks the remember-login cookie when session storage has no token', async () => {
  requestMock.mockRejectedValueOnce(new ApiError(401, 'authentication', 'Invalid.'));
  await expect(authProvider.checkAuth({})).rejects.toMatchObject({
    status: 401,
    category: 'authentication',
  });
  expect(requestMock).toHaveBeenCalledWith('/Users/Me', { auth: 'none' });
});

it('allows administrator verification through the remember-login cookie', async () => {
  requestMock.mockResolvedValueOnce(user());

  await expect(authProvider.checkAuth({})).resolves.toBeUndefined();
  expect(requestMock).toHaveBeenCalledWith('/Users/Me', { auth: 'none' });
});

it('rejects a signed-in non-administrator without clearing the shared session', async () => {
  sessionStorage.setItem('tjxy.web.token', 'viewer-token');
  requestMock.mockResolvedValueOnce(user({
    Policy: { ...user().Policy, IsAdministrator: false },
  }));

  await expect(authProvider.checkAuth({})).rejects.toMatchObject({
    status: 403,
    category: 'authorization',
  });
  expect(sessionStorage.getItem('tjxy.web.token')).toBe('viewer-token');
});

it('preserves authentication and requests access-denied routing on 403', async () => {
  sessionStorage.setItem('tjxy.web.token', 'token');
  await expect(authProvider.checkError(new ApiError(403, 'authorization', 'Forbidden.')))
    .rejects.toMatchObject({
      status: 403,
      logoutUser: false,
      redirectTo: '/admin/access-denied',
      message: false,
    });
  expect(sessionStorage.getItem('tjxy.web.token')).toBe('token');
});

it('clears authentication on 401', async () => {
  sessionStorage.setItem('tjxy.web.token', 'token');
  await expect(authProvider.checkError(new ApiError(401, 'authentication', 'Invalid.')))
    .rejects.toMatchObject({ status: 401 });
  expect(sessionStorage.getItem('tjxy.web.token')).toBeNull();
});

it('returns administrator identity and permissions from current server state', async () => {
  sessionStorage.setItem('tjxy.web.token', 'token');
  requestMock.mockResolvedValue(user());

  await expect(authProvider.getIdentity?.()).resolves.toEqual({
    id: 'admin-id',
    fullName: 'Admin',
  });
  await expect(authProvider.getPermissions?.({})).resolves.toBe('administrator');
});

it('logout clears the shared token and retains the stable device identity', async () => {
  sessionStorage.setItem('tjxy.web.token', 'token');
  localStorage.setItem('tjxy.web.deviceId', 'device-id');

  await expect(authProvider.logout({})).resolves.toBeUndefined();

  expect(sessionStorage.getItem('tjxy.web.token')).toBeNull();
  expect(localStorage.getItem('tjxy.web.deviceId')).toBe('device-id');
  expect(requestMock).toHaveBeenCalledWith('/Sessions/Logout', { method: 'POST' });
});
