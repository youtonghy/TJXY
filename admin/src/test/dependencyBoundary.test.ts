import packageLock from '../../package-lock.json';
import packageManifest from '../../package.json';

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
  const forbidden = Object.keys(packageLock.packages).filter((packagePath) => {
    return forbiddenNames.some((name) => packagePath === `node_modules/${name}`
      || packagePath.endsWith(`/node_modules/${name}`))
      || forbiddenScopes.some((scope) => packagePath.includes(`node_modules/${scope}`));
  });

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
