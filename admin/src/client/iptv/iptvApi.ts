// bkliveinfo stream resolution, ported from `bk_playurls` in ysp-live.py.
// The endpoint has no CORS headers, so in production this must go through
// `desktopAwareFetch` (Tauri http on desktop, the bridged fetch inside the
// mobile WebView). A plain browser session will fail and surfaces as
// `IptvResolveError`.

import { desktopAwareFetch, isDesktopShell } from '../api/apiBase';
import type { IptvChannel } from './iptvChannels';
import { makeIptvCkey } from './iptvCrypto';

const BKLIVEINFO_URL = 'https://bkliveinfo.ysp.cctv.cn/';
const BK_H264 = btoa('H(30:1080,60:1080|30:1080,60:1080)');
const REQUEST_TIMEOUT_MS = 15_000;

export class IptvResolveError extends Error {
  constructor(
    message: string,
    readonly cause?: unknown,
  ) {
    super(message);
    this.name = 'IptvResolveError';
  }
}

// The bkliveinfo endpoint sends no CORS headers, so resolution only works where
// fetch is bridged outside the browser: the desktop Tauri shell or the mobile
// WebView (react-native-webview injects `window.ReactNativeWebView`).
export function isIptvSupportedShell(): boolean {
  if (isDesktopShell()) return true;
  return typeof window !== 'undefined' && 'ReactNativeWebView' in window;
}

type FetchLike = (url: string, init?: RequestInit) => Promise<Response>;

export interface ResolveOptions {
  fetchImpl?: FetchLike;
  signal?: AbortSignal;
}

interface BkliveinfoResponse {
  iretcode?: number | string;
  errinfo?: string;
  playurl?: string;
  backurl_list?: unknown;
  backurlList?: unknown;
  backurl?: unknown;
}

export async function resolveIptvChannel(channel: IptvChannel, options: ResolveOptions = {}): Promise<string[]> {
  const fetchImpl = options.fetchImpl ?? desktopAwareFetch;
  const token = makeIptvCkey(channel.sid);
  const query = new URLSearchParams({
    atime: '120',
    livepid: channel.pid,
    cnlid: channel.sid,
    appVer: 'V8.22.1035.3031',
    app_version: '300090',
    caplv: '1',
    cmd: '2',
    defn: channel.defn,
    device: 'iPhone',
    encryptVer: '4.2',
    getpreviewinfo: '0',
    hevclv: '0',
    lang: 'zh-Hans_CN',
    livequeue: '0',
    logintype: '1',
    nettype: '1',
    newnettype: '1',
    newplatform: '4330403',
    platform: '4330403',
    sdtfrom: 'v3021',
    spacode: '23',
    spaudio: '1',
    spdemuxer: '6',
    spdrm: '2',
    spdynamicrange: '1',
    spflv: '1',
    spflvaudio: '1',
    sphdrfps: '60',
    sphttps: '1',
    spvcode: BK_H264,
    spvideo: '4',
    stream: '1',
    system: '1',
    sysver: 'ios18.2.1',
    uhd_flag: '0',
    cKey: token.cKey,
    guid: token.guid,
    fntick: String(token.ts),
    flowid: token.flowId,
    playbacktime: '0',
  });

  let response: Response;
  try {
    response = await fetchImpl(`${BKLIVEINFO_URL}?${query.toString()}`, {
      headers: { 'User-Agent': 'qqlive', Accept: 'application/json' },
      signal: options.signal ?? AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });
  } catch (error) {
    throw new IptvResolveError('stream resolve request failed', error);
  }
  if (!response.ok) throw new IptvResolveError(`stream resolve failed: http ${String(response.status)}`);

  let payload: BkliveinfoResponse;
  try {
    payload = (await response.json()) as BkliveinfoResponse;
  } catch (error) {
    throw new IptvResolveError('stream resolve returned invalid json', error);
  }
  return extractPlayUrls(payload);
}

// Resolved urls stay valid for a few minutes; reuse them across page visits
// (matches BK_URL_TTL in ysp-live.py).
const RESOLVE_TTL_MS = 300_000;
const resolveCache = new Map<string, { at: number; urls: string[] }>();

export async function resolveIptvChannelCached(channel: IptvChannel, options: ResolveOptions = {}): Promise<string[]> {
  const hit = resolveCache.get(channel.slug);
  if (hit && Date.now() - hit.at < RESOLVE_TTL_MS) return hit.urls;
  const urls = await resolveIptvChannel(channel, options);
  resolveCache.set(channel.slug, { at: Date.now(), urls });
  return urls;
}

export function extractPlayUrls(payload: BkliveinfoResponse): string[] {
  if (Number(payload.iretcode ?? -1) !== 0) {
    throw new IptvResolveError(`bkliveinfo iretcode=${String(payload.iretcode)} ${payload.errinfo ?? ''}`.trim());
  }
  const urls: string[] = [];
  if (typeof payload.playurl === 'string') urls.push(payload.playurl);
  const backup = payload.backurl_list ?? payload.backurlList ?? payload.backurl;
  if (Array.isArray(backup)) {
    for (const item of backup) {
      if (typeof item === 'string') urls.push(item);
      else if (item && typeof item === 'object') {
        const record = item as Record<string, unknown>;
        const value = record.url ?? record.playurl;
        if (typeof value === 'string') urls.push(value);
      }
    }
  } else if (typeof backup === 'string') {
    for (const part of backup.split(/[;,]/)) {
      const trimmed = part.trim();
      if (trimmed) urls.push(trimmed);
    }
  }
  const unique = [...new Set(urls)].filter((url) => url.includes('.cctv.'));
  if (unique.length === 0) throw new IptvResolveError('bkliveinfo returned no playable url');
  unique.sort((a, b) => rank(a) - rank(b) || (a < b ? -1 : a > b ? 1 : 0));
  return unique;
}

function rank(url: string): number {
  return url.includes('bklive-') ? 0 : 1;
}
