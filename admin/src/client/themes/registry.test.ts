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
  expect(resolved.options).toEqual({ contentWidth: 'standard', accent: 'teal' });
  expect(resolved.didFallback).toBe(true);
});

it('keeps stored classic options valid when the accent palette was never chosen', () => {
  const resolved = resolveClientTheme({
    id: 'classic', schemaVersion: 1, options: { contentWidth: 'wide' }, revision: 6,
  });
  expect(resolved.options).toEqual({ contentWidth: 'wide', accent: 'teal' });
  expect(resolveClientTheme({
    id: 'classic', schemaVersion: 1, options: { accent: 'emerald' }, revision: 7,
  }).options).toEqual({ contentWidth: 'standard', accent: 'emerald' });
});
