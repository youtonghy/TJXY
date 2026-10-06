import { desktopAwareFetch } from '../api/apiBase';
import { IptvResolveError, resolveIptvChannel } from './iptvApi';
import type { IptvChannel } from './iptvChannels';
import { JceDeadHostError, jceTimeshiftUrl } from './iptvJce';

// Port of the rolling-window playlist engine from the upstream ysp-live.py
// server. Direct bkliveinfo CDN URLs cannot be played by a browser: their
// playlists are effectively single-use and their segments reject plain
// requests. The upstream server instead polls the JCE timeshift API for a
// ~5 minute catchup window and keeps merging it into a rolling live playlist.
// This file replicates that engine in-page: an hls.js custom loader serves
// the assembled playlist on every manifest poll and fetches segments through
// `desktopAwareFetch`, which reaches the native networking bridge in the app
// shells (bypassing CORS and mixed-content limits).
const JCE_WINDOW_SECONDS = 300;
// Upstream v7.4 keeps 120 segments; the ~75-segment timeshift window must fit
// entirely or evicted segments re-enter the queue and rewind the timeline.
const MAX_SEGMENTS = 120;
const INITIAL_SEGMENTS = 25;
const VISIBLE_SEGMENTS = 15;
const REFRESH_INTERVAL_MS = 10_000;
// Upstream discards a window with no successful refresh for 30s so a rewound
// upstream timeline can never wedge the monotonic guard.
const STALE_RESET_MS = 30_000;
const BK_URL_TTL_MS = 300_000;
const JCE_MAX_FAILURES = 3;

// Upstream FORCE_BK: channels without JCE timeshift coverage.
const IPTV_FORCE_BK = new Set([
  'cctv11',
  'cctv12',
  'cctv14',
  'cctv15',
  'cctv16',
  'cctv164k',
  'cctv17',
  'cctv4k',
  'cctvfyjc',
  'cctvdyjc',
  'cctvhjjc',
]);

// Upstream sends these on playlist/segment fetches. Browser fetch silently
// drops forbidden headers, but the app shells' native fetch honors them.
const JCE_PLAYLIST_HEADERS = {
  'User-Agent':
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36',
};
const BK_PLAYLIST_HEADERS = {
  'User-Agent': 'qqlive',
  Referer: 'https://live.cctv.cn/',
  Accept: 'application/vnd.apple.mpegurl,application/json,*/*',
};
const SEGMENT_HEADERS = {
  UID: '0000000000000000',
  APPID: '9f5c54c4ed0e50109b800f7e28fec205',
  Referer: 'api.cctv.cn',
  'User-Agent': 'cctv_app_tv',
};

interface WindowSegment {
  dur: number;
  pdt: string;
  url: string;
}

interface StoredSegment extends WindowSegment {
  seq: number;
}

export function isIptvJceChannel(channel: IptvChannel): boolean {
  return !IPTV_FORCE_BK.has(channel.slug);
}

function segmentKey(url: string, pdt: string): string {
  if (pdt) return `pdt:${pdt}`;
  try {
    const parsed = new URL(url);
    return `${parsed.protocol}//${parsed.host}${parsed.pathname}`;
  } catch {
    return url;
  }
}

function parseWindowPlaylist(text: string, baseUrl: string): WindowSegment[] {
  const segments: WindowSegment[] = [];
  let dur = 6;
  let pdt = '';
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (line.startsWith('#EXTINF:')) {
      const parsed = Number.parseFloat(line.slice(8).split(',')[0] ?? '');
      dur = Number.isFinite(parsed) ? parsed : 6;
    } else if (line.startsWith('#EXT-X-PROGRAM-DATE-TIME:')) {
      pdt = line.slice(25);
    } else if (line && !line.startsWith('#')) {
      segments.push({ dur, pdt, url: new URL(line, baseUrl).toString() });
      pdt = '';
    }
  }
  return segments;
}

type FetchLike = (url: string, init?: RequestInit) => Promise<Response>;

interface IptvLiveDeps {
  now?: () => number;
  fetchImpl?: FetchLike;
  timeshiftUrl?: (pid: string, sid: string, start: number, end: number, stream: string) => Promise<string>;
  resolveBk?: (channel: IptvChannel) => Promise<string[]>;
}

