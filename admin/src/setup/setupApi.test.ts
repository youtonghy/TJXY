import { apiRequest } from '../api/httpClient';
import { completeSetup, getSetupStatus, recoverSetup, subscribeSetupProgress, testSetupDatabase, uploadSetupBranding, validateSetupNetwork } from './setupApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);
const csrf = 'csrf-token-0123456789abcdef';

beforeEach(() => { requestMock.mockReset(); });

it('strictly parses setup status and rejects response drift', async () => {
  requestMock.mockResolvedValueOnce({
    State: 'unconfigured',
    InstallationId: '11111111-1111-4111-8111-111111111111',
    CsrfToken: 'a'.repeat(64),
    DatabaseBackends: ['sqlite', 'postgresql', 'mysql'],
    DeploymentMode: 'native',
    Version: '0.1.0',
    ConfigurationWritable: true,
    SourceEligible: true,
    BlockingOverrides: [],
    ManagedDatabaseBackend: null,
  });
  await expect(getSetupStatus()).resolves.toEqual({
    state: 'unconfigured',
    installationId: '11111111-1111-4111-8111-111111111111',
    csrfToken: 'a'.repeat(64),
    databaseBackends: ['sqlite', 'postgresql', 'mysql'],
    deploymentMode: 'native',
    version: '0.1.0',
    configurationWritable: true,
    sourceEligible: true,
    blockingOverrides: [],
    managedDatabaseBackend: null,
  });

  requestMock.mockResolvedValueOnce({
    State: 'unconfigured',
    InstallationId: '11111111-1111-4111-1111-111111111111',
    CsrfToken: 'a'.repeat(64),
    DatabaseBackends: ['sqlite'],
    DeploymentMode: 'native',
    Version: '0.1.0',
    ConfigurationWritable: true,
    SourceEligible: true,
    BlockingOverrides: [],
    ManagedDatabaseBackend: null,
    Secret: 'leaked',
  });
  await expect(getSetupStatus()).rejects.toMatchObject({ category: 'invalid-response' });
});

it('sends csrf on database test and parses safe metadata', async () => {
  requestMock.mockResolvedValueOnce({ Backend: 'sqlite', Version: '3.49.1', ElapsedMilliseconds: 8 });
  await expect(testSetupDatabase(csrf, { Backend: 'sqlite', Path: '/data/tjxy.db' }))
    .resolves.toEqual({ backend: 'sqlite', version: '3.49.1', elapsedMilliseconds: 8 });
  expect(requestMock).toHaveBeenCalledWith('/Setup/Database/Test', {
    auth: 'none',
    method: 'POST',
    headers: { 'X-TJXY-Setup-CSRF': csrf },
    body: JSON.stringify({ Backend: 'sqlite', Path: '/data/tjxy.db' }),
  });
});

it('rejects database metadata for a different backend than the tested draft', async () => {
  requestMock.mockResolvedValueOnce({ Backend: 'mysql', Version: '8.4.0', ElapsedMilliseconds: 8 });

  await expect(testSetupDatabase(csrf, { Backend: 'sqlite', Path: '/data/tjxy.db' }))
    .rejects.toMatchObject({ category: 'invalid-response' });
});

it('submits secrets only in the final request body', async () => {
  requestMock.mockResolvedValueOnce({ DestinationUrl: 'http://127.0.0.1:8096/login?redirect=%2Fadmin' });
  const draft = {
    siteTitle: 'Cinema', siteSubtitle: 'Private screenings', locale: 'zh-CN' as const,
    logoUrl: '/brand/tjxy-mark.webp', iconUrl: '/brand/favicon.svg',
    database: { Backend: 'sqlite' as const, Path: '/data/tjxy.db' },
    network: { listenHost: '127.0.0.1', port: 8096, publicUrl: null },
    administratorUsername: 'admin', administratorPassword: 'correct horse',
  };
  await expect(completeSetup(csrf, draft)).resolves.toBe('http://127.0.0.1:8096/login?redirect=%2Fadmin');
  expect(requestMock).toHaveBeenCalledWith('/Setup/Complete', expect.objectContaining({
    auth: 'none', method: 'POST', headers: { 'X-TJXY-Setup-CSRF': csrf },
  }));
  expect(requestMock.mock.calls[0]?.[0]).not.toContain('correct horse');
});

it('submits a null database when the server manages PostgreSQL', async () => {
  requestMock.mockResolvedValueOnce({ DestinationUrl: 'http://127.0.0.1:8096/login?redirect=%2Fadmin' });
  await completeSetup(csrf, {
    siteTitle: 'Cinema', siteSubtitle: '', locale: 'en-US',
    logoUrl: '/brand/tjxy-mark.webp', iconUrl: '/brand/favicon.svg', database: null,
    network: { listenHost: '0.0.0.0', port: 8096, publicUrl: null },
    administratorUsername: 'admin', administratorPassword: 'correct horse',
  });
  expect(requestMock.mock.calls[0]?.[0]).toBe('/Setup/Complete');
  expect(requestMock.mock.calls[0]?.[1]?.body).toContain('"Database":null');
});

