import { render, type RenderOptions, type RenderResult } from '@testing-library/react';
import {
  CoreAdminContext,
  type AuthProvider,
  type DataProvider,
  type I18nProvider,
} from 'ra-core';
import { StrictMode, type ReactElement } from 'react';
import { MemoryRouter, type InitialEntry } from 'react-router-dom';

const unexpectedDataCall = (): Promise<never> => Promise.reject(
  new Error('Unexpected data provider call in an isolated UI test.'),
);

export const strictTestDataProvider: DataProvider = {
  getList: unexpectedDataCall,
  getOne: unexpectedDataCall,
  getMany: unexpectedDataCall,
  getManyReference: unexpectedDataCall,
  create: unexpectedDataCall,
  update: unexpectedDataCall,
  updateMany: unexpectedDataCall,
  delete: unexpectedDataCall,
  deleteMany: unexpectedDataCall,
};

export const defaultTestAuthProvider: AuthProvider = {
  login: () => Promise.resolve(undefined),
  logout: () => Promise.resolve(undefined),
  checkAuth: () => Promise.resolve(undefined),
  checkError: () => Promise.resolve(undefined),
  getIdentity: () => Promise.resolve({ id: 'admin-id', fullName: 'Admin' }),
  getPermissions: () => Promise.resolve('administrator'),
};

interface RenderWithAdminOptions extends Omit<RenderOptions, 'wrapper'> {
  initialEntries?: InitialEntry[];
  authProvider?: AuthProvider;
  dataProvider?: DataProvider;
  i18nProvider?: I18nProvider;
  strict?: boolean;
}

export function renderWithAdmin(
  ui: ReactElement,
  {
    initialEntries = ['/admin/users'],
    authProvider = defaultTestAuthProvider,
    dataProvider = strictTestDataProvider,
    i18nProvider,
    strict = false,
    ...renderOptions
  }: RenderWithAdminOptions = {},
): RenderResult {
  const tree = (
    <MemoryRouter initialEntries={initialEntries}>
      <CoreAdminContext
        authProvider={authProvider}
        basename="/admin"
        dataProvider={dataProvider}
        i18nProvider={i18nProvider}
      >
        {ui}
      </CoreAdminContext>
    </MemoryRouter>
  );

  return render(strict ? <StrictMode>{tree}</StrictMode> : tree, renderOptions);
}