async function fetchAbsPlaylist(fetchImpl: FetchLike, url: string, depth = 0): Promise<string> {
  const response = await fetchImpl(url, { headers: BK_PLAYLIST_HEADERS });
  if (!response.ok) throw new IptvResolveError(`playlist http ${String(response.status)}`);
  const text = await response.text();
  const finalUrl = response.url || url;
  if (depth < 2 && text.includes('#EXT-X-STREAM-INF')) {
    const lines = text.split('\n');
    for (let i = 0; i < lines.length; i++) {
      if (!lines[i]?.trim().startsWith('#EXT-X-STREAM-INF')) continue;
      for (let j = i + 1; j < lines.length; j++) {
        const candidate = lines[j]?.trim() ?? '';
        if (candidate && !candidate.startsWith('#')) {
          return fetchAbsPlaylist(fetchImpl, new URL(candidate, finalUrl).toString(), depth + 1);
        }
      }
      break;
    }
  }
  return text
    .split('\n')
    .map((line) => {
      const trimmed = line.trim();
      return trimmed && !trimmed.startsWith('#') ? new URL(trimmed, finalUrl).toString() : line;
    })
    .join('\n');
}

/**
 * Holds the rolling live state for one channel: the merged JCE catchup
 * window, or a resolved bkliveinfo playlist for channels without timeshift.
 */
export class IptvLiveSession {
  private mode: 'jce' | 'bk';
  private readonly segments = new Map<string, StoredSegment>();
  private readonly order: string[] = [];
  private seq = 0;
  private lastRefresh = 0;
  private refreshing?: Promise<void>;
  private jceFailures = 0;
  private lastPdt = '';
  private bkUrls: string[] = [];
  private bkUrlsAt = 0;
  private bkPlaylist = '';
  private bkPlaylistAt = 0;
  private lastError = '';
  private readonly now: () => number;
  private readonly fetchImpl: FetchLike;
  private readonly timeshiftUrl: NonNullable<IptvLiveDeps['timeshiftUrl']>;
  private readonly resolveBk: NonNullable<IptvLiveDeps['resolveBk']>;

  constructor(
    private readonly channel: IptvChannel,
    deps: IptvLiveDeps = {},
  ) {
    this.mode = isIptvJceChannel(channel) ? 'jce' : 'bk';
    this.now = deps.now ?? (() => Date.now());
    this.fetchImpl = deps.fetchImpl ?? desktopAwareFetch;
    this.timeshiftUrl = deps.timeshiftUrl ?? jceTimeshiftUrl;
    this.resolveBk = deps.resolveBk ?? resolveIptvChannel;
  }

  /** Returns the current media playlist, refreshing the window when stale. */
  async manifest(): Promise<string> {
    const stale = this.now() - this.lastRefresh > REFRESH_INTERVAL_MS;
    if (stale || !this.hasContent()) {
      try {
        await this.refresh();
      } catch (error) {
        if (!this.hasContent()) throw error;
      }
    }
    const playlist = this.buildPlaylist();
    if (!playlist) {
      throw new IptvResolveError(this.lastError || 'empty playlist');
    }
    return playlist;
  }

  private hasContent(): boolean {
    return this.mode === 'bk' ? this.bkPlaylist.length > 0 : this.order.length > 0;
  }

  private refresh(): Promise<void> {
    this.refreshing ??= this.refreshOnce()
      .then(() => {
        this.lastRefresh = this.now();
      })
      .finally(() => {
        this.refreshing = undefined;
      });
    return this.refreshing;
  }

  private async refreshOnce(): Promise<void> {
    // lastRefresh only advances on success, so a gap this long means the
    // merged window is stale: reseed instead of extending a dead timeline.
    if (this.lastRefresh > 0 && this.now() - this.lastRefresh > STALE_RESET_MS) {
      this.segments.clear();
      this.order.length = 0;
      this.lastPdt = '';
      this.bkPlaylist = '';
    }
    try {
      if (this.mode === 'jce') {
        try {
          await this.jceRefresh();
          this.jceFailures = 0;
        } catch (error) {
          this.jceFailures += 1;
          if (error instanceof JceDeadHostError || this.jceFailures >= JCE_MAX_FAILURES) {
            this.mode = 'bk';
          } else {
            throw error;
          }
        }
      }
      if (this.mode === 'bk') await this.bkRefresh();
      this.lastError = '';
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
      throw error;
    }
  }

