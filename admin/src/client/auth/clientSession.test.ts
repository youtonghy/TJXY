import { CLIENT_TOKEN_KEY, clearClientToken, getClientToken, setClientToken } from './clientSession';

it('uses the shared browser credential consumed by the administrator session', () => {
  setClientToken('client-token');
  expect(getClientToken()).toBe('client-token');
  expect(sessionStorage.getItem(CLIENT_TOKEN_KEY)).toBe('client-token');
  clearClientToken();
  expect(sessionStorage.getItem('tjxy.web.token')).toBeNull();
});
