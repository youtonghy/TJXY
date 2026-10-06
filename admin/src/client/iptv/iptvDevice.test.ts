import { describe, expect, it } from 'vitest';
import type { IptvChannel } from './iptvChannels';
import { aesGcmEncryptB64 } from './iptvDeviceCrypto';
import {
  IptvDeviceEngine,
  isIptvDeviceChannel,
  isSessionInvalidatingError,
  livePlaybackHostNeedsSignedHeaders,
  rewritePlaylistUrls,
} from './iptvDevice';

// Deterministic harness: fake clock, seeded RNG, in-memory storage and a
// scripted fetch router that plays the upstream cloud/app/live endpoints.
const SESSION_KEY = 'session-key-under-test';
const LIVE_URL = 'http://live01.cctv.cn/hls/cctv1.m3u8';
const CDN_URL = 'http://cdn.example/live/cctv1/index.m3u8';
const CDN_PLAYLIST = '#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.000,\nseg1.ts\n#EXTINF:6.000,\nseg2.ts\n';

const channel: IptvChannel = {
  slug: 'cctv1',
  name: 'CCTV-1',
  sid: 'sid',
  pid: 'pid',
  defn: 'fhd',
  timeshift: true,
  liveId: 'Live-test-1',
};

const fixedBytes = (length: number) => Uint8Array.from({ length }, (_, i) => i % 256);
const jsonHeaders = { 'Content-Type': 'application/json' };

interface RecordedCall {
  url: string;
  method: string;
  headers: Record<string, string>;
  body: string;
}

function makeStorage() {
  const map = new Map<string, string>();
  return {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => {
      map.set(key, value);
    },
    map,
  };
}

function toBodyText(body: BodyInit | null | undefined): string {
  if (body === null || body === undefined) return '';
  if (typeof body === 'string') return body;
  if (body instanceof Uint8Array) return new TextDecoder().decode(body);
  return '';
}

function makeRouter(options: { live01Status?: number } = {}) {
  const calls: RecordedCall[] = [];
  let sessionKey = SESSION_KEY;
  const fetchImpl = (url: string, init?: RequestInit): Promise<Response> => {
    const headers = (init?.headers ?? {}) as Record<string, string>;
    calls.push({
      url,
      method: init?.method ?? 'GET',
      headers,
      body: toBodyText(init?.body),
    });
    const respond = (value: unknown, status = 200) =>
      Promise.resolve(new Response(JSON.stringify(value), { status, headers: jsonHeaders }));
    if (url.includes('collect.cctv.cn')) return respond({});
    if (url.includes('dictionary')) return respond({});
    if (url.includes('app/start')) {
      // The engine derives the AES key from the X-Fingerprint it sent.
      const fp = headers['X-Fingerprint'] ?? '';
      return respond({ data: { key: aesGcmEncryptB64(sessionKey, fp.slice(0, 32), fixedBytes) } });
    }
    if (url.includes('data/message/single')) return respond({ result: 0 });
    if (url.includes('device/v2/get')) return respond({ result: 0, data: { guid: 'cloud-guid-1' } });
    if (url.includes('device/v2/register')) return respond({ result: 0, data: { guid: 'cloud-guid-1' } });
    if (url.includes('api/index')) return respond({});
    if (url.includes('drm/config')) return respond({});
    if (url.includes('version/config')) return respond({});
    if (url.includes('live/v1/01')) {
      const status = options.live01Status ?? 200;
      if (status !== 200) return respond({ error: 'bad' }, status);
      return respond({
        data: {
          videoList: [
            { url: aesGcmEncryptB64(LIVE_URL, sessionKey, fixedBytes), rate: '36p', rateName: 'UHD' },
            { url: 'http://fallback.invalid/lo.m3u8', rate: '15p' },
          ],
        },
      });
    }
    if (url.includes('live/v1/02')) {
      return respond({ data: { appSecret: aesGcmEncryptB64('vdn-secret', sessionKey, fixedBytes) } });
    }
    if (url.includes('getstream')) return respond({ succeed: '1', url: CDN_URL });
    if (url === CDN_URL) return Promise.resolve(new Response(CDN_PLAYLIST, { status: 200 }));
    return Promise.resolve(new Response('not found', { status: 404 }));
  };
  return { calls, fetchImpl, setSessionKey: (key: string) => { sessionKey = key; } };
}

function makeEngine(router: { fetchImpl: (url: string, init?: RequestInit) => Promise<Response> }, nowRef: { t: number }) {
  return new IptvDeviceEngine({
    fetchImpl: router.fetchImpl,
    now: () => nowRef.t,
    randomBytes: fixedBytes,
    storage: makeStorage(),
    sleep: () => Promise.resolve(),
  });
}

