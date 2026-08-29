import { ApiError, apiRequest } from '../api/httpClient';
import { invalidResponse, isNonNegativeInteger, isRecord, validText, validUuid } from '../api/responseValidation';
import type {
  CompleteSetupDraft,
  DatabaseBackend,
  DatabaseDraft,
  DatabaseTestResult,
  NetworkDraft,
  NetworkValidationResult,
  SetupProgressEvent,
  SetupProgressStage,
  SetupStatus,
} from './setupTypes';

const CSRF_HEADER = 'X-TJXY-Setup-CSRF';
const MAX_BRANDING_BYTES = 2 * 1024 * 1024;
const BRANDING_TYPES = ['image/png', 'image/jpeg', 'image/webp', 'image/x-icon', 'image/vnd.microsoft.icon'];
const progressStages: SetupProgressStage[] = [
  'connecting_database', 'migrating_database', 'creating_administrator', 'saving_settings',
  'completing_installation', 'complete',
];

interface SetupEventSource {
  addEventListener(type: 'stage', listener: (event: MessageEvent<string>) => void): void;
  addEventListener(type: 'error', listener: () => void): void;
  close(): void;
}

type SetupEventSourceFactory = (url: string) => SetupEventSource;

export async function getSetupStatus(signal?: AbortSignal): Promise<SetupStatus> {
  const value = await apiRequest<unknown>('/Setup/Status', {
    auth: 'none',
    ...(signal === undefined ? {} : { signal }),
  });
  if (
    !isRecord(value)
    || !exactKeys(value, ['State', 'InstallationId', 'CsrfToken', 'DatabaseBackends', 'DeploymentMode', 'Version', 'ConfigurationWritable', 'SourceEligible', 'BlockingOverrides', 'ManagedDatabaseBackend'])
    || (value.State !== 'unconfigured' && value.State !== 'pending')
    || !validUuid(value.InstallationId)
    || !validToken(value.CsrfToken)
    || !Array.isArray(value.DatabaseBackends)
    || value.DatabaseBackends.length !== 3
    || !value.DatabaseBackends.every(isBackend)
    || new Set(value.DatabaseBackends).size !== value.DatabaseBackends.length
    || (value.DeploymentMode !== 'native' && value.DeploymentMode !== 'container')
    || !validText(value.Version, 64)
    || typeof value.ConfigurationWritable !== 'boolean'
    || typeof value.SourceEligible !== 'boolean'
    || !validBlockingOverrides(value.BlockingOverrides)
    || (value.ManagedDatabaseBackend !== null && !isBackend(value.ManagedDatabaseBackend))
  ) throw invalidResponse('setup status');
  return {
    state: value.State,
    installationId: value.InstallationId,
    csrfToken: value.CsrfToken,
    databaseBackends: value.DatabaseBackends,
    deploymentMode: value.DeploymentMode,
    version: value.Version,
    configurationWritable: value.ConfigurationWritable,
    sourceEligible: value.SourceEligible,
    blockingOverrides: value.BlockingOverrides,
    managedDatabaseBackend: value.ManagedDatabaseBackend,
  };
}

export async function testSetupDatabase(
  csrfToken: string,
  draft: DatabaseDraft,
  signal?: AbortSignal,
): Promise<DatabaseTestResult> {
  requireCsrf(csrfToken);
  const value = await apiRequest<unknown>('/Setup/Database/Test', {
    auth: 'none',
    method: 'POST',
    headers: { [CSRF_HEADER]: csrfToken },
    body: JSON.stringify(draft),
    ...(signal === undefined ? {} : { signal }),
  });
  if (
    !isRecord(value)
    || !exactKeys(value, ['Backend', 'Version', 'ElapsedMilliseconds'])
    || !isBackend(value.Backend)
    || value.Backend !== draft.Backend
    || !validText(value.Version, 128)
    || !isNonNegativeInteger(value.ElapsedMilliseconds)
  ) throw invalidResponse('database test');
  return {
    backend: value.Backend,
    version: value.Version,
    elapsedMilliseconds: value.ElapsedMilliseconds,
  };
}

export async function validateSetupNetwork(
  csrfToken: string,
  draft: NetworkDraft,
  signal?: AbortSignal,
): Promise<NetworkValidationResult> {
  requireCsrf(csrfToken);
  const value = await apiRequest<unknown>('/Setup/Network/Validate', {
    auth: 'none',
    method: 'POST',
    headers: { [CSRF_HEADER]: csrfToken },
    body: JSON.stringify({
      ListenHost: draft.listenHost,
      Port: draft.port,
      PublicUrl: draft.publicUrl,
    }),
    ...(signal === undefined ? {} : { signal }),
  });
  if (
    !isRecord(value)
    || !exactKeys(value, ['ListenHost', 'Port', 'PublicUrl', 'DestinationUrl'])
    || !validText(value.ListenHost, 64)
    || !Number.isSafeInteger(value.Port)
    || (value.Port as number) < 1
    || (value.Port as number) > 65_535
    || (value.PublicUrl !== null && !validHttpUrl(value.PublicUrl))
    || !validDestination(value.DestinationUrl)
  ) throw invalidResponse('network validation');
  return {
    listenHost: value.ListenHost,
    port: value.Port as number,
    publicUrl: value.PublicUrl,
    destinationUrl: value.DestinationUrl,
  };
}

