// In-page port of the upstream ysp-live.py device-registration engine: the
// "high-bitrate" path used for the 27 channels present in the upstream
// `channels()` map (liveId on IptvChannel). A bootstrap registers a spoofed TV
// build fingerprint, exchanges an AES session key through app/start, and the
// per-channel resolve chains live/v1/01 -> live/v1/02 -> vdn getstream into a
// signed CDN playlist. Failures drop the session and surface so the caller can
// fall back to the JCE/bkliveinfo rolling playlist, exactly like upstream.

import { desktopAwareFetch } from '../api/apiBase';
import type { IptvChannel } from './iptvChannels';
import {
  IPTV_DEVICE_AK,
  aesGcmDecryptB64,
  aesGcmEncryptB64,
  buildVdnAppcommon,
  compactJsonBytes,
  computeFingerprint,
  computeVdnCode,
  formEncode,
  formUrlencodeValue,
  javaStringHashcode,
  javaUuidFromHashes,
  rsaEncryptDeviceId,
  sha1Upper,
  uuid4,
} from './iptvDeviceCrypto';
import { IPTV_DEVICE_PROFILE_POOL, type IptvDeviceProfileTemplate } from './iptvDeviceProfiles';

const CLOUD_GET_URL = 'https://ytpcloudws.cctv.cn/cloudps/wssapi/device/v2/get';
const CLOUD_REGISTER_URL = 'https://ytpcloudws.cctv.cn/cloudps/wssapi/device/v2/register';
const APP_START_URL = 'https://ytpaddr.cctv.cn/gsnw/api/app/start/v1/01';
const DRM_CONFIG_URL = 'https://ytpaddr.cctv.cn/gsnw/drm/config/obtain/v1';
const VERSION_CONFIG_URL = 'https://ytpaddr.cctv.cn/gsnw/version/config/obtain/v1';
const DICTIONARY_URL = 'https://ytpaddr.cctv.cn/gsnw/player/dictionary/obtain/v1';
const INDEX_URL = 'https://ytpaddr.cctv.cn/gsnw/api/index/v1/01';
const REPORT_SINGLE_URL = 'https://ytpdata.cctv.cn/das/app/data/message/single';
const COLLECT_REPORT_URL = 'https://collect.cctv.cn/cctvmobileinf/rest/cctv/receive/new/app';
const LIVE_V1_01_URL = 'https://ytpaddr.cctv.cn/gsnw/api/live/v1/01';
const LIVE_V1_02_URL = 'https://ytpaddr.cctv.cn/gsnw/api/live/v1/02';
const VDN_GETSTREAM_URL = 'https://ytpvdn.cctv.cn/cctvmobileinf/rest/cctv/videoliveUrl/getstream';
const DEFAULT_LIVE_USER_ID = 'BAEBFF2B-C516-4F34-ABC0-A824A6461CBD';
const DEFAULT_DEVICE_NAME = '央视频电视投屏助手';
const VDN_APP_CHANNEL = 'dangbei';
const VDN_APP_VERSION = '1.4.1';
const REPORT_APP_KEY = '1178c84d-4818-44ff-b415-02106e87e144';
const COLLECT_SDK_VERSION = '1.0.0';
const DEFAULT_PAGE_NAME = 'com.cctv.tv.mvp.ui.activity.MainActivity';
const DEFAULT_ACCEPT_LANGUAGE = 'zh-CN,zh;q=0.8';
const RESULT_OK = 0;
const RESULT_NEEDS_REGISTER = 601;
const RESULT_GET_MISSING_OR_INVALID = 2;
const RESULT_REGISTERED_ELSEWHERE = 694;
const RESULT_REGISTER_RETRY_LATER = 695;

// Upstream engine_args: control pacing, entry cache, session and heartbeat.
const CONTROL_INTERVAL_MS = 1_000;
const ENTRY_TTL_MS = 600_000;
const STALE_WHILE_REFRESH_MS = 120_000;
const REFRESH_ERROR_COOLDOWN_MS = 30_000;
const COOLDOWN_MAX_MS = 300_000;
const SESSION_TTL_MS = 7_200_000;
const SESSION_RENEW_THRESHOLD_MS = 300_000;
const HEARTBEAT_INTERVAL_MS = 30_000;
const DEVICE_STATE_KEY = 'tjxy-iptv-device-state';
const REQUEST_TIMEOUT_MS = 15_000;

export class IptvDeviceError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'IptvDeviceError';
  }
}

export interface IptvDeviceProfile {
  androidId: string;
  mac: string;
  hardware: string;
  board: string;
  brand: string;
  device: string;
  manufacturer: string;
  model: string;
  product: string;
  tags: string;
  buildType: string;
  user: string;
  resolution: string;
  display: string;
  versionId: string;
  host: string;
  fingerprint: string;
  reportModel: string;
}

interface IptvDeviceState {
  schemaVersion: number;
  profileSource: string;
  profile: IptvDeviceProfile;
  screenParam: string;
  castModel: string;
  xUid: string;
  cloudGuid: string;
  registeredAt: number;
  updatedAt: number;
}

interface IptvIdentity {
  xUid: string;
  xFingerprint: string;
  fingerprintTimestampMs: number;
  headers: Record<string, string>;
}

export interface IptvDeviceEntry {
  slug: string;
  finalUrl: string;
  playbackHeaders: Record<string, string>;
  androidId: string;
  finalHost: string;
  refreshedAt: number;
  expiresAt: number;
}

interface IptvAppSession {
  profile: IptvDeviceProfile;
  identity: IptvIdentity;
  sessionKey: string;
  cloudGuid: string;
  screenParam: string;
  castModel: string;
  createdAt: number;
}

type FetchLike = (url: string, init?: RequestInit) => Promise<Response>;
type RandomSource = (length: number) => Uint8Array;
type KeyValueStore = Pick<Storage, 'getItem' | 'setItem'>;

export interface IptvDeviceDeps {
  fetchImpl?: FetchLike;
  now?: () => number;
  randomBytes?: RandomSource;
  storage?: KeyValueStore | null;
  sleep?: (ms: number) => Promise<void>;
}

function defaultRandom(length: number): Uint8Array {
  const out = new Uint8Array(length);
  crypto.getRandomValues(out);
  return out;
}

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

function defaultStorage(): KeyValueStore | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

