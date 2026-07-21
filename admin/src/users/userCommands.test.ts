import { apiRequest } from '../api/httpClient';
import { updateUserPassword, updateUserPolicy } from './userCommands';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);

beforeEach(() => {
  requestMock.mockReset();
  requestMock.mockResolvedValue(undefined);
});

it('updates a password through its dedicated command', async () => {
  await updateUserPassword('user/id', {
    newPassword: 'new password',
    resetPassword: false,
  });

  expect(requestMock).toHaveBeenCalledWith('/Users/user%2Fid/Password', {
    method: 'POST',
    body: JSON.stringify({ NewPw: 'new password', ResetPassword: false }),
  });
});

it('updates supported policy flags and pins local providers', async () => {
  await updateUserPolicy('u2', { isAdministrator: true, isDisabled: false });

  expect(requestMock).toHaveBeenCalledWith('/Users/u2/Policy', {
    method: 'POST',
    body: JSON.stringify({
      IsAdministrator: true,
      IsDisabled: false,
      AuthenticationProviderId: 'TJXY.LocalAuthentication',
      PasswordResetProviderId: 'TJXY.LocalPasswordReset',
    }),
  });
});

it('rejects an invalid identifier before making a command request', async () => {
  await expect(updateUserPassword('', { newPassword: 'password', resetPassword: false }))
    .rejects.toMatchObject({ category: 'validation' });
  expect(requestMock).not.toHaveBeenCalled();
});
