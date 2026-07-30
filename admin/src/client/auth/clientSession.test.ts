import { CLIENT_TOKEN_KEY, clearClientToken, getClientToken, setClientToken } from './clientSession';

it('stores ordinary-user credentials independently from the administrator session', () => {
  sessionStorage.setItem('tjxy.admin.token', 'admin-token');
  setClientToken('client-token');
  expect(getClientToken()).toBe('client-token');
  expect(sessionStorage.getItem(CLIENT_TOKEN_KEY)).toBe('client-token');
  clearClientToken();
  expect(sessionStorage.getItem('tjxy.admin.token')).toBe('admin-token');
});