function sanitizeProfileId(value: string): string {
  let out = '';
  let lastUnderscore = false;
  for (const ch of value) {
    if (/^[\x20-\x7e]$/.test(ch) && /[a-z0-9]/i.test(ch)) {
      out += ch.toLowerCase();
      lastUnderscore = false;
    } else if (!lastUnderscore) {
      out += '_';
      lastUnderscore = true;
    }
  }
  return out.replace(/^_+|_+$/g, '');
}

function resolutionFromScreenParam(screenParam: string): string {
  const parts = screenParam.split('-');
  if (parts.length >= 2 && parts[0] && parts[1]) return `${parts[0]}*${parts[1]}`;
  return '3840*2160';
}

function randomHexString(length: number, randomBytes: RandomSource): string {
  const hex = '0123456789abcdef';
  const bytes = randomBytes(length);
  let out = '';
  for (const byte of bytes) out += hex[byte >> 4] ?? '0';
  return out;
}

function randomMacAddress(randomBytes: RandomSource): string {
  const raw = randomBytes(6);
  raw[0] = ((raw[0] ?? 0) | 2) & 254;
  return [...raw].map((byte) => byte.toString(16).padStart(2, '0')).join(':');
}

export function deviceProfileFromTemplate(
  template: IptvDeviceProfileTemplate,
  androidId: string,
  mac: string,
): IptvDeviceProfile {
  const brandId = sanitizeProfileId(template.brand);
  const modelId = sanitizeProfileId(template.model);
  const device = `${brandId}_${modelId}`;
  return {
    androidId,
    mac,
    hardware: template.hardware,
    board: template.board,
    brand: template.brand,
    device,
    manufacturer: template.manufacturer,
    model: template.model,
    product: device,
    tags: 'release-keys',
    buildType: 'user',
    user: 'build',
    resolution: resolutionFromScreenParam(template.screen_param),
    display: `${template.model}-user 13 ${template.version_id} 2024 release-keys`,
    versionId: template.version_id,
    host: `${brandId}-tv-build`,
    fingerprint: `${template.manufacturer}/${device}/${device}:13/${template.version_id}/2024:user/release-keys`,
    reportModel: template.report_model,
  };
}

export function computeXUid(profile: IptvDeviceProfile): string {
  const buildIdentityStr =
    '1698' + profile.hardware + profile.board + profile.brand + profile.device + profile.manufacturer
    + profile.model + profile.product + profile.tags + profile.buildType + profile.user + profile.resolution + profile.mac;
  const uuidPart = javaUuidFromHashes(javaStringHashcode(buildIdentityStr), javaStringHashcode(profile.model));
  return sha1Upper(`${profile.androidId}|${uuidPart}`);
}

function inferOsVersion(profile: IptvDeviceProfile): string {
  const colon = profile.fingerprint.indexOf(':');
  if (colon < 0) return '';
  const tail = profile.fingerprint.slice(colon + 1);
  const slash = tail.indexOf('/');
  return slash >= 0 ? tail.slice(0, slash) : '';
}

function inferSdkInt(profile: IptvDeviceProfile): string {
  const os = inferOsVersion(profile);
  const major = os ? os.split('.')[0] : '';
  return { '13': '33', '12': '31', '11': '30', '10': '29', '9': '28', '8': '26', '7': '24', '6': '23' }[major ?? ''] ?? '';
}

function appStartField(value: unknown, limit: number): string {
  const text = String(value);
  return limit >= 0 ? text.slice(0, limit) : text;
}

function buildReportCommonValue(
  profile: IptvDeviceProfile,
  xUid: string,
  appChannel: string,
  version: string,
  sdkVersion: string,
  dataTimeMs: number,
): Record<string, unknown> {
  const model = profile.reportModel || profile.model.replaceAll(profile.manufacturer, '').replaceAll(' ', '');
  return {
    cctv_id: appStartField(xUid, 64),
    device_id: appStartField(profile.androidId, 64),
    idfa: '',
    idfv: '',
    user_id: '',
    app_key: appStartField(REPORT_APP_KEY, 64),
    imei: '',
    android_id: appStartField(profile.androidId, 64),
    mac: appStartField(profile.mac, 64),
    device_builder_type: appStartField(profile.buildType, 64),
    device_hardware: appStartField(profile.hardware, 64),
    device_board: appStartField(profile.board, 64),
    device_brand: appStartField(profile.brand, 64),
    device_params: appStartField(profile.device, 64),
    device_display: appStartField(profile.display, 64),
    device_version_id: appStartField(profile.versionId, 64),
    device_host: appStartField(profile.host, 128),
    device_product: appStartField(profile.product, 64),
    device_tags: appStartField(profile.tags, 64),
    device_user: appStartField(profile.user, 30),
    device_fingerprint: appStartField(profile.fingerprint, 128),
    device_manufacturer: appStartField(profile.manufacturer, 64),
    device_model: appStartField(model, 50),
    device_resolution: appStartField(profile.resolution, 20),
    system_type: 'Android',
    device_type: 'TV',
    app_language: 'CHINESE',
    app_version: appStartField(version, 30),
    sdk_version: appStartField(sdkVersion, 30),
    os_version: appStartField(inferOsVersion(profile), 20),
    app_channel: appStartField(appChannel, 50),
    data_time: appStartField(dataTimeMs, 13),
  };
}

function buildIdentity(profile: IptvDeviceProfile, nowMs: number): IptvIdentity {
  const xUid = computeXUid(profile);
  const [xFingerprint, ts] = computeFingerprint(xUid, nowMs);
  return {
    xUid,
    xFingerprint: xFingerprint,
    fingerprintTimestampMs: ts,
    headers: {
      Accept: 'application/json',
      'Accept-Language': DEFAULT_ACCEPT_LANGUAGE,
      Referer: 'api.cctv.cn',
      'User-Agent': 'cctv_app_tv',
      UID: profile.androidId,
      appChannel: VDN_APP_CHANNEL,
      'X-Uid': xUid,
      'X-Fingerprint': xFingerprint,
      'X-Version': VDN_APP_VERSION,
      'Content-Type': 'application/json; charset=utf-8',
      Connection: 'Keep-Alive',
      'Accept-Encoding': 'gzip',
      'Cache-Control': 'no-cache',
    },
  };
}