  private async jceRefresh(): Promise<void> {
    const nowSec = Math.floor(this.now() / 1000);
    const m3u8Url = await this.timeshiftUrl(
      this.channel.pid,
      this.channel.sid,
      nowSec - JCE_WINDOW_SECONDS,
      nowSec,
      this.channel.defn,
    );
    const response = await this.fetchImpl(m3u8Url, { headers: JCE_PLAYLIST_HEADERS });
    if (!response.ok) throw new IptvResolveError(`window playlist http ${String(response.status)}`);
    const text = await response.text();
    const windowSegments = parseWindowPlaylist(text, response.url || m3u8Url);
    if (!windowSegments.length) throw new IptvResolveError('empty window playlist');
    if (!this.order.length) {
      // Seed only the newest slice of the first window (upstream: last 25).
      for (const segment of windowSegments.slice(-INITIAL_SEGMENTS)) {
        const key = segmentKey(segment.url, segment.pdt);
        this.seq += 1;
        this.segments.set(key, { ...segment, seq: this.seq });
        this.order.push(key);
        if (segment.pdt) this.lastPdt = segment.pdt;
      }
    } else {
      for (const segment of windowSegments) {
        // Monotonic timeline guard: segments at or below the last consumed
        // PDT were already served; re-appending them rewinds the playlist
        // timeline and stalls the decoder (upstream v7.4 fix).
        if (segment.pdt && this.lastPdt && segment.pdt <= this.lastPdt) continue;
        const key = segmentKey(segment.url, segment.pdt);
        const existing = this.segments.get(key);
        if (existing) {
          existing.url = segment.url;
          continue;
        }
        this.seq += 1;
        this.segments.set(key, { ...segment, seq: this.seq });
        this.order.push(key);
        if (segment.pdt) this.lastPdt = segment.pdt;
      }
    }
    while (this.order.length > MAX_SEGMENTS) {
      const dropped = this.order.shift();
      if (dropped !== undefined) this.segments.delete(dropped);
    }
  }

  private async bkRefresh(): Promise<void> {
    if (this.bkPlaylist && this.now() - this.bkPlaylistAt <= REFRESH_INTERVAL_MS) return;
    const now = this.now();
    if (now - this.bkUrlsAt > BK_URL_TTL_MS || !this.bkUrls.length) {
      this.bkUrls = await this.resolveBk(this.channel);
      this.bkUrlsAt = now;
    }
    let lastError: unknown = new IptvResolveError('empty playlist');
    for (let attempt = 0; attempt < 2; attempt++) {
      for (const url of this.bkUrls) {
        try {
          const playlist = await fetchAbsPlaylist(this.fetchImpl, url);
          if (!playlist.includes('#EXTM3U')) continue;
          this.bkPlaylist = playlist;
          this.bkPlaylistAt = this.now();
          return;
        } catch (error) {
          lastError = error;
        }
      }
      if (attempt === 0) {
        try {
          this.bkUrls = await this.resolveBk(this.channel);
          this.bkUrlsAt = this.now();
        } catch {
          // Keep the previous error; the next attempt reuses whatever we have.
        }
      }
    }
    this.bkUrlsAt = 0;
    throw lastError instanceof Error ? lastError : new IptvResolveError(String(lastError));
  }

  private buildPlaylist(): string {
    if (this.mode === 'bk') return this.bkPlaylist || '';
    const keys = this.order.slice(-VISIBLE_SEGMENTS);
    const segments = keys
      .map((key) => this.segments.get(key))
      .filter((segment): segment is StoredSegment => segment !== undefined);
    if (!segments.length) return '';
    return renderPlaylist(segments, false);
  }
}

