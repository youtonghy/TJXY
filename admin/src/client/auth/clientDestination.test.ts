import { safeClientDestination } from './clientDestination';

it.each([
  ['/app/items/abc?q=1', '/app/items/abc?q=1'],
  ['/admin/users', '/admin/users'],
  ['/admin/login', '/admin'],
  ['/admin/users\rmalformed', '/app/'],
  ['https://evil.invalid/app', '/app/'],
  ['//evil.invalid/app', '/app/'],
])('normalizes %s to a safe ordinary-client destination', (input, expected) => {
  expect(safeClientDestination(input)).toBe(expected);
});