function freshHeaders(
  template: Record<string, string>,
  contentType: string,
  randomBytes: RandomSource,
  accept?: string,
  forceTs?: number,
): Record<string, string> {
  const headers: Record<string, string> = {};
  if (accept !== undefined) headers.Accept = accept;
  headers['X-Timestamp'] = String(forceTs ?? Date.now());
  headers['X-Nonce'] = uuid4(randomBytes);
  for (const key of ['Accept-Language', 'Referer', 'User-Agent', 'UID', 'appChannel', 'X-Uid', 'X-Fingerprint', 'X-Version']) {
    const value = template[key];
    if (value !== undefined) headers[key] = value;
  }
  headers['Content-Type'] = contentType;
  for (const key of ['Connection', 'Accept-Encoding', 'Cache-Control']) {
    const value = template[key];
    if (value !== undefined) headers[key] = value;
  }
  return headers;
}

function rootHeaders(version: string, randomBytes: RandomSource): Record<string, string> {
  return {
    'X-Uid': 'ROOT',
    'X-Fingerprint': 'ROOT',
    'X-Nonce': uuid4(randomBytes),
    'X-Timestamp': String(Date.now()),
    'X-Version': version,
    UID: 'ROOT',
    Referer: 'api.cctv.cn',
    'User-Agent': 'cctv_app_tv',
    appChannel: 'ROOT',
    Connection: 'Keep-Alive',
    'Accept-Encoding': 'gzip',
  };
}

function collectHeaders(profile: IptvDeviceProfile): Record<string, string> {
  const release = inferOsVersion(profile);
  return {
    'Content-type': 'application/x-www-form-urlencoded',
    Charset: 'UTF-8',
    'User-Agent': `Dalvik/2.1.0 (Linux; U; Android ${release || 'Android'}; ${profile.model} Build/${profile.versionId})`,
    Connection: 'Keep-Alive',
    'Accept-Encoding': 'gzip',
  };
}

function parseResultCode(value: unknown): number | null {
  if (value === null || typeof value !== 'object') return null;
  const record = value as Record<string, unknown>;
  for (const key of ['result', 'code', 'errCode', 'errcode', 'ret']) {
    const raw = record[key];
    if (typeof raw === 'boolean') continue;
    if (typeof raw === 'number') return raw;
    if (typeof raw === 'string') {
      const parsed = Number.parseInt(raw, 10);
      if (!Number.isNaN(parsed)) return parsed;
    }
  }
  for (const key of ['data', 'error', 'response']) {
    const found = parseResultCode(record[key]);
    if (found !== null) return found;
  }
  return null;
}

function extractGuid(value: unknown): string {
  if (value === null || typeof value !== 'object') return '';
  const data = (value as Record<string, unknown>).data;
  if (data !== null && typeof data === 'object') {
    const guid = (data as Record<string, unknown>).guid;
    if (typeof guid === 'string') return guid;
  }
  return '';
}

function urlHost(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return '';
  }
}

export function livePlaybackHostNeedsSignedHeaders(host: string): boolean {
  const lower = host.toLowerCase();
  return lower.includes('liveali') || lower.includes('liveten');
}

function tsPathMatchesFinalUrl(tsPath: string, finalUrl: string): boolean {
  let prefix: string;
  try {
    prefix = new URL(finalUrl).pathname;
  } catch {
    return false;
  }
  if (prefix.endsWith('.m3u8')) prefix = prefix.slice(0, -5);
  return prefix !== '' && tsPath.startsWith(prefix);
}

// Upstream session-invalidation predicates: these failures mean the app
// session (or identity) must be rebuilt rather than retried in place.
export function isSessionInvalidatingError(message: string): boolean {
  const lower = message.toLowerCase();
  const sessionCore400 =
    (lower.includes('http 400') || lower.includes('status=400'))
    && ['app/start', 'live/v1/01', 'live/v1/02', 'vdn', 'heartbeat'].some((n) => lower.includes(n));
  return (
    lower.includes('app/start')
    || lower.includes('decrypt')
    || lower.includes('session_key')
    || lower.includes('missing encrypted session key')
    || sessionCore400
    || lower.includes('live/v1/01 http 400')
    || lower.includes('live/v1/01 http 401')
    || lower.includes('live/v1/01 http 403')
    || lower.includes('live/v1/02 http 400')
    || lower.includes('live/v1/02 http 401')
    || lower.includes('live/v1/02 http 403')
  );
}

export function rewritePlaylistUrls(text: string, baseUrl: string): string {
  let hasBase = false;
  try {
    const parsed = new URL(baseUrl);
    hasBase = Boolean(parsed.protocol && parsed.host);
  } catch {
    hasBase = false;
  }
  return text
    .split('\n')
    .map((line) => {
      const stripped = line.trim();
      if (!stripped || stripped.startsWith('#')) return line;
      return hasBase ? new URL(stripped, baseUrl).toString() : stripped;
    })
    .join('\n');
}

type JsonObject = Record<string, unknown>;

export function isIptvDeviceChannel(channel: IptvChannel): boolean {
  return channel.liveId !== undefined;
}

/**
 * The device-protocol engine: owns the app session (bootstrap + heartbeat +
 * renewal), the per-channel resolved-entry cache and the failure cooldown.
 * `fetchPlaylist` throws when the device path is unavailable; callers fall
 * back to the rolling JCE/bk playlist exactly like upstream's do_GET.
 */
export class IptvDeviceEngine {
  private readonly fetchImpl: FetchLike;
  private readonly now: () => number;
  private readonly randomBytes: RandomSource;
  private readonly storage: KeyValueStore | null;
  private readonly sleep: (ms: number) => Promise<void>;
  private deviceState: IptvDeviceState | null = null;
  private appSession: IptvAppSession | null = null;
  private sessionBootstrap: Promise<IptvAppSession> | null = null;
  private readonly entries = new Map<string, IptvDeviceEntry>();
  private readonly failures = new Map<string, { failedAt: number; message: string; count: number }>();
  private readonly refreshing = new Map<string, Promise<IptvDeviceEntry>>();
  private controlChain: Promise<unknown> = Promise.resolve();
  private lastControlRequestAt = 0;
  private lastBusinessEndAt = 0;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  lastError = '';

  constructor(deps: IptvDeviceDeps = {}) {
    this.fetchImpl = deps.fetchImpl ?? desktopAwareFetch;
    this.now = deps.now ?? (() => Date.now());
    this.randomBytes = deps.randomBytes ?? defaultRandom;
    this.storage = deps.storage !== undefined ? deps.storage : defaultStorage();
    this.sleep = deps.sleep ?? defaultSleep;
  }