it('submits recovery credentials only to the csrf-protected recovery endpoint', async () => {
  requestMock.mockResolvedValueOnce({ DestinationUrl: 'http://127.0.0.1:8096/login?redirect=%2Fadmin' });

  await expect(recoverSetup(csrf, 'setup-admin', 'correct horse'))
    .resolves.toBe('http://127.0.0.1:8096/login?redirect=%2Fadmin');
  expect(requestMock).toHaveBeenCalledWith('/Setup/Recover', {
    auth: 'none',
    method: 'POST',
    headers: { 'X-TJXY-Setup-CSRF': csrf },
    body: JSON.stringify({ AdministratorUsername: 'setup-admin', AdministratorPassword: 'correct horse' }),
  });
});

it('strictly validates the normalized network preflight response', async () => {
  requestMock.mockResolvedValueOnce({
    ListenHost: '127.0.0.1',
    Port: 8096,
    PublicUrl: 'https://media.example.test',
    DestinationUrl: 'https://media.example.test/login?redirect=%2Fadmin',
  });

  await expect(validateSetupNetwork(csrf, {
    listenHost: '127.0.0.1', port: 8096, publicUrl: 'https://media.example.test',
  })).resolves.toEqual({
    listenHost: '127.0.0.1',
    port: 8096,
    publicUrl: 'https://media.example.test',
    destinationUrl: 'https://media.example.test/login?redirect=%2Fadmin',
  });
  expect(requestMock).toHaveBeenCalledWith('/Setup/Network/Validate', expect.objectContaining({
    method: 'POST',
    headers: { 'X-TJXY-Setup-CSRF': csrf },
  }));

  requestMock.mockResolvedValueOnce({
    ListenHost: '127.0.0.1',
    Port: 8096,
    PublicUrl: null,
    DestinationUrl: 'javascript:alert(1)',
  });
  await expect(validateSetupNetwork(csrf, {
    listenHost: '127.0.0.1', port: 8096, publicUrl: null,
  })).rejects.toMatchObject({ category: 'invalid-response' });
});

it('uploads a bounded branding image with csrf and parses its public asset path', async () => {
  requestMock.mockResolvedValueOnce({
    AssetUrl: `/Branding/Assets/logo-${'a'.repeat(64)}.png`,
  });
  const file = new File([new Uint8Array([137, 80, 78, 71])], 'logo.png', { type: 'image/png' });

  await expect(uploadSetupBranding(csrf, 'logo', file))
    .resolves.toBe(`/Branding/Assets/logo-${'a'.repeat(64)}.png`);
  expect(requestMock).toHaveBeenCalledWith('/Setup/Branding/logo', {
    auth: 'none',
    method: 'PUT',
    headers: {
      'Content-Type': 'image/png',
      'X-TJXY-Setup-CSRF': csrf,
    },
    body: file,
  });

  requestMock.mockResolvedValueOnce({ AssetUrl: '/tmp/private/logo.png' });
  await expect(uploadSetupBranding(csrf, 'logo', file))
    .rejects.toMatchObject({ category: 'invalid-response' });
});

it('accepts monotonic progress and closes on repeated or regressing stages', () => {
  const listeners: Partial<Record<'stage' | 'error', (event?: MessageEvent<string>) => void>> = {};
  const close = vi.fn();
  const stages: string[] = [];
  const errors = vi.fn();
  const installationId = '11111111-1111-4111-8111-111111111111';
  const unsubscribe = subscribeSetupProgress(
    installationId,
    (event) => { stages.push(event.stage); },
    errors,
    (url) => {
      expect(url).toBe(`/Setup/Progress?installationId=${installationId}`);
      return {
        addEventListener(type, listener) { listeners[type] = listener as (event?: MessageEvent<string>) => void; },
        close,
      };
    },
  );

  listeners.stage?.({ data: JSON.stringify({ InstallationId: installationId, Stage: 'connecting_database' }) } as MessageEvent<string>);
  listeners.stage?.({ data: JSON.stringify({ InstallationId: installationId, Stage: 'migrating_database' }) } as MessageEvent<string>);
  listeners.stage?.({ data: JSON.stringify({ InstallationId: installationId, Stage: 'migrating_database' }) } as MessageEvent<string>);
  expect(stages).toEqual(['connecting_database', 'migrating_database']);
  expect(errors).toHaveBeenCalledOnce();
  expect(close).toHaveBeenCalledOnce();
  unsubscribe();
});