function renderPlaylist(segments: StoredSegment[], endlist: boolean): string {
  const target = Math.max(6, ...segments.map((segment) => Math.floor(segment.dur + 0.5)));
  const lines = [
    '#EXTM3U',
    '#EXT-X-VERSION:3',
    `#EXT-X-TARGETDURATION:${String(target)}`,
    `#EXT-X-MEDIA-SEQUENCE:${String(segments[0]?.seq ?? 0)}`,
  ];
  for (const segment of segments) {
    if (segment.pdt) lines.push(`#EXT-X-PROGRAM-DATE-TIME:${segment.pdt}`);
    lines.push(`#EXTINF:${segment.dur.toFixed(3)},`, segment.url);
  }
  if (endlist) lines.push('#EXT-X-ENDLIST');
  return `${lines.join('\n')}\n`;
}

interface IptvReplayDeps {
  fetchImpl?: FetchLike;
  timeshiftUrl?: IptvLiveDeps['timeshiftUrl'];
}

/**
 * Resolves a finished programme's catchup window through the JCE timeshift
 * API and returns it as a static VOD playlist (absolute segment URLs and
 * `#EXT-X-ENDLIST`), which hls.js can seek like any recorded asset.
 */
export async function resolveIptvReplay(
  channel: IptvChannel,
  startSec: number,
  endSec: number,
  deps: IptvReplayDeps = {},
): Promise<string> {
  const timeshiftUrl = deps.timeshiftUrl ?? jceTimeshiftUrl;
  const fetchImpl = deps.fetchImpl ?? desktopAwareFetch;
  const m3u8Url = await timeshiftUrl(channel.pid, channel.sid, startSec, endSec, channel.defn);
  const response = await fetchImpl(m3u8Url, { headers: JCE_PLAYLIST_HEADERS });
  if (!response.ok) throw new IptvResolveError(`replay playlist http ${String(response.status)}`);
  const text = await response.text();
  const segments = parseWindowPlaylist(text, response.url || m3u8Url);
  if (!segments.length) throw new IptvResolveError('empty replay playlist');
  return renderPlaylist(
    segments.map((segment, index) => ({ ...segment, seq: index })),
    true,
  );
}

// hls.js plumbing: a custom loader lets the rolling session answer playlist
// polls while segment requests go through the native-capable fetch. Using it
// requires `enableWorker: false` because custom loaders cannot run inside
// hls.js's worker.
const SENTINEL_URL = 'http://tjxy-iptv.invalid/live.m3u8';
const SEGMENT_CONTEXT_TYPES = new Set(['media-fragment', 'key']);

type HlsModule = typeof import('hls.js');
type LoaderContext = import('hls.js').LoaderContext;
type LoaderConfiguration = import('hls.js').LoaderConfiguration;
type LoaderCallbacks = import('hls.js').LoaderCallbacks<LoaderContext>;
type LoaderStats = import('hls.js').LoaderStats;
type LoaderResponse = import('hls.js').LoaderResponse;

function makeStats(): LoaderStats {
  const empty = { start: 0, first: 0, end: 0 };
  return {
    aborted: false,
    loaded: 0,
    retry: 0,
    total: 0,
    chunkCount: 0,
    bwEstimate: 0,
    loading: { ...empty },
    parsing: { start: 0, end: 0 },
    buffering: { ...empty },
  };
}