  isInCooldown(slug: string): boolean {
    const failure = this.failures.get(slug);
    if (!failure) return false;
    // Upstream: max(base, 30s * 2^(count-1)) capped at 300s — equivalent to
    // exponential backoff from the 30s base while base is 30s.
    const cooldown = Math.min(REFRESH_ERROR_COOLDOWN_MS * 2 ** Math.max(0, failure.count - 1), COOLDOWN_MAX_MS);
    return this.now() - failure.failedAt < cooldown;
  }

  clearCooldown(slug: string): void {
    this.failures.delete(slug);
  }

  /**
   * Kick off session bootstrap in the background. The app session handshake
   * is ~15 paced control requests; warming it while the user browses the
   * channel grid makes the first device-mode manifest poll instant.
   */
  prewarm(): void {
    void this.ensureFreshSession().then(
      () => undefined,
      () => undefined,
    );
  }

  /** Latest resolved playlist body for a channel, through the device path. */
  async fetchPlaylist(channel: IptvChannel): Promise<string> {
    if (this.isInCooldown(channel.slug)) {
      const failure = this.failures.get(channel.slug);
      throw new IptvDeviceError(`channel ${channel.slug} in cooldown: ${failure?.message ?? ''}`);
    }
    try {
      const entry = await this.ensureEntry(channel);
      const playlist = await this.fetchEntryPlaylist(entry);
      return rewritePlaylistUrls(playlist, entry.finalUrl);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.recordFailure(channel.slug, message);
      if (isSessionInvalidatingError(message)) this.appSession = null;
      this.lastError = message;
      throw error instanceof IptvDeviceError ? error : new IptvDeviceError(message);
    }
  }

  /** Signed playback headers a segment request should carry. */
  segmentHeaders(url: string): Record<string, string> {
    let parsed: URL;
    try {
      parsed = new URL(url);
    } catch {
      return this.defaultPlaybackHeaders();
    }
    const host = parsed.hostname.toLowerCase();
    let sameHost: Record<string, string> | null = null;
    for (const entry of this.entries.values()) {
      if (entry.finalHost.toLowerCase() !== host) continue;
      if (tsPathMatchesFinalUrl(parsed.pathname, entry.finalUrl)) return entry.playbackHeaders;
      sameHost ??= entry.playbackHeaders;
    }
    return sameHost ?? this.defaultPlaybackHeaders();
  }

  private defaultPlaybackHeaders(): Record<string, string> {
    return {
      UID: this.deviceState?.profile.androidId ?? '',
      APPID: IPTV_DEVICE_AK,
      Referer: 'api.cctv.cn',
      'User-Agent': 'cctv_app_tv',
    };
  }

  private recordFailure(slug: string, message: string): void {
    const previous = this.failures.get(slug);
    this.failures.set(slug, { failedAt: this.now(), message, count: (previous?.count ?? 0) + 1 });
  }

  private async ensureEntry(channel: IptvChannel): Promise<IptvDeviceEntry> {
    const now = this.now();
    const entry = this.entries.get(channel.slug);
    if (entry && now < entry.expiresAt && !this.missingSignedHeaders(entry)) return entry;
    if (entry && now < entry.expiresAt + STALE_WHILE_REFRESH_MS && !this.missingSignedHeaders(entry)) {
      // Serve the stale entry and refresh in the background (upstream
      // stale-while-refresh semantics); background failures still feed the
      // cooldown counter and invalidate the session when appropriate.
      void this.refreshEntry(channel).then(
        () => undefined,
        (error: unknown) => {
          const message = error instanceof Error ? error.message : String(error);
          this.recordFailure(channel.slug, message);
          if (isSessionInvalidatingError(message)) this.appSession = null;
          this.lastError = message;
        },
      );
      return entry;
    }
    return this.refreshEntry(channel);
  }

  private missingSignedHeaders(entry: IptvDeviceEntry): boolean {
    if (!livePlaybackHostNeedsSignedHeaders(entry.finalHost)) return false;
    return ['APPID', 'APPSIGN', 'APPRANDOMSTR'].some((key) => !(entry.playbackHeaders[key] ?? '').trim());
  }

  private refreshEntry(channel: IptvChannel): Promise<IptvDeviceEntry> {
    const liveId = channel.liveId;
    if (!liveId) return Promise.reject(new IptvDeviceError(`channel ${channel.slug} has no device live id`));
    let pending = this.refreshing.get(channel.slug);
    if (!pending) {
      pending = (async () => {
        // Business pacing: upstream waits refresh_interval since the last
        // resolve before starting a new one.
        const wait = CONTROL_INTERVAL_MS - (this.now() - this.lastBusinessEndAt);
        if (wait > 0) await this.sleep(wait);
        const session = await this.ensureFreshSession();
        const entry = await this.resolveChannelOnce(session, channel.slug, liveId);
        this.entries.set(channel.slug, entry);
        this.failures.delete(channel.slug);
        return entry;
      })().finally(() => {
        this.refreshing.delete(channel.slug);
        this.lastBusinessEndAt = this.now();
      });
      this.refreshing.set(channel.slug, pending);
    }
    return pending;
  }

  private async fetchEntryPlaylist(entry: IptvDeviceEntry): Promise<string> {
    const headers: Record<string, string> = {
      Accept: '*/*',
      'Accept-Encoding': 'identity',
      Connection: 'close',
    };
    for (const key of ['UID', 'APPID', 'APPRANDOMSTR', 'Referer', 'User-Agent', 'APPSIGN']) {
      const value = entry.playbackHeaders[key];
      if (value?.trim()) headers[key] = value.replaceAll(/[\r\n]/g, '');
    }
    const response = await this.fetchImpl(entry.finalUrl, {
      headers,
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });
    if (!response.ok) throw new IptvDeviceError(`upstream m3u8 http ${String(response.status)}`);
    return response.text();
  }

  // -- App session -----------------------------------------------------------

  private ensureFreshSession(): Promise<IptvAppSession> {
    const session = this.appSession;
    if (session && this.now() - session.createdAt < SESSION_TTL_MS) {
      const remaining = SESSION_TTL_MS - (this.now() - session.createdAt);
      if (remaining > SESSION_RENEW_THRESHOLD_MS) return Promise.resolve(session);
    }
    this.sessionBootstrap ??= this.bootstrapSession()
      .then((fresh) => {
        this.appSession = fresh;
        return fresh;
      })
      .finally(() => {
        this.sessionBootstrap = null;
      });
    return this.sessionBootstrap;
  }

