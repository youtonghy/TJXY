import { resolveClientTheme } from './registry';

it('normalizes supported theme options against the compiled schema', () => {
  const resolved = resolveClientTheme({
    id: 'cinema', schemaVersion: 1,
    options: { density: 'compact', contentWidth: 'invalid', accent: 'teal', ignored: true },
    revision: 4,
  });
  expect(resolved.definition.id).toBe('cinema');
  expect(resolved.options).toEqual({ density: 'compact', contentWidth: 'wide', accent: 'teal' });
  expect(resolved.didFallback).toBe(false);
});

it('falls back to classic when a stored theme is not compiled into the client', () => {
  const resolved = resolveClientTheme({
    id: 'removed-theme', schemaVersion: 1, options: { density: 'compact' }, revision: 5,
  });
  expect(resolved.definition.id).toBe('classic');
  expect(resolved.options).toEqual({ contentWidth: 'standard' });
  expect(resolved.didFallback).toBe(true);
});