function createLoader(manifestProvider: () => Promise<string>) {
  return class IptvLoader {
    context: LoaderContext | null = null;
    stats: LoaderStats = makeStats();
    private controller: AbortController | null = null;
    private timedOut = false;

    destroy(): void {
      this.abort();
    }

    abort(): void {
      this.stats.aborted = true;
      this.controller?.abort();
    }

    load(context: LoaderContext, config: LoaderConfiguration, callbacks: LoaderCallbacks): void {
      this.context = context;
      this.stats = makeStats();
      this.stats.loading.start = performance.now();
      this.timedOut = false;
      const controller = new AbortController();
      this.controller = controller;
      const timeoutMs = config.loadPolicy.maxLoadTimeMs;
      const timer = setTimeout(() => {
        this.timedOut = true;
        controller.abort();
      }, timeoutMs);

      void this.fetch(context, controller.signal)
        .then((response) => {
          clearTimeout(timer);
          this.stats.loading.end = performance.now();
          this.stats.loaded =
            response.data instanceof ArrayBuffer
              ? response.data.byteLength
              : typeof response.data === 'string'
                ? response.data.length
                : 0;
          this.stats.total = this.stats.loaded;
          this.stats.chunkCount = 1;
          callbacks.onSuccess(response, this.stats, context, null);
        })
        .catch((error: unknown) => {
          clearTimeout(timer);
          this.stats.loading.end = performance.now();
          if (this.timedOut) {
            callbacks.onTimeout(this.stats, context, null);
            return;
          }
          if (controller.signal.aborted) {
            this.stats.aborted = true;
            callbacks.onAbort?.(this.stats, context, null);
            return;
          }
          const code = error instanceof HttpError ? error.status : 0;
          callbacks.onError({ code, text: error instanceof Error ? error.message : String(error) }, context, null, this.stats);
        });
    }

    private async fetch(context: LoaderContext, signal: AbortSignal): Promise<LoaderResponse> {
      if (!SEGMENT_CONTEXT_TYPES.has(context.type)) {
        const body = await manifestProvider();
        this.stats.loading.first = performance.now();
        return { url: context.url, data: body };
      }
      const response = await desktopAwareFetch(context.url, {
        headers: SEGMENT_HEADERS,
        signal,
      });
      if (!response.ok) throw new HttpError(response.status);
      this.stats.loading.first = performance.now();
      const data = context.responseType === 'arraybuffer' ? await response.arrayBuffer() : await response.text();
      return { url: response.url || context.url, data };
    }
  };
}

class HttpError extends Error {
  constructor(readonly status: number) {
    super(`HTTP ${String(status)}`);
  }
}

function attachWithProvider(
  Hls: HlsModule['default'],
  video: HTMLVideoElement,
  manifestProvider: () => Promise<string>,
  onFatalError: () => void,
): () => void {
  const hls = new Hls({ enableWorker: false, loader: createLoader(manifestProvider) });
  let mediaRecoveries = 0;
  let networkRecoveries = 0;
  hls.on(Hls.Events.ERROR, (_event, data) => {
    if (!data.fatal) return;
    if (data.type === Hls.ErrorTypes.MEDIA_ERROR && mediaRecoveries < 1) {
      mediaRecoveries += 1;
      hls.recoverMediaError();
      return;
    }
    if (data.type === Hls.ErrorTypes.NETWORK_ERROR && networkRecoveries < 3) {
      networkRecoveries += 1;
      hls.startLoad();
      return;
    }
    onFatalError();
  });
  hls.loadSource(SENTINEL_URL);
  hls.attachMedia(video);
  return () => {
    hls.destroy();
    video.removeAttribute('src');
    video.load();
  };
}

/**
 * Attaches a live channel to the video element through hls.js and the rolling
 * JCE playlist session. Returns `undefined` when MediaSource/hls.js is
 * unavailable so the caller can fall back to native playback.
 */
export async function attachIptvLive(
  video: HTMLVideoElement,
  channel: IptvChannel,
  onFatalError: () => void,
): Promise<(() => void) | undefined> {
  const hlsModule: HlsModule = await import('hls.js');
  const Hls = hlsModule.default;
  if (!Hls.isSupported()) return undefined;
  const session = new IptvLiveSession(channel);
  return attachWithProvider(Hls, video, () => session.manifest(), onFatalError);
}

/**
 * Attaches a finished programme's catchup window as on-demand playback.
 * Without MediaSource the element falls back to playing the raw catchup
 * URL natively, which works because the playlist is a static window.
 */
export async function attachIptvReplay(
  video: HTMLVideoElement,
  channel: IptvChannel,
  programme: { start: number; stop: number },
  onFatalError: () => void,
): Promise<() => void> {
  const hlsModule: HlsModule = await import('hls.js');
  const Hls = hlsModule.default;
  const startSec = Math.floor(programme.start / 1000);
  const endSec = Math.floor(programme.stop / 1000);
  if (!Hls.isSupported()) {
    const url = await jceTimeshiftUrl(channel.pid, channel.sid, startSec, endSec, channel.defn);
    video.src = url;
    return () => {
      video.removeAttribute('src');
      video.load();
    };
  }
  const playlist = resolveIptvReplay(channel, startSec, endSec);
  return attachWithProvider(Hls, video, () => playlist, onFatalError);
}
