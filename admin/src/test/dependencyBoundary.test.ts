import packageManifest from '../../package.json';
import pnpmLock from '../../pnpm-lock.yaml?raw';

const forbiddenNames = ['react-admin', 'ra-ui-materialui'];
const forbiddenScopes = ['@mui/', '@emotion/'];
const productionSources = import.meta.glob<string>([
  '../**/*.ts',
  '../**/*.tsx',
  '!../test/**',
  '!../**/*.test.ts',
  '!../**/*.test.tsx',
], {
  eager: true,
  import: 'default',
  query: '?raw',
});

it('keeps presentation packages out of direct dependencies', () => {
  const forbidden = Object.keys(packageManifest.dependencies).filter(isForbiddenPackage);

  expect(forbidden).toEqual([]);
});

it('keeps presentation packages out of the complete lockfile graph', () => {
  const lockedPackages = lockfilePackageNames(pnpmLock);
  const forbidden = lockedPackages.filter(isForbiddenPackage);

  expect(lockedPackages).toContain('react');
  expect(forbidden).toEqual([]);
});

it('keeps presentation imports out of production source', () => {
  const forbidden = Object.entries(productionSources)
    .filter(([, source]) => /(?:react-admin|ra-ui-materialui|@mui\/|@emotion\/)/u.test(source))
    .map(([file]) => file);

  expect(forbidden).toEqual([]);
});

function isForbiddenPackage(name: string): boolean {
  return forbiddenNames.includes(name) || forbiddenScopes.some((scope) => name.startsWith(scope));
}

// pnpm-lock.yaml lists every resolved package as a two-space-indented
// `name@version:` key under `packages:` and `snapshots:`.
function lockfilePackageNames(lockfile: string): string[] {
  return [...lockfile.matchAll(/^ {2}'?((?:@[^/'\s]+\/)?[^@'\s]+)@/gmu)].map(([, name]) => name ?? '');
}
