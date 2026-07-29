import type {
  CreateParams,
  DataProvider,
  DeleteParams,
  GetListParams,
  GetOneParams,
  Identifier,
  RaRecord,
  UpdateParams,
} from 'ra-core';

import { ApiError, apiRequest } from './httpClient';
import type { TjxyUser, UserRecord } from './types';

const RESOURCE = 'users';

export type UserAccessFilter = 'all' | 'administrator' | 'standard' | 'disabled';

export interface UserListFilter {
  q?: string;
  access?: UserAccessFilter;
}

export interface UserListMeta {
  totalUsers: number;
  administrators: number;
  disabled: number;
}

/* React Admin's DataProvider contract requires generic methods so callers select record types. */
/* eslint-disable @typescript-eslint/no-unnecessary-type-parameters */
export const dataProvider: DataProvider = {
  async getList<RecordType extends RaRecord = UserRecord>(resource: string, params: GetListParams) {
    requireResource(resource);
    const filter = parseUserListFilter(params.filter);
    const sort = validatedSort(params);
    const { page, perPage } = pagination(params);
    const users = await fetchUsers(params.signal);
    const meta = summarizeUsers(users);
    const filtered = filterUsers(users, filter);
    const sorted = sortUsers(filtered, sort);
    const start = (page - 1) * perPage;
    return {
      data: sorted.slice(start, start + perPage) as unknown as RecordType[],
      total: sorted.length,
      meta,
    };
  },

  async getOne<RecordType extends RaRecord = UserRecord>(resource: string, params: GetOneParams) {
    requireResource(resource);
    return { data: await fetchUser(params.id as unknown, params.signal) as unknown as RecordType };
  },

  async create<RecordType extends Omit<RaRecord, 'id'> = Omit<UserRecord, 'id'>,
    ResultRecordType extends RaRecord = RecordType & { id: Identifier }>(
    resource: string,
    params: CreateParams,
  ) {
    requireResource(resource);
    const data = requireCreateData(params.data);
    const created = await apiRequest<unknown>('/Users/New', {
      method: 'POST',
      body: JSON.stringify(data),
    });
    return { data: toAdminUser(created) as unknown as ResultRecordType };
  },

  async update<RecordType extends RaRecord = UserRecord>(resource: string, params: UpdateParams) {
    requireResource(resource);
    const name = requireName(params.data);
    const id = encodedIdentifier(params.id as unknown);
    await apiRequest(`/Users?userId=${id}`, {
      method: 'POST',
      body: JSON.stringify({ Name: name }),
    });
    return { data: await fetchUser(params.id as unknown) as unknown as RecordType };
  },

  async delete<RecordType extends RaRecord = UserRecord>(resource: string, params: DeleteParams) {
    requireResource(resource);
    if (params.previousData === undefined) {
      throw new ApiError(400, 'validation', 'The current user record is required for deletion.');
    }
    await apiRequest(`/Users/${encodedIdentifier(params.id as unknown)}`, { method: 'DELETE' });
    return { data: params.previousData as unknown as RecordType };
  },

  getMany: unsupported,
  getManyReference: unsupported,
  updateMany: unsupported,
  deleteMany: unsupported,
};
/* eslint-enable @typescript-eslint/no-unnecessary-type-parameters */

export function toAdminUser(value: unknown): UserRecord {
  if (!isTjxyUser(value)) {
    throw new ApiError(200, 'invalid-response', 'The server returned an invalid user.');
  }
  return { ...value, id: value.Id };
}

async function fetchUsers(signal?: AbortSignal): Promise<UserRecord[]> {
  const value = await apiRequest<unknown>('/Users', signal === undefined ? {} : { signal });
  if (!Array.isArray(value)) {
    throw new ApiError(200, 'invalid-response', 'The server returned an invalid user list.');
  }
  return value.map(toAdminUser);
}

async function fetchUser(id: unknown, signal?: AbortSignal): Promise<UserRecord> {
  const value = await apiRequest<unknown>(
    `/Users/${encodedIdentifier(id)}`,
    signal === undefined ? {} : { signal },
  );
  return toAdminUser(value);
}

function validatedSort(params: GetListParams): { field: 'Name'; order: 'ASC' | 'DESC' } {
  const sort = params.sort ?? { field: 'Name', order: 'ASC' as const };
  const order: unknown = sort.order;
  if (sort.field !== 'Name' || !isSortOrder(order)) {
    throw new ApiError(400, 'validation', 'Users can only be sorted by name.');
  }
  return { field: 'Name', order };
}

