import { loginDestination } from './loginDestination';

const origin = 'https://admin.example.test';

it('falls back to Dashboard for direct login without router state', () => {
  expect(loginDestination(null, origin)).toBe('/admin');
});

it('restores a validated admin pathname and search', () => {
  expect(loginDestination({
    nextPathname: '/admin/tasks',
    nextSearch: '?view=recent',
  }, origin)).toBe('/admin/tasks?view=recent');
});

it.each([
  ['external URL', { nextPathname: 'https://evil.example/admin/users' }],
  ['protocol-relative URL', { nextPathname: '//evil.example/admin/users' }],
  ['non-admin path', { nextPathname: '/public' }],
  ['login loop', { nextPathname: '/admin/login' }],
  ['authentication loop', { nextPathname: '/admin/authentication-error' }],
  ['access-denied loop', { nextPathname: '/admin/access-denied' }],
  ['malformed search', { nextPathname: '/admin/users', nextSearch: 'page=2' }],
  ['control character', { nextPathname: '/admin/users\u0000' }],
])('rejects %s', (_label, state) => {
  expect(loginDestination(state, origin)).toBe('/admin');
});

it('discards fragments from an otherwise safe destination', () => {
  expect(loginDestination({ nextPathname: '/admin/users#private' }, origin))
    .toBe('/admin/users');
});