export async function uploadSetupBranding(
  csrfToken: string,
  kind: 'logo' | 'icon',
  file: File,
): Promise<string> {
  requireCsrf(csrfToken);
  if (!BRANDING_TYPES.includes(file.type) || file.size === 0 || file.size > MAX_BRANDING_BYTES) {
    throw new ApiError(422, 'validation', 'The selected image is not valid.');
  }
  const value = await apiRequest<unknown>(`/Setup/Branding/${kind}`, {
    auth: 'none',
    method: 'PUT',
    headers: {
      'Content-Type': file.type,
      [CSRF_HEADER]: csrfToken,
    },
    body: file,
  });
  if (
    !isRecord(value)
    || !exactKeys(value, ['AssetUrl'])
    || typeof value.AssetUrl !== 'string'
    || !new RegExp(`^/Branding/Assets/${kind}-[a-f0-9]{64}\\.(?:png|jpg|webp|ico)$`, 'u').test(value.AssetUrl)
  ) throw invalidResponse('branding upload');
  return value.AssetUrl;
}

export async function completeSetup(
  csrfToken: string,
  draft: CompleteSetupDraft,
): Promise<string> {
  requireCsrf(csrfToken);
  const value = await apiRequest<unknown>('/Setup/Complete', {
    auth: 'none',
    method: 'POST',
    headers: { [CSRF_HEADER]: csrfToken },
    body: JSON.stringify({
      SiteTitle: draft.siteTitle,
      SiteSubtitle: draft.siteSubtitle,
      Locale: draft.locale,
      LogoUrl: draft.logoUrl,
      IconUrl: draft.iconUrl,
      Database: draft.database,
      Network: {
        ListenHost: draft.network.listenHost,
        Port: draft.network.port,
        PublicUrl: draft.network.publicUrl,
      },
      AdministratorUsername: draft.administratorUsername,
      AdministratorPassword: draft.administratorPassword,
    }),
  });
  return parseDestination(value, 'setup completion');
}

export async function recoverSetup(
  csrfToken: string,
  administratorUsername: string,
  administratorPassword: string,
): Promise<string> {
  requireCsrf(csrfToken);
  const value = await apiRequest<unknown>('/Setup/Recover', {
    auth: 'none',
    method: 'POST',
    headers: { [CSRF_HEADER]: csrfToken },
    body: JSON.stringify({
      AdministratorUsername: administratorUsername,
      AdministratorPassword: administratorPassword,
    }),
  });
  return parseDestination(value, 'setup recovery');
}

export function subscribeSetupProgress(
  installationId: string,
  onStage: (event: SetupProgressEvent) => void,
  onError: () => void,
  factory: SetupEventSourceFactory = (url) => new EventSource(url),
): () => void {
  if (!validUuid(installationId)) throw invalidResponse('setup installation id');
  const source = factory(`/Setup/Progress?installationId=${encodeURIComponent(installationId)}`);
  let lastIndex = -1;
  let closed = false;
  const fail = () => {
    if (closed) return;
    closed = true;
    source.close();
    onError();
  };
  source.addEventListener('stage', (message) => {
    try {
      const event = parseSetupProgress(JSON.parse(message.data) as unknown);
      if (event.installationId !== installationId) { fail(); return; }
      if (event.stage === 'failed') {
        onStage(event); source.close(); closed = true; return;
      }
      const index = progressStages.indexOf(event.stage);
      if (index <= lastIndex) { fail(); return; }
      lastIndex = index;
      onStage(event);
      if (event.stage === 'complete') { source.close(); closed = true; }
    } catch { fail(); }
  });
  source.addEventListener('error', fail);
  return () => { if (!closed) source.close(); closed = true; };
}

export function parseSetupProgress(value: unknown): SetupProgressEvent {
  if (
    !isRecord(value)
    || !exactKeys(value, ['InstallationId', 'Stage'])
    || !validUuid(value.InstallationId)
    || !isProgressStage(value.Stage)
  ) throw invalidResponse('setup progress');
  return { installationId: value.InstallationId, stage: value.Stage };
}

function parseDestination(value: unknown, context: string): string {
  if (!isRecord(value) || !exactKeys(value, ['DestinationUrl']) || !validDestination(value.DestinationUrl)) {
    throw invalidResponse(context);
  }
  return value.DestinationUrl;
}

function validDestination(value: unknown): value is string {
  if (typeof value !== 'string' || value.length > 2_048) return false;
  try {
    const url = new URL(value);
    return (url.protocol === 'http:' || url.protocol === 'https:')
      && url.username === ''
      && url.password === ''
      && url.pathname === '/login'
      && url.search === '?redirect=%2Fadmin'
      && url.hash === '';
  } catch {
    return false;
  }
}

function validHttpUrl(value: unknown): value is string {
  if (typeof value !== 'string' || value.length > 2_048) return false;
  try {
    const url = new URL(value);
    return (url.protocol === 'http:' || url.protocol === 'https:')
      && url.hostname.length > 0
      && url.username === ''
      && url.password === ''
      && url.search === ''
      && url.hash === '';
  } catch {
    return false;
  }
}

function requireCsrf(value: string): void {
  if (!validToken(value)) throw new ApiError(403, 'authorization', 'The setup session is invalid.');
}

function validToken(value: unknown): value is string {
  return typeof value === 'string'
    && value.length >= 16
    && value.length <= 256
    && /^[A-Za-z0-9_-]+$/u.test(value);
}

function isBackend(value: unknown): value is DatabaseBackend {
  return value === 'sqlite' || value === 'postgresql' || value === 'mysql';
}

function isProgressStage(value: unknown): value is SetupProgressStage {
  return value === 'failed' || progressStages.includes(value as SetupProgressStage);
}

function validBlockingOverrides(value: unknown): value is string[] {
  return Array.isArray(value)
    && value.length <= 5
    && value.every((name): name is string => typeof name === 'string' && /^TJXY_[A-Z_]+$/u.test(name))
    && new Set<string>(value).size === value.length;
}

function exactKeys(value: Record<string, unknown>, keys: string[]): boolean {
  const actual = Object.keys(value);
  return actual.length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}