  private async bootstrapSession(): Promise<IptvAppSession> {
    const deviceState = this.loadDeviceState();
    const profile = deviceState.profile;
    const identity = buildIdentity(profile, this.now());
    deviceState.xUid = identity.xUid;
    const pageSessionId = uuid4(this.randomBytes);
    await this.withControl(() => this.collectReport(profile, {
      key: 'app_start_d1',
      value: buildReportCommonValue(profile, identity.xUid, VDN_APP_CHANNEL, VDN_APP_VERSION, COLLECT_SDK_VERSION, this.now()),
    }));
    await this.dictionaryObtain();
    const sessionKey = await this.appStartFlow(profile, identity);
    try {
      await this.appEventFlow(profile, identity);
    } catch {
      // Optional telemetry.
    }
    try {
      const pageEndMs = this.now();
      const pageStartMs = pageEndMs - 1000;
      const pageValue = buildReportCommonValue(profile, identity.xUid, VDN_APP_CHANNEL, VDN_APP_VERSION, '', pageEndMs + 2);
      pageValue.start_time = String(pageStartMs);
      pageValue.end_time = String(pageEndMs);
      pageValue.duration = String(pageEndMs - pageStartMs);
      pageValue.page_name = DEFAULT_PAGE_NAME;
      pageValue.session_id = pageSessionId;
      pageValue.network_type = 'WIFI';
      await this.withControl(() => this.collectReport(profile, { key: 'page_d1', value: pageValue }));
    } catch {
      // Optional telemetry.
    }
    try {
      await this.pageEventFlow(profile, identity, pageSessionId, this.now() - 1000, this.now());
    } catch {
      // Optional telemetry.
    }
    let cloudGuid: string;
    try {
      cloudGuid = await this.cloudRegistrationFlow(identity, identity.xUid);
    } catch {
      cloudGuid = '';
    }
    if (!cloudGuid && deviceState.cloudGuid) cloudGuid = deviceState.cloudGuid;
    deviceState.updatedAt = this.now();
    if (cloudGuid) {
      deviceState.cloudGuid = cloudGuid;
      if (deviceState.registeredAt <= 0) deviceState.registeredAt = this.now();
    }
    this.saveDeviceState(deviceState);
    if (cloudGuid) {
      try {
        await this.deviceInfoReportFlow(profile, identity, cloudGuid);
      } catch {
        // Optional telemetry.
      }
    }
    await this.heartbeatFlow(profile, identity, cloudGuid);
    await this.indexFlow(identity);
    await this.warmupFlow(identity);
    this.startHeartbeat();
    return {
      profile,
      identity,
      sessionKey,
      cloudGuid,
      screenParam: deviceState.screenParam,
      castModel: deviceState.castModel,
      createdAt: this.now(),
    };
  }

  private withControl<T>(fn: () => Promise<T>): Promise<T> {
    const run = this.controlChain.then(async () => {
      const wait = Math.max(0, CONTROL_INTERVAL_MS - (this.now() - this.lastControlRequestAt));
      if (wait > 0) await this.sleep(wait);
      try {
        return await fn();
      } finally {
        this.lastControlRequestAt = this.now();
      }
    });
    this.controlChain = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  }

  private async postJson(url: string, headers: Record<string, string>, body: unknown, label: string): Promise<[number, unknown, string]> {
    const response = await this.fetchImpl(url, {
      method: 'POST',
      headers,
      body: compactJsonBytes(body),
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });
    const text = await response.text();
    let value: unknown;
    try {
      value = JSON.parse(text);
    } catch {
      value = null;
    }
    return [response.status, value, `${label} http ${String(response.status)}`];
  }

  private async collectReport(profile: IptvDeviceProfile, body: JsonObject): Promise<void> {
    const response = await this.fetchImpl(COLLECT_REPORT_URL, {
      method: 'POST',
      headers: collectHeaders(profile),
      body: formEncode([['info', compactJsonToText(body)]]),
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });
    if (!response.ok) throw new IptvDeviceError(`collect report http ${String(response.status)}`);
  }

