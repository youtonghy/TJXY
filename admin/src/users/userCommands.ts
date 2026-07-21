import { ApiError, apiRequest } from '../api/httpClient';

export interface UserPasswordInput {
  newPassword: string;
  resetPassword: boolean;
}

export interface UserPolicyInput {
  isAdministrator: boolean;
  isDisabled: boolean;
}

export async function updateUserPassword(
  id: string,
  input: UserPasswordInput,
): Promise<void> {
  const encodedId = validIdentifier(id);
  await apiRequest(`/Users/${encodedId}/Password`, {
    method: 'POST',
    body: JSON.stringify({
      NewPw: input.newPassword,
      ResetPassword: input.resetPassword,
    }),
  });
}

export async function updateUserPolicy(id: string, input: UserPolicyInput): Promise<void> {
  const encodedId = validIdentifier(id);
  await apiRequest(`/Users/${encodedId}/Policy`, {
    method: 'POST',
    body: JSON.stringify({
      IsAdministrator: input.isAdministrator,
      IsDisabled: input.isDisabled,
      AuthenticationProviderId: 'TJXY.LocalAuthentication',
      PasswordResetProviderId: 'TJXY.LocalPasswordReset',
    }),
  });
}

function validIdentifier(id: string): string {
  if (id.length === 0) {
    throw new ApiError(400, 'validation', 'A user identifier is required.');
  }
  return encodeURIComponent(id);
}
