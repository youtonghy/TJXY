import type { AuthProvider, UserIdentity } from 'ra-core';

import { ApiError, apiRequest } from '../api/httpClient';
import type { TjxyUser } from '../api/types';
import { clearSession, getAccessToken } from './session';

export const authProvider: AuthProvider = {
  async login() {
    await requireAdministrator();
  },

  async logout() {
    try {
      if (getAccessToken() !== null) await apiRequest('/Sessions/Logout', { method: 'POST' });
    } finally {
      clearSession();
    }
  },

  async checkAuth() {
    await requireAdministrator();
  },

  checkError(error: unknown) {
    if (statusOf(error) === 403) {
      return Promise.reject(new AccessDeniedAuthError());
    }
    if (statusOf(error) === 401) {
      clearSession();
      return Promise.reject(
        error instanceof Error
          ? error
          : new ApiError(401, 'authentication', 'Your session is not valid.'),
      );
    }
    return Promise.resolve();
  },

  async getIdentity(): Promise<UserIdentity> {
    const user = await requireAdministrator();
    return { id: user.Id, fullName: user.Name };
  },

  async getPermissions() {
    await requireAdministrator();
    return 'administrator';
  },
};

class AccessDeniedAuthError extends Error {
  readonly status = 403;
  readonly category = 'authorization';
  readonly logoutUser = false;
  readonly redirectTo = '/admin/access-denied';

  constructor() {
    super('Administrator access is required.');
    this.name = 'AccessDeniedAuthError';
    Object.defineProperty(this, 'message', { configurable: true, value: false });
  }
}

async function requireAdministrator(): Promise<TjxyUser> {
  if (getAccessToken() === null) {
    throw new ApiError(401, 'authentication', 'Your session is not valid.');
  }
  const user = await apiRequest<unknown>('/Users/Me');
  if (!isUser(user)) {
    throw new ApiError(200, 'invalid-response', 'The server returned an invalid response.');
  }
  if (!user.Policy.IsAdministrator || user.Policy.IsDisabled) {
    throw new ApiError(403, 'authorization', 'Administrator access is required.');
  }
  return user;
}

function isUser(value: unknown): value is TjxyUser {
  if (!isRecord(value) || !isRecord(value.Policy)) {
    return false;
  }
  return typeof value.Id === 'string'
    && value.Id.length > 0
    && typeof value.Name === 'string'
    && typeof value.Policy.IsAdministrator === 'boolean'
    && typeof value.Policy.IsDisabled === 'boolean';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function statusOf(error: unknown): number | undefined {
  if (typeof error !== 'object' || error === null || !('status' in error)) {
    return undefined;
  }
  return typeof error.status === 'number' ? error.status : undefined;
}
