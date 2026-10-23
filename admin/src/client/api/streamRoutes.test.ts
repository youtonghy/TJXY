import { setClientToken } from '../auth/clientSession';
import {
  activeStreamOrigin,
  clientMediaBlob,
  ensureStreamRoutes,
  markStreamOriginFailed,
  refreshStreamRoutes,
  resetStreamRoutesForTests,
  resolveStreamUrl,
  selectStreamOrigin,
  setStreamRoutePreference,
  streamRouteCandidates,
  type StreamRoute,
} from './streamRoutes';

const CURRENT = window.location.origin;
const FAST = 'https://fast.example.com';
const SLOW = 'https://slow.example.com';

function route(origin: string, latencyMs?: number, status: StreamRoute['status'] = 'ok'): StreamRoute {
  return { origin, current: origin === CURRENT, status, ...(latencyMs === undefined ? {} : { latencyMs }) };
}

function urlOf(input: RequestInfo | URL): URL {
  return new URL(input instanceof Request ? input.url : input.toString());
}

function delayed(ms: number, response: () => Response): Promise<Response> {
  return new Promise((resolve) => { setTimeout(() => { resolve(response()); }, ms); });
}

/** Serves the address list and pings; `delays` maps an origin to its ping time, `null` for unreachable. */
function stubServer(delays: Record<string, number | null>) {
  const fetchMock = vi.fn((input: RequestInfo | URL) => {
    const url = urlOf(input);
    if (url.pathname === '/System/StreamAddresses') {
      return Promise.resolve(new Response(JSON.stringify({
        Addresses: [{ Name: 'Fast', Url: `${FAST}/` }, { Name: 'Slow', Url: SLOW }],
      }), { headers: { 'Content-Type': 'application/json' } }));
    }
    if (url.pathname === '/System/Ping') {
      const delay = delays[url.origin];
      if (delay === null || delay === undefined) return Promise.reject(new TypeError('Failed to fetch'));
      return delayed(delay, () => new Response('TJXY Server'));
    }
    return Promise.resolve(new Response(url.origin));
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

beforeEach(() => {
  window.localStorage.clear();
  window.sessionStorage.clear();
  resetStreamRoutesForTests();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

it('selects the fastest reachable route and honours a manual choice while it exists', () => {
  const routes = [route(CURRENT, 40), route(FAST, 12), route(SLOW, undefined, 'failed')];
  expect(selectStreamOrigin(routes, { mode: 'auto' }, CURRENT)).toBe(FAST);
  expect(selectStreamOrigin([route(CURRENT, 12), route(FAST, 12)], { mode: 'auto' }, CURRENT)).toBe(CURRENT);
  expect(selectStreamOrigin(routes, { mode: 'manual', origin: CURRENT }, CURRENT)).toBe(CURRENT);
  expect(selectStreamOrigin(routes, { mode: 'manual', origin: 'https://removed.example.com' }, CURRENT)).toBe(FAST);
  expect(selectStreamOrigin([route(FAST, undefined, 'pending')], { mode: 'auto' }, CURRENT)).toBe(CURRENT);
});

it('always offers the current address first and merges duplicate addresses', () => {
  const routes = streamRouteCandidates([
    { Name: 'Here', Url: `${CURRENT}/` },
    { Name: 'Fast', Url: FAST },
    { Name: 'Fast again', Url: `${FAST}/` },
    { Name: 'Broken', Url: 'not a url at all' },
  ], CURRENT);
  expect(routes.map((candidate) => [candidate.origin, candidate.name, candidate.current])).toEqual([
    [CURRENT, 'Here', true],
    [FAST, 'Fast', false],
  ]);
});

it('measures every route and plays from the fastest one', async () => {
  stubServer({ [CURRENT]: 60, [FAST]: 1, [SLOW]: null });
  await expect(refreshStreamRoutes()).resolves.toBe(FAST);
  expect(resolveStreamUrl('/Videos/1/stream?static=true')).toBe(`${FAST}/Videos/1/stream?static=true`);
  expect(resolveStreamUrl('https://iptv.example.com/live.m3u8')).toBe('https://iptv.example.com/live.m3u8');

  // A fresh measurement is reused instead of probing again.
  const fetchMock = stubServer({});
  await expect(ensureStreamRoutes()).resolves.toBe(FAST);
  expect(fetchMock).not.toHaveBeenCalled();

  markStreamOriginFailed(FAST);
  expect(activeStreamOrigin()).toBe(CURRENT);
});

it('keeps a manual route across reloads of the preference', async () => {
  stubServer({ [CURRENT]: 1, [FAST]: 30, [SLOW]: 50 });
  await refreshStreamRoutes();
  expect(activeStreamOrigin()).toBe(CURRENT);
  setStreamRoutePreference({ mode: 'manual', origin: SLOW });
  expect(activeStreamOrigin()).toBe(SLOW);
  resetStreamRoutesForTests();
  await ensureStreamRoutes();
  expect(activeStreamOrigin()).toBe(SLOW);
});

it('loads images from the active route and falls back to the current address', async () => {
  setClientToken('token');
  const fetchMock = stubServer({ [CURRENT]: 60, [FAST]: 1 });
  await refreshStreamRoutes();
  await expect((await clientMediaBlob('/Items/1/Images/Primary')).text()).resolves.toBe(FAST);
  const imageCall = fetchMock.mock.calls.find(([input]) => urlOf(input).href === `${FAST}/Items/1/Images/Primary`);
  expect((imageCall as unknown as [string, RequestInit] | undefined)?.[1]).toMatchObject({ credentials: 'omit' });

  fetchMock.mockImplementation((input: RequestInfo | URL) => {
    const url = urlOf(input);
    if (url.origin === FAST) return Promise.reject(new TypeError('Failed to fetch'));
    return Promise.resolve(new Response(url.origin));
  });
  await expect((await clientMediaBlob('/Items/1/Images/Primary')).text()).resolves.toBe(CURRENT);
  expect(activeStreamOrigin()).toBe(CURRENT);
});
