import { expect, type BrowserContext, type Route } from '@playwright/test';

const adminId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f31';
const libraryId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const rootId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';
const fixtureSession = 'fixture-session-not-a-production-credential';

export const fixtureIds = { adminId, libraryId, rootId } as const;

export interface AdminFixtureHarness {
  unexpectedRequests: string[];
  oauthReferrers: string[];
  assertComplete(): void;
}

export async function installAdminFixtures(
  context: BrowserContext,
  options: { authenticated?: boolean } = {},
): Promise<AdminFixtureHarness> {
  const unexpectedRequests: string[] = [];
  const oauthReferrers: string[] = [];

  if (options.authenticated !== false) {
    await context.addInitScript((token) => {
      window.sessionStorage.setItem('tjxy.web.token', token);
    }, fixtureSession);
  } else {
    await context.addInitScript(() => {
      window.sessionStorage.removeItem('tjxy.web.token');
      window.localStorage.removeItem('tjxy.web.deviceId');
    });
  }

  await context.route('**/*', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const method = request.method();

    if (url.pathname.startsWith('/admin/')) {
      await route.continue();
      return;
    }
    if (url.pathname === '/health/ready') {
      await route.fulfill({ body: 'ready', contentType: 'text/plain', status: 200 });
      return;
    }
    if (url.pathname === '/oauth-fixture/authorize') {
      oauthReferrers.push(request.headers().referer ?? '');
      await route.fulfill({ body: '<!doctype html><title>Fixture OAuth</title>', contentType: 'text/html', status: 200 });
      return;
    }

    const response = fixtureResponse(method, url);
    if (response !== null) {
      await fulfill(route, response);
      return;
    }
    if (isApiPath(url.pathname)) {
      unexpectedRequests.push(`${method} ${safeDiagnosticPath(url)}`);
      await route.fulfill({
        body: JSON.stringify({ error: 'unhandled-fixture-route' }),
        contentType: 'application/json',
        status: 501,
      });
      return;
    }
    await route.continue();
  });

  return {
    unexpectedRequests,
    oauthReferrers,
    assertComplete() {
      expect(unexpectedRequests, 'unexpected fixture API requests').toEqual([]);
    },
  };
}

export async function installLoginFixtures(context: BrowserContext): Promise<AdminFixtureHarness> {
  return installAdminFixtures(context, { authenticated: false });
}

type FixtureResponse = {
  body?: unknown;
  contentType?: string;
  status?: number;
};

function fixtureResponse(method: string, url: URL): FixtureResponse | null {
  const { pathname } = url;
  if (method === 'GET' && pathname === '/Users/Me') return { body: adminUser };
  if (method === 'GET' && pathname === '/Users') return { body: fixtureUsers };
  if (method === 'GET' && /^\/Users\/[^/]+$/u.test(pathname)) {
    const id = decodeURIComponent(pathname.slice('/Users/'.length));
    const user = fixtureUsers.find((candidate) => candidate.Id === id);
    return user === undefined ? { body: {}, status: 404 } : { body: user };
  }
  if (method === 'GET' && pathname === '/Devices') return { body: devicesResponse };
  if (method === 'GET' && pathname === '/Auth/Keys') return { body: apiKeysResponse };
  if (method === 'GET' && pathname === '/ScheduledTasks') return { body: scheduledTasks };
  if (method === 'GET' && pathname === '/Admin/Tasks/Jobs') return { body: taskJobs };
  if (method === 'GET' && pathname === '/Admin/Dashboard/Summary') return { body: dashboardSummary };
  if (method === 'GET' && pathname === '/Admin/Dashboard/NowPlaying') return { body: [] };
  if (
    method === 'GET'
    && (pathname === '/Admin/Dashboard/LoginHistory' || pathname === '/Admin/Dashboard/WatchHistory')
  ) return { body: emptyDashboardPage };
  if (method === 'GET' && pathname === '/Library/VirtualFolders') return { body: librariesResponse };
  if (method === 'GET' && /^\/Admin\/Libraries\/[^/]+\/HybridCandidates$/u.test(pathname)) {
    return { body: hybridCandidatesResponse };
  }
  if (method === 'POST' && pathname === '/Admin/Storage/OAuth/GoogleDrive/Start') {
    return { body: oauthStart(url, 'google-fixture-state') };
  }
  if (method === 'POST' && pathname === '/Admin/Storage/OAuth/OneDrive/Start') {
    return { body: oauthStart(url, 'onedrive-fixture-state') };
  }
  return null;
}

function oauthStart(url: URL, state: string) {
  return {
    State: state,
    AuthorizationUrl: `${url.origin}/oauth-fixture/authorize`,
  };
}

async function fulfill(route: Route, response: FixtureResponse) {
  const status = response.status ?? 200;
  if (response.body === undefined) {
    await route.fulfill({ status });
    return;
  }
  await route.fulfill({
    body: typeof response.body === 'string' ? response.body : JSON.stringify(response.body),
    contentType: response.contentType ?? 'application/json',
    status,
  });
}

function isApiPath(pathname: string): boolean {
  return [
    '/Admin/',
    '/Auth/',
    '/Devices',
    '/Library/',
    '/ScheduledTasks',
    '/Users',
  ].some((prefix) => pathname.startsWith(prefix));
}