function isSortOrder(value: unknown): value is 'ASC' | 'DESC' {
  return value === 'ASC' || value === 'DESC';
}

function sortUsers(
  users: UserRecord[],
  sort: { field: 'Name'; order: 'ASC' | 'DESC' },
): UserRecord[] {
  const direction = sort.order === 'ASC' ? 1 : -1;
  return [...users].sort((left, right) => {
    const names = left.Name.localeCompare(right.Name, undefined, { sensitivity: 'base' });
    return (names === 0 ? left.Id.localeCompare(right.Id) : names) * direction;
  });
}

function pagination(params: GetListParams): { page: number; perPage: number } {
  const page = params.pagination?.page ?? 1;
  const perPage = params.pagination?.perPage ?? 25;
  if (!Number.isSafeInteger(page) || page < 1 || !Number.isSafeInteger(perPage) || perPage < 1) {
    throw new ApiError(400, 'validation', 'Pagination values must be positive integers.');
  }
  return { page, perPage };
}

function requireResource(resource: string): void {
  if (resource !== RESOURCE) {
    throw unsupportedError();
  }
}

function parseUserListFilter(filter: unknown): Required<UserListFilter> {
  if (filter === undefined) {
    return { q: '', access: 'all' };
  }
  if (!isRecord(filter) || Object.keys(filter).some((key) => key !== 'q' && key !== 'access')) {
    throw invalidFilterError();
  }

  const q = filter.q === undefined ? '' : filter.q;
  const access = filter.access === undefined ? 'all' : filter.access;
  if (typeof q !== 'string' || !isUserAccessFilter(access)) throw invalidFilterError();
  return { q: q.trim().toLocaleLowerCase(), access };
}

function isUserAccessFilter(value: unknown): value is UserAccessFilter {
  return value === 'all'
    || value === 'administrator'
    || value === 'standard'
    || value === 'disabled';
}

function invalidFilterError(): ApiError {
  return new ApiError(400, 'validation', 'User filters are invalid.');
}

function filterUsers(users: UserRecord[], filter: Required<UserListFilter>): UserRecord[] {
  return users.filter((user) => {
    const matchesQuery = filter.q.length === 0
      || user.Name.toLocaleLowerCase().includes(filter.q)
      || user.Id.toLocaleLowerCase().includes(filter.q);
    if (!matchesQuery) return false;

    switch (filter.access) {
      case 'administrator':
        return !user.Policy.IsDisabled && user.Policy.IsAdministrator;
      case 'standard':
        return !user.Policy.IsDisabled && !user.Policy.IsAdministrator;
      case 'disabled':
        return user.Policy.IsDisabled;
      case 'all':
        return true;
    }
  });
}

function summarizeUsers(users: UserRecord[]): UserListMeta {
  return {
    totalUsers: users.length,
    administrators: users.filter((user) => (
      !user.Policy.IsDisabled && user.Policy.IsAdministrator
    )).length,
    disabled: users.filter((user) => user.Policy.IsDisabled).length,
  };
}

function requireCreateData(value: unknown): { Name: string; Password: string } {
  if (!isRecord(value) || typeof value.Name !== 'string' || typeof value.Password !== 'string') {
    throw new ApiError(400, 'validation', 'A name and password are required.');
  }
  return { Name: value.Name, Password: value.Password };
}

function requireName(value: unknown): string {
  if (!isRecord(value) || typeof value.Name !== 'string') {
    throw new ApiError(400, 'validation', 'A user name is required.');
  }
  return value.Name;
}

function encodedIdentifier(id: unknown): string {
  if ((typeof id !== 'string' && typeof id !== 'number') || String(id).length === 0) {
    throw new ApiError(400, 'validation', 'A user identifier is required.');
  }
  return encodeURIComponent(String(id));
}

function isTjxyUser(value: unknown): value is TjxyUser {
  return isRecord(value)
    && typeof value.Id === 'string'
    && value.Id.length > 0
    && typeof value.Name === 'string'
    && isRecord(value.Policy)
    && typeof value.Policy.IsAdministrator === 'boolean'
    && typeof value.Policy.IsDisabled === 'boolean';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function unsupported(): Promise<never> {
  return Promise.reject(unsupportedError());
}

function unsupportedError(): ApiError {
  return new ApiError(405, 'validation', 'This user operation is not supported.');
}