  private async dictionaryObtain(): Promise<void> {
    await this.withControl(async () => {
      const response = await this.fetchImpl(DICTIONARY_URL, {
        method: 'POST',
        headers: rootHeaders(VDN_APP_VERSION, this.randomBytes),
        body: new Uint8Array(0),
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
      if (!response.ok) throw new IptvDeviceError(`dictionary http ${String(response.status)}`);
    });
  }

  private async appStartFlow(profile: IptvDeviceProfile, identity: IptvIdentity): Promise<string> {
    let last = '';
    for (let attempt = 1; attempt <= 4; attempt += 1) {
      if (attempt > 1) await this.sleep((attempt - 1) * 1000);
      const result = await this.withControl(async () => {
        const tokenTime = this.now();
        const [xfp, ts] = computeFingerprint(identity.xUid, tokenTime);
        identity.xFingerprint = xfp;
        identity.fingerprintTimestampMs = ts;
        identity.headers['X-Fingerprint'] = xfp;
        const body = { key: 'app_start_d1', value: buildReportCommonValue(profile, identity.xUid, VDN_APP_CHANNEL, VDN_APP_VERSION, '', this.now()) };
        const headers = freshHeaders(identity.headers, 'application/json', this.randomBytes, 'application/json', ts);
        headers.UID = '';
        const response = await this.fetchImpl(APP_START_URL, {
          method: 'POST',
          headers,
          body: compactJsonBytes(body, true),
          signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
        });
        const text = await response.text();
        return [response.status, text] as const;
      });
      const [status, text] = result;
      if (status >= 200 && status < 300) {
        let value: unknown;
        try {
          value = JSON.parse(text);
        } catch {
          value = null;
        }
        const data = value !== null && typeof value === 'object' ? (value as JsonObject).data : null;
        let encrypted = '';
        if (data !== null && typeof data === 'object') {
          const keyVal = (data as JsonObject).key ?? data;
          encrypted = typeof keyVal === 'string' ? keyVal : '';
        } else if (typeof data === 'string') {
          encrypted = data;
        }
        if (encrypted) {
          return aesGcmDecryptB64(encrypted, identity.xFingerprint.slice(0, 32));
        }
      }
      last = `app/start http ${String(status)}: ${text.slice(0, 300)}`;
    }
    throw new IptvDeviceError(last || 'app/start failed');
  }

  private async appEventFlow(profile: IptvDeviceProfile, identity: IptvIdentity): Promise<void> {
    await this.withControl(async () => {
      const eventTime = this.now();
      const value = buildReportCommonValue(profile, identity.xUid, VDN_APP_CHANNEL, VDN_APP_VERSION, '', eventTime);
      value.event_id = 'app_start';
      value.event_name = '应用启动';
      value.event_time = String(eventTime);
      value.network_type = 'WIFI';
      value.cur_version = VDN_APP_VERSION;
      value.channel = VDN_APP_CHANNEL;
      value.pre_version = VDN_APP_VERSION;
      const headers = freshHeaders(identity.headers, 'application/json', this.randomBytes, 'application/json');
      headers.UID = '';
      const response = await this.fetchImpl(REPORT_SINGLE_URL, {
        method: 'POST',
        headers,
        body: compactJsonBytes({ key: 'event', value }),
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
      if (!response.ok) throw new IptvDeviceError(`app event http ${String(response.status)}`);
    });
  }

  private async pageEventFlow(profile: IptvDeviceProfile, identity: IptvIdentity, sessionId: string, startMs: number, endMs: number): Promise<void> {
    await this.withControl(async () => {
      const value = buildReportCommonValue(profile, identity.xUid, VDN_APP_CHANNEL, VDN_APP_VERSION, '', endMs + 2);
      value.start_time = String(startMs);
      value.end_time = String(endMs);
      value.duration = String(Math.max(0, endMs - startMs));
      value.page_name = DEFAULT_PAGE_NAME;
      value.session_id = sessionId;
      value.network_type = 'WIFI';
      const response = await this.fetchImpl(REPORT_SINGLE_URL, {
        method: 'POST',
        headers: freshHeaders(identity.headers, 'application/json', this.randomBytes, 'application/json'),
        body: compactJsonBytes({ key: 'page_d1', value }),
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
      if (!response.ok) throw new IptvDeviceError(`page event http ${String(response.status)}`);
    });
  }

  private async cloudRegistrationFlow(identity: IptvIdentity, cloudDeviceId: string): Promise<string> {
    const body = { device_name: DEFAULT_DEVICE_NAME, device_id: rsaEncryptDeviceId(cloudDeviceId, this.randomBytes) };
    const headers = () => freshHeaders(identity.headers, 'application/json', this.randomBytes, 'application/json');
    const postCloud = (url: string) =>
      this.withControl(async () => {
        const response = await this.fetchImpl(url, {
          method: 'POST',
          headers: headers(),
          body: compactJsonBytes(body),
          signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
        });
        const text = await response.text();
        try {
          return JSON.parse(text) as unknown;
        } catch {
          return null;
        }
      });
    const first = await postCloud(CLOUD_GET_URL);
    const firstCode = parseResultCode(first);
    if (firstCode === RESULT_OK) return extractGuid(first);
    if (firstCode !== RESULT_NEEDS_REGISTER && firstCode !== RESULT_GET_MISSING_OR_INVALID) return '';
    let guid = '';
    let lastResult: number | null = null;
    for (let i = 0; i < 2; i += 1) {
      const value = await postCloud(CLOUD_REGISTER_URL);
      lastResult = parseResultCode(value);
      guid = extractGuid(value);
      if (guid || lastResult !== RESULT_REGISTER_RETRY_LATER) break;
    }
    if (guid) return guid;
    if (lastResult === RESULT_OK || lastResult === RESULT_REGISTERED_ELSEWHERE || lastResult === RESULT_GET_MISSING_OR_INVALID) {
      const value = await postCloud(CLOUD_GET_URL);
      return extractGuid(value);
    }
    return '';
  }

  private async deviceInfoReportFlow(profile: IptvDeviceProfile, identity: IptvIdentity, cloudGuid: string): Promise<void> {
    await this.withControl(async () => {
      const value = buildReportCommonValue(profile, identity.xUid, VDN_APP_CHANNEL, VDN_APP_VERSION, '', this.now());
      const sdkInt = inferSdkInt(profile);
      let systemInfo = inferOsVersion(profile);
      if (sdkInt) systemInfo = systemInfo ? `${systemInfo}/${sdkInt}` : sdkInt;
      Object.assign(value, {
        version: VDN_APP_VERSION,
        network_status: 'WiFi',
        device_info: `${profile.brand}-${profile.model}`,
        manufacturer: profile.manufacturer,
        cpu_info: '',
        chip_info: profile.hardware,
        ram_info: '',
        memory_info: '',
        system_info: systemInfo,
        guid: cloudGuid,
      });
      const response = await this.fetchImpl(REPORT_SINGLE_URL, {
        method: 'POST',
        headers: freshHeaders(identity.headers, 'application/json', this.randomBytes, 'application/json'),
        body: compactJsonBytes({ key: 'app_device_info', value }),
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
      if (!response.ok) throw new IptvDeviceError(`device info http ${String(response.status)}`);
    });
  }

  private async heartbeatFlow(profile: IptvDeviceProfile, identity: IptvIdentity, cloudGuid: string): Promise<string> {
    return this.withControl(async () => {
      const value = buildReportCommonValue(profile, identity.xUid, VDN_APP_CHANNEL, VDN_APP_VERSION, '', this.now());
      value.network_type = 'WiFi';
      value.guid = cloudGuid;
      value.other = '';
      const response = await this.fetchImpl(REPORT_SINGLE_URL, {
        method: 'POST',
        headers: freshHeaders(identity.headers, 'application/json', this.randomBytes, 'application/json'),
        body: compactJsonBytes({ key: 'app_heartbeat', value }),
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
      if (!response.ok) throw new IptvDeviceError(`heartbeat http ${String(response.status)}`);
      const text = await response.text();
      let parsed: unknown;
      try {
        parsed = JSON.parse(text);
      } catch {
        parsed = null;
      }
      const code = parseResultCode(parsed);
      return code === null ? '' : String(code);
    });
  }

  private async indexFlow(identity: IptvIdentity): Promise<void> {
    await this.withControl(async () => {
      const response = await this.fetchImpl(INDEX_URL, {
        method: 'POST',
        headers: freshHeaders(identity.headers, 'application/json', this.randomBytes, 'application/json'),
        body: compactJsonBytes({ channel: VDN_APP_CHANNEL, source: 'application' }),
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
      if (!response.ok) throw new IptvDeviceError(`index http ${String(response.status)}`);
    });
  }

  private async warmupFlow(identity: IptvIdentity): Promise<void> {
    const appcommon = buildVdnAppcommon(VDN_APP_VERSION);
    await this.withControl(async () => {
      const response = await this.fetchImpl(DRM_CONFIG_URL, {
        method: 'POST',
        headers: freshHeaders(identity.headers, 'application/x-www-form-urlencoded', this.randomBytes),
        body: formEncode([['appcommon', appcommon]]),
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
      if (!response.ok) throw new IptvDeviceError(`drm config http ${String(response.status)}`);
    });
    await this.withControl(async () => {
      const headers = freshHeaders(identity.headers, '', this.randomBytes);
      delete headers['Content-Type'];
      const response = await this.fetchImpl(`${VERSION_CONFIG_URL}?appcommon=${formUrlencodeValue(appcommon)}`, {
        headers,
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
      if (!response.ok) throw new IptvDeviceError(`version config http ${String(response.status)}`);
    });
  }

  private startHeartbeat(): void {
    if (this.heartbeatTimer !== null || typeof setInterval !== 'function') return;
    this.heartbeatTimer = setInterval(() => {
      void this.sendHeartbeat();
    }, HEARTBEAT_INTERVAL_MS);
    // Node-only: don't keep the test runner alive for the daemon timer.
    (this.heartbeatTimer as unknown as { unref?: () => void }).unref?.();
  }

  /** One heartbeat tick (kept separate from the timer for tests). */
  async sendHeartbeat(): Promise<void> {
    try {
      const session = await this.ensureFreshSession();
      const wait = CONTROL_INTERVAL_MS - (this.now() - this.lastBusinessEndAt);
      if (wait > 0) await this.sleep(wait);
      await this.heartbeatFlow(session.profile, session.identity, session.cloudGuid);
      this.lastBusinessEndAt = this.now();
    } catch (error) {
      this.lastError = `heartbeat failed: ${error instanceof Error ? error.message : String(error)}`;
    }
  }

  stopHeartbeat(): void {
    if (this.heartbeatTimer !== null) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  // -- Channel resolution ----------------------------------------------------

  private async resolveChannelOnce(session: IptvAppSession, slug: string, liveId: string): Promise<IptvDeviceEntry> {
    const live01 = await this.liveV101Flow(session, liveId);
    const appSecret = await this.liveV102Flow(session);
    const vdn = await this.vdnGetstreamFlow(session.identity, live01.liveUrl, appSecret);
    const playbackHeaders = this.defaultPlaybackHeaders();
    playbackHeaders.APPRANDOMSTR = vdn.appRandomStr;
    playbackHeaders.APPSIGN = vdn.appSign;
    return {
      slug,
      finalUrl: vdn.finalUrl,
      playbackHeaders,
      androidId: session.profile.androidId,
      finalHost: urlHost(vdn.finalUrl),
      refreshedAt: this.now(),
      expiresAt: this.now() + ENTRY_TTL_MS,
    };
  }

  private async liveV101Flow(session: IptvAppSession, liveId: string): Promise<{ liveUrl: string; rate: string; rateName: string }> {
    const body = {
      screenParam: session.screenParam,
      rate: '',
      systemType: 'ios',
      model: session.castModel,
      id: liveId,
      userId: DEFAULT_LIVE_USER_ID,
      clientSign: 'cctvVideo',
      deviceId: { serial: '', imei: '', android_id: '' },
    };
    const [status, value, label] = await this.withControl(() =>
      this.postJson(LIVE_V1_01_URL, freshHeaders(session.identity.headers, 'application/json', this.randomBytes, 'application/json'), body, 'live/v1/01'),
    );
    if (status < 200 || status >= 300) throw new IptvDeviceError(label);
    const data = value !== null && typeof value === 'object' ? (value as JsonObject).data : null;
    const dataObj = data !== null && typeof data === 'object' ? (data as JsonObject) : null;
    // Upstream prefers a non-empty videoList, then falls back to videos.
    const pickVideos = (v: unknown): unknown[] | null =>
      Array.isArray(v) && v.length > 0 ? v : null;
    const videos = pickVideos(dataObj?.videoList) ?? pickVideos(dataObj?.videos);
    if (!videos) throw new IptvDeviceError('live/v1/01 missing videos');
    let fallback: JsonObject | null = null;
    let selected: JsonObject | null = null;
    for (const item of videos) {
      if (item === null || typeof item !== 'object') continue;
      const record = item as JsonObject;
      if (typeof record.url !== 'string' || !record.url) continue;
      fallback ??= record;
      if (record.rate === '36p') {
        selected = record;
        break;
      }
    }
    const video = selected ?? fallback;
    if (!video) throw new IptvDeviceError('live/v1/01 no usable URL');
    const rawUrl = typeof video.url === 'string' ? video.url : '';
    const liveUrl = rawUrl.startsWith('http://') || rawUrl.startsWith('https://')
      ? rawUrl
      : aesGcmDecryptB64(rawUrl, session.sessionKey);
    return {
      liveUrl,
      rate: typeof video.rate === 'string' ? video.rate : '',
      rateName: typeof video.rateName === 'string' ? video.rateName : '',
    };
  }

  private async liveV102Flow(session: IptvAppSession): Promise<string> {
    const body = { guid: aesGcmEncryptB64('', session.sessionKey, this.randomBytes) };
    const [status, value, label] = await this.withControl(() =>
      this.postJson(LIVE_V1_02_URL, freshHeaders(session.identity.headers, 'application/json', this.randomBytes, 'application/json'), body, 'live/v1/02'),
    );
    if (status < 200 || status >= 300) throw new IptvDeviceError(label);
    let encrypted: string | null = null;
    const data = value !== null && typeof value === 'object' ? (value as JsonObject).data : null;
    if (data !== null && typeof data === 'object') {
      for (const key of ['appSecret', 'app_secret']) {
        const v = (data as JsonObject)[key];
        if (typeof v === 'string') {
          encrypted = v;
          break;
        }
      }
    } else if (typeof data === 'string') {
      encrypted = data;
    }
    if (encrypted === null) throw new IptvDeviceError('live/v1/02 missing appSecret');
    return aesGcmDecryptB64(encrypted, session.sessionKey);
  }

  private async vdnGetstreamFlow(identity: IptvIdentity, liveUrl: string, appSecret: string): Promise<{ finalUrl: string; appSign: string; appRandomStr: string }> {
    return this.withControl(async () => {
      const [appSign, randomStr] = computeVdnCode(appSecret, undefined, this.randomBytes);
      const headers = freshHeaders(identity.headers, 'application/x-www-form-urlencoded', this.randomBytes);
      headers.APPID = IPTV_DEVICE_AK;
      headers.APPSIGN = appSign;
      headers.APPRANDOMSTR = randomStr;
      const appcommon = buildVdnAppcommon(VDN_APP_VERSION);
      const response = await this.fetchImpl(VDN_GETSTREAM_URL, {
        method: 'POST',
        headers,
        body: formEncode([['appcommon', appcommon], ['url', liveUrl]]),
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
      const text = await response.text();
      if (!response.ok) throw new IptvDeviceError(`vdn http ${String(response.status)}: ${text.slice(0, 300)}`);
      let value: unknown;
      try {
        value = JSON.parse(text);
      } catch {
        value = null;
      }
      if (value !== null && typeof value === 'object') {
        const record = value as JsonObject;
        const succeed = record.succeed;
        const succeedStr = typeof succeed === 'string' ? succeed.replaceAll('"', '') : succeed === undefined || succeed === null ? '' : JSON.stringify(succeed).replaceAll('"', '');
        if (succeedStr === '1' && typeof record.url === 'string') {
          return { finalUrl: record.url, appSign, appRandomStr: randomStr };
        }
      }
      throw new IptvDeviceError(`vdn did not return final url: ${text.slice(0, 400)}`);
    });
  }

  // -- Device state persistence (upstream device-state-rs.json) ---------------

  private loadDeviceState(): IptvDeviceState {
    if (this.deviceState) return this.deviceState;
    const stored = this.storage?.getItem(DEVICE_STATE_KEY);
    if (stored) {
      const str = (v: unknown): string => (typeof v === 'string' ? v : '');
      const num = (v: unknown, fallback = 0): number => (typeof v === 'number' && Number.isFinite(v) ? v : fallback);
      try {
        const data = JSON.parse(stored) as Record<string, unknown>;
        const profile = data.profile as Record<string, unknown> | undefined;
        if (!profile || typeof profile !== 'object') throw new Error('no profile');
        const state: IptvDeviceState = {
          schemaVersion: num(data.schema_version, 1),
          profileSource: str(data.profile_source),
          profile: {
            androidId: str(profile.android_id),
            mac: str(profile.mac),
            hardware: str(profile.hardware),
            board: str(profile.board),
            brand: str(profile.brand),
            device: str(profile.device),
            manufacturer: str(profile.manufacturer),
            model: str(profile.model),
            product: str(profile.product),
            tags: str(profile.tags),
            buildType: str(profile.build_type),
            user: str(profile.user),
            resolution: str(profile.resolution),
            display: str(profile.display),
            versionId: str(profile.version_id),
            host: str(profile.host),
            fingerprint: str(profile.fingerprint),
            reportModel: str(profile.report_model),
          },
          screenParam: str(data.screen_param),
          castModel: str(data.cast_model),
          xUid: str(data.x_uid),
          cloudGuid: str(data.cloud_guid),
          registeredAt: num(data.registered_at),
          updatedAt: num(data.updated_at),
        };
        if (state.profile.androidId.length !== 16 || !state.profile.mac) throw new Error('bad device state');
        state.xUid = computeXUid(state.profile);
        if (!state.screenParam) state.screenParam = '7680-4320-280';
        if (!state.castModel) state.castModel = state.profile.model;
        this.deviceState = state;
        return state;
      } catch {
        // Fall through to a fresh identity.
      }
    }
    const template = IPTV_DEVICE_PROFILE_POOL[Math.floor((this.randomBytes(1)[0] ?? 0) / 256 * IPTV_DEVICE_PROFILE_POOL.length) % IPTV_DEVICE_PROFILE_POOL.length]
      ?? IPTV_DEVICE_PROFILE_POOL[0];
    if (!template) throw new IptvDeviceError('device profile pool is empty');
    const profile = deviceProfileFromTemplate(template, randomHexString(16, this.randomBytes), randomMacAddress(this.randomBytes));
    const state: IptvDeviceState = {
      schemaVersion: 1,
      profileSource: template.source,
      profile,
      screenParam: template.screen_param,
      castModel: template.cast_model,
      xUid: computeXUid(profile),
      cloudGuid: '',
      registeredAt: 0,
      updatedAt: this.now(),
    };
    this.deviceState = state;
    this.saveDeviceState(state);
    return state;
  }

  private saveDeviceState(state: IptvDeviceState): void {
    try {
      this.storage?.setItem(DEVICE_STATE_KEY, JSON.stringify({
        schema_version: state.schemaVersion,
        profile_source: state.profileSource,
        profile: {
          android_id: state.profile.androidId,
          mac: state.profile.mac,
          hardware: state.profile.hardware,
          board: state.profile.board,
          brand: state.profile.brand,
          device: state.profile.device,
          manufacturer: state.profile.manufacturer,
          model: state.profile.model,
          product: state.profile.product,
          tags: state.profile.tags,
          build_type: state.profile.buildType,
          user: state.profile.user,
          resolution: state.profile.resolution,
          display: state.profile.display,
          version_id: state.profile.versionId,
          host: state.profile.host,
          fingerprint: state.profile.fingerprint,
          report_model: state.profile.reportModel,
        },
        screen_param: state.screenParam,
        cast_model: state.castModel,
        x_uid: state.xUid,
        cloud_guid: state.cloudGuid,
        registered_at: state.registeredAt,
        updated_at: state.updatedAt,
      }));
    } catch {
      // Storage is best-effort; a fresh identity still works in memory.
    }
  }
}

function compactJsonToText(value: unknown): string {
  return new TextDecoder().decode(compactJsonBytes(value));
}

let sharedEngine: IptvDeviceEngine | null = null;

export function getIptvDeviceEngine(): IptvDeviceEngine {
  sharedEngine ??= new IptvDeviceEngine();
  return sharedEngine;
}