function safeDiagnosticPath(url: URL): string {
  if (/^\/Auth\/Keys\/[^/]+$/u.test(url.pathname)) return '/Auth/Keys/[REDACTED]';
  return `${url.pathname}${url.search}`;
}

const basePolicy = {
  IsAdministrator: false,
  IsDisabled: false,
  EnableMediaPlayback: true,
  EnableAudioPlaybackTranscoding: true,
  EnableVideoPlaybackTranscoding: true,
  EnablePlaybackRemuxing: true,
  AuthenticationProviderId: 'TJXY.LocalAuthentication',
  PasswordResetProviderId: 'TJXY.LocalPasswordReset',
};

const adminUser = {
  Name: 'Fixture Administrator',
  ServerId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f99',
  Id: adminId,
  HasPassword: true,
  HasConfiguredPassword: true,
  Configuration: {},
  Policy: { ...basePolicy, IsAdministrator: true },
};

const fixtureUsers = [
  adminUser,
  {
    ...adminUser,
    Name: 'Alexandria Long-Name Operations Account Used For Responsive Verification',
    Id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f32',
    Policy: { ...basePolicy },
  },
  {
    ...adminUser,
    Name: 'Disabled Support',
    Id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f33',
    Policy: { ...basePolicy, IsDisabled: true },
  },
];

const deviceCapabilities = {
  PlayableMediaTypes: ['Video', 'Audio'],
  SupportedCommands: ['Play'],
  SupportsMediaControl: true,
  SupportsPersistentIdentifier: true,
  DeviceProfile: null,
  AppStoreUrl: null,
  IconUrl: null,
};

const devicesResponse = {
  Items: [{
    Name: 'Operations workstation',
    CustomName: 'Sydney control room',
    Id: 'fixture-device',
    LastUserName: 'Fixture Administrator',
    AppName: 'TJXY Web',
    AppVersion: '0.1.0',
    LastUserId: adminId,
    DateLastActivity: '2026-07-28T08:30:00Z',
    Capabilities: deviceCapabilities,
    IconUrl: null,
  }],
  StartIndex: 0,
  TotalRecordCount: 1,
};

const apiKeysResponse = {
  Items: [{
    Id: 7,
    AccessToken: '0000000000000000000000000000000000000000000000000000000000000000',
    DeviceId: null,
    AppName: 'Fixture automation',
    AppVersion: null,
    DeviceName: null,
    UserId: adminId,
    IsActive: true,
    DateCreated: '2026-07-27T08:00:00Z',
    DateRevoked: null,
    DateLastActivity: '2026-07-28T08:30:00Z',
    UserName: 'Fixture Administrator',
  }],
  StartIndex: 0,
  TotalRecordCount: 1,
};

const scheduledTasks = [{
  Id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f21',
  Name: 'Scan Media Library',
  State: 'Idle',
  Description: 'Discover and reconcile every configured catalog source.',
  Category: 'Library',
  Key: 'FullMediaScan',
}];

const statuses = ['Pending', 'Retrying', 'Running', 'Completed', 'Cancelled', 'Failed'];
const taskJobs = statuses.map((status, index) => ({
  Id: `018f17ac-4e99-7ec5-b4fd-8f15ca9f4f${String(40 + index)}`,
  TaskKind: index % 2 === 0 ? 'FullMediaScan' : 'ResolveMetadata',
  ScopeType: 'Library',
  ScopeId: libraryId,
  Status: status,
  Priority: 20 - index,
  AttemptCount: index,
  CreatedAt: '2026-07-28T08:00:00Z',
  StartedAt: index === 0 ? null : '2026-07-28T08:01:00Z',
  CompletedAt: index < 3 ? null : '2026-07-28T08:05:00Z',
}));

const dashboardSummary = {
  From: '2026-07-28T00:00:00Z',
  To: '2026-07-28T08:30:00Z',
  UsersTotal: fixtureUsers.length,
  UsersDisabled: 1,
  CatalogTotal: 3129,
  Movies: 100,
  Series: 100,
  Episodes: 2929,
  PlayCount: 4,
  UniqueViewers: 1,
  CurrentlyWatching: 0,
  Trend: [],
  TopItems: [],
};

const emptyDashboardPage = {
  Items: [],
  StartIndex: 0,
  TotalRecordCount: 0,
};

const librariesResponse = [{
  ItemId: libraryId,
  Name: 'International Film Archive With A Deliberately Long Operational Name',
  CollectionType: 'movies',
  Locations: [`tjxy://storage-root/${rootId}`],
  LibraryOptions: {
    Enabled: true,
    ScanProfile: 'Hybrid',
    ProfileVersion: 4,
    ObjectSelectionScope: 'all_synced_objects',
    MetadataPolicy: 'full',
    ExpansionPolicy: 'background',
    ProbePolicy: 'on_playback',
  },
}];

const hybridCandidatesResponse = {
  Items: [{
    Id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f51',
    Name: 'A Very Long Candidate Title For Mobile Wrapping Verification',
    ProductionYear: 2026,
    StructureState: 'title_complete',
    SelectedAt: '2026-07-28T08:15:00Z',
  }],
  StartIndex: 0,
  TotalRecordCount: 1,
};