describe('iptvDeviceEngine happy path', () => {
  it('bootstraps in the upstream request order and resolves the channel', async () => {
    const nowRef = { t: 1_700_000_000_000 };
    const router = makeRouter();
    const engine = makeEngine(router, nowRef);
    const playlist = await engine.fetchPlaylist(channel);

    expect(playlist).toContain('http://cdn.example/live/cctv1/seg1.ts');
    expect(playlist).toContain('http://cdn.example/live/cctv1/seg2.ts');
    const urls = router.calls.map((call) => call.url);
    const order = [
      'collect.cctv.cn', // app_start_d1 report
      'dictionary',
      'app/start',
      'data/message/single', // app event
      'collect.cctv.cn', // page report
      'data/message/single', // page event
      'device/v2/get', // cloud registration
      'data/message/single', // device info
      'data/message/single', // heartbeat
      'api/index',
      'drm/config',
      'version/config',
      'live/v1/01',
      'live/v1/02',
      'getstream',
      CDN_URL,
    ];
    expect(urls).toHaveLength(order.length);
    order.forEach((needle, i) => {
      expect(urls[i], `request ${String(i)}`).toContain(needle);
    });
  });

  it('reuses the cached entry: a second playlist fetch only re-GETs the CDN playlist', async () => {
    const nowRef = { t: 1_700_000_000_000 };
    const router = makeRouter();
    const engine = makeEngine(router, nowRef);
    await engine.fetchPlaylist(channel);
    router.calls.length = 0;
    nowRef.t += 10_000;
    await engine.fetchPlaylist(channel);
    expect(router.calls.map((call) => call.url)).toEqual([CDN_URL]);
  });

  it('attaches signed playback headers for segments under the resolved prefix', async () => {
    const nowRef = { t: 1_700_000_000_000 };
    const router = makeRouter();
    const engine = makeEngine(router, nowRef);
    await engine.fetchPlaylist(channel);
    const signed = engine.segmentHeaders('http://cdn.example/live/cctv1/seg9.ts');
    expect(signed.APPID).toBe('9f5c54c4ed0e50109b800f7e28fec205');
    expect(signed.APPSIGN).toBeTruthy();
    expect(signed.APPRANDOMSTR).toBeTruthy();
    const vdnCall = router.calls.find((call) => call.url.includes('getstream'));
    expect(vdnCall?.headers.APPID).toBe('9f5c54c4ed0e50109b800f7e28fec205');
    expect(vdnCall?.headers.APPSIGN).toBe(signed.APPSIGN);
    // Unrelated hosts keep the anonymous JCE-style default headers.
    expect(engine.segmentHeaders('http://other.example/x.ts').APPSIGN).toBeUndefined();
  });

  it('skips the whole device path for channels without a liveId', async () => {
    const nowRef = { t: 1_700_000_000_000 };
    const router = makeRouter();
    const engine = makeEngine(router, nowRef);
    await expect(engine.fetchPlaylist({ ...channel, slug: 'cctv6', liveId: undefined })).rejects.toThrow('live id');
    expect(router.calls).toHaveLength(0);
    expect(isIptvDeviceChannel(channel)).toBe(true);
    expect(isIptvDeviceChannel({ ...channel, liveId: undefined })).toBe(false);
  });
});

describe('iptvDeviceEngine failure handling', () => {
  it('enters cooldown after a resolve failure and retries after it elapses', async () => {
    const nowRef = { t: 1_700_000_000_000 };
    const router = makeRouter({ live01Status: 400 });
    const engine = makeEngine(router, nowRef);
    await expect(engine.fetchPlaylist(channel)).rejects.toThrow('live/v1/01 http 400');
    const live01Calls = router.calls.filter((call) => call.url.includes('live/v1/01')).length;
    expect(live01Calls).toBe(1);

    // Inside the 30s cooldown the engine refuses without new live calls.
    nowRef.t += 5_000;
    await expect(engine.fetchPlaylist(channel)).rejects.toThrow('cooldown');
    expect(router.calls.filter((call) => call.url.includes('live/v1/01'))).toHaveLength(live01Calls);

    // After the cooldown a fresh attempt re-bootstraps the dropped session.
    nowRef.t += 30_000;
    await expect(engine.fetchPlaylist(channel)).rejects.toThrow('live/v1/01 http 400');
    expect(router.calls.filter((call) => call.url.includes('live/v1/01'))).toHaveLength(2);
    expect(router.calls.some((call) => call.url.includes('app/start'))).toBe(true);
  });

  it('invalidates the session on decrypt-level failures', () => {
    for (const [message, expected] of [
      ['app/start http 400: x', true],
      ['live/v1/01 http 400', true],
      ['live/v1/01 http 500', false],
      ['vdn http 500', false],
      ['AES-GCM decrypt failed', true],
      ['missing encrypted session key', true],
      ['upstream m3u8 http 404', false],
    ] as const) {
      expect(isSessionInvalidatingError(message), message).toBe(expected);
    }
  });
});

describe('iptvDevice helpers', () => {
  it('rewrites relative playlist urls against the final url', () => {
    const out = rewritePlaylistUrls('#EXTM3U\nseg.ts\nhttp://abs/x.ts\n#C\n', 'http://cdn.example/a/b/index.m3u8');
    expect(out).toBe('#EXTM3U\nhttp://cdn.example/a/b/seg.ts\nhttp://abs/x.ts\n#C\n');
  });

  it('flags the live CDN hosts that require signed headers', () => {
    expect(livePlaybackHostNeedsSignedHeaders('liveali.cctv.cn')).toBe(true);
    expect(livePlaybackHostNeedsSignedHeaders('LIVETEN.cctv.cn')).toBe(true);
    expect(livePlaybackHostNeedsSignedHeaders('cdn.example')).toBe(false);
  });
});
