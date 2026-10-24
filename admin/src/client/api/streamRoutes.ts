import { useSyncExternalStore } from 'react';
import { API_BASE_CHANGED_EVENT, getApiBaseUrl, isDesktopShell, normalizeOrigin, resolveApiUrl } from './apiBase';
import { ClientApiError, clientBlob, clientFetch, clientRequest } from './clientApi';
import { getClientToken } from '../auth/clientSession';

/**
 * Stream routes are alternate origins of this server. API calls and the
 * sign-in session stay on the API base; media streams and images are loaded
 * from the fastest route, or the one the user picked.
 */
export interface StreamRoute {
  origin: string;
  /** Administrator-provided label; absent for the current address. */
  name?: string;
  current: boolean;
  status: 'pending' | 'ok' | 'failed';
  latencyMs?: number;
}

export type StreamRoutePreference = { mode: 'auto' } | { mode: 'manual'; origin: string };

export interface StreamRoutesSnapshot {
  routes: StreamRoute[];
  preference: StreamRoutePreference;
  probing: boolean;
  activeOrigin: string;
}

interface StreamAddressesResponse {
  Addresses?: { Name: string; Url: string }[];
}

interface ProbeCache {
  apiBase: string;
  measuredAt: number;
  routes: StreamRoute[];
}

export const STREAM_ROUTE_PREFERENCE_KEY = 'tjxy.streamRoute.preference';
export const STREAM_ROUTE_PROBE_KEY = 'tjxy.streamRoute.probe';
export const PROBE_TTL_MS = 10 * 60 * 1000;
const PING_ROUNDS = 3;
const PING_TIMEOUT_MS = 2000;
/** Longest a starting player waits for a probe before using the best known route. */
export const PLAYBACK_ROUTE_WAIT_MS = 1500;
const PING_BODY = 'TJXY Server';

let routes: StreamRoute[] = [];
let routesApiBase = '';
let measuredAt = 0;
let preference: StreamRoutePreference = readPreference();
let probing = false;
let snapshot: StreamRoutesSnapshot = buildSnapshot();
let inFlight: Promise<string> | undefined;
const listeners = new Set<() => void>();

// The desktop shell can switch servers; routes of the old server no longer apply.
if (typeof window !== 'undefined') {
  window.addEventListener(API_BASE_CHANGED_EVENT, () => {
    routes = [];
    routesApiBase = '';
    measuredAt = 0;
    emit();
  });
}

function emit(): void {
  snapshot = buildSnapshot();
  listeners.forEach((listener) => { listener(); });
}

function buildSnapshot(): StreamRoutesSnapshot {
  return { routes, preference, probing, activeOrigin: selectStreamOrigin(routes, preference, getApiBaseUrl()) };
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

export function useStreamRoutes(): StreamRoutesSnapshot {
  return useSyncExternalStore(subscribe, () => snapshot, () => snapshot);
}

/**
 * Picks the origin to load media from. A manual choice wins while the route
 * still exists; otherwise the lowest measured latency wins, and the current
 * address is used until something faster has been measured.
 */
export function selectStreamOrigin(
  candidates: readonly StreamRoute[],
  choice: StreamRoutePreference,
  apiBase: string,
): string {
  if (choice.mode === 'manual' && candidates.some((route) => route.origin === choice.origin)) return choice.origin;
  let best: StreamRoute | undefined;
  for (const route of candidates) {
    if (route.status !== 'ok' || route.latencyMs === undefined) continue;
    if (best?.latencyMs === undefined || route.latencyMs < best.latencyMs
      || (route.latencyMs === best.latencyMs && route.current)) best = route;
  }
  return best?.origin ?? apiBase;
}

export function activeStreamOrigin(): string {
  return snapshot.activeOrigin;
}

/** Resolves a server-relative stream path on a route; absolute URLs are external and kept. */
export function resolveStreamUrl(path: string, origin = activeStreamOrigin()): string {
  if (/^https?:\/\//i.test(path)) return path;
  return resolveApiUrl(path, origin || getApiBaseUrl());
}

export function setStreamRoutePreference(next: StreamRoutePreference): void {
  preference = next;
  try {
    window.localStorage.setItem(STREAM_ROUTE_PREFERENCE_KEY, JSON.stringify(next));
  } catch {
    // Storage may be unavailable (private mode); the choice still applies to this page.
  }
  emit();
}

/** Excludes a route from automatic selection until the next probe. */
export function markStreamOriginFailed(origin: string): void {
  const target = routes.find((route) => route.origin === origin && !route.current && route.status !== 'failed');
  if (!target) return;
  routes = routes.map((route) => (route === target ? { ...route, status: 'failed', latencyMs: undefined } : route));
  writeProbeCache();
  emit();
}

/**
 * Returns the active origin. The address list is fetched every time so that
 * administrator changes show up at once; latencies are re-measured only when
 * the set of routes changed or the last measurement is stale.
 */
export async function ensureStreamRoutes(): Promise<string> {
  if (inFlight) return inFlight;
  const apiBase = getApiBaseUrl();
  if (!routes.length || routesApiBase !== apiBase) {
    const cached = readProbeCache();
    if (cached) {
      routes = cached.routes;
      routesApiBase = cached.apiBase;
      measuredAt = cached.measuredAt;
      emit();
    }
  }
  let configured: { Name: string; Url: string }[];
  try {
    configured = await fetchConfiguredAddresses();
  } catch {
    // Unreachable API: keep what is known rather than discarding measurements.
    return routes.length && routesApiBase === apiBase ? activeStreamOrigin() : refreshStreamRoutes();
  }
  const candidates = streamRouteCandidates(configured, apiBase);
  if (routesApiBase === apiBase && isFresh(measuredAt) && sameOrigins(routes, candidates)) {
    const names = new Map(candidates.map((candidate) => [candidate.origin, candidate.name]));
    routes = routes.map((route) => ({ ...route, name: names.get(route.origin) }));
    writeProbeCache();
    emit();
    return activeStreamOrigin();
  }
  return probe(() => Promise.resolve(candidates));
}

function sameOrigins(current: readonly StreamRoute[], next: readonly StreamRoute[]): boolean {
  return current.length === next.length && current.every((route, index) => route.origin === next[index]?.origin);
}

/** Lists the configured addresses; older servers without the endpoint have none. */
async function fetchConfiguredAddresses(): Promise<{ Name: string; Url: string }[]> {
  try {
    return (await clientRequest<StreamAddressesResponse>('/System/StreamAddresses')).Addresses ?? [];
  } catch (error) {
    if (error instanceof ClientApiError && error.status !== 0) return [];
    throw error;
  }
}

/**
 * Resolves the origin for a stream that is about to start. A probe still in
 * flight may delay playback only briefly; afterwards the best route known so
 * far, at worst the current address, is used.
 */
export function streamOriginForPlayback(maxWaitMs = PLAYBACK_ROUTE_WAIT_MS): Promise<string> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<string>((resolve) => {
    timer = setTimeout(() => { resolve(activeStreamOrigin()); }, maxWaitMs);
  });
  const probed = ensureStreamRoutes().catch(() => activeStreamOrigin());
  return Promise.race([probed, deadline]).finally(() => { clearTimeout(timer); });
}

function isFresh(timestamp: number): boolean {
  return Date.now() - timestamp <= PROBE_TTL_MS;
}

/** Reloads the configured addresses and measures every route again. */
export function refreshStreamRoutes(): Promise<string> {
  return probe(async () => streamRouteCandidates(
    await fetchConfiguredAddresses().catch(() => []),
    getApiBaseUrl(),
  ));
}

function probe(loadCandidates: () => Promise<StreamRoute[]>): Promise<string> {
  inFlight ??= probeAll(loadCandidates).finally(() => { inFlight = undefined; });
  return inFlight;
}

async function probeAll(loadCandidates: () => Promise<StreamRoute[]>): Promise<string> {
  const apiBase = getApiBaseUrl();
  probing = true;
  emit();
  try {
    const candidates = await loadCandidates();
    routes = candidates;
    routesApiBase = apiBase;
    emit();
    const measured = await Promise.all(candidates.map(async (route) => {
      const latencyMs = await measureLatency(route.origin);
      return latencyMs === undefined
        ? { ...route, status: 'failed' as const }
        : { ...route, status: 'ok' as const, latencyMs };
    }));
    if (getApiBaseUrl() === apiBase) {
      routes = measured;
      measuredAt = Date.now();
      writeProbeCache();
    }
  } finally {
    probing = false;
    emit();
  }
  return activeStreamOrigin();
}

export function streamRouteCandidates(
  configured: readonly { Name: string; Url: string }[],
  apiBase: string,
): StreamRoute[] {
  const blockInsecure = !isDesktopShell() && window.location.protocol === 'https:';
  const candidates: StreamRoute[] = apiBase ? [{ origin: apiBase, current: true, status: 'pending' }] : [];
  for (const address of configured) {
    let origin: string;
    try {
      origin = normalizeOrigin(address.Url);
    } catch {
      continue;
    }
    const existing = candidates.find((route) => route.origin === origin);
    if (existing) {
      existing.name ??= address.Name;
      continue;
    }
    // Browsers block http media and fetches on an https page.
    if (blockInsecure && origin.startsWith('http:')) continue;
    candidates.push({ origin, name: address.Name, current: false, status: 'pending' });
  }
  return candidates;
}

/**
 * Times `/System/Ping` on one origin. The first round warms up DNS, TCP and
 * TLS and is not counted; the best later round is the latency.
 */
export async function measureLatency(origin: string): Promise<number | undefined> {
  let best: number | undefined;
  for (let round = 0; round < PING_ROUNDS; round += 1) {
    const elapsed = await pingOnce(origin);
    if (elapsed === undefined) return undefined;
    if (round > 0 && (best === undefined || elapsed < best)) best = elapsed;
  }
  return best === undefined ? undefined : Math.max(1, Math.round(best));
}

async function pingOnce(origin: string): Promise<number | undefined> {
  const controller = new AbortController();
  const timer = setTimeout(() => { controller.abort(); }, PING_TIMEOUT_MS);
  try {
    const startedAt = performance.now();
    const response = await fetch(`${origin}/System/Ping?t=${Date.now().toString(36)}`, {
      cache: 'no-store',
      credentials: 'omit',
      signal: controller.signal,
    });
    const elapsed = performance.now() - startedAt;
    if (!response.ok || (await response.text()).trim() !== PING_BODY) return undefined;
    return elapsed;
  } catch {
    return undefined;
  } finally {
    clearTimeout(timer);
  }
}

/**
 * Loads an authenticated media blob (posters, artwork) from the active route,
 * falling back to the current address when that route is unreachable.
 */
export async function clientMediaBlob(path: string, signal?: AbortSignal): Promise<Blob> {
  const apiBase = getApiBaseUrl();
  const origin = activeStreamOrigin();
  // Only bearer tokens travel cross-origin; cookie sessions stay on the API base.
  if (origin && origin !== apiBase && getClientToken()) {
    try {
      const response = await clientFetch(path, { signal }, origin);
      if (response.ok) return await response.blob();
      if (response.status < 500) throw new ClientApiError(response.status, response.status === 404 ? 'not-found' : 'unexpected');
      markStreamOriginFailed(origin);
    } catch (error) {
      if (error instanceof DOMException && error.name === 'AbortError') throw error;
      if (error instanceof ClientApiError && error.status !== 0) throw error;
      markStreamOriginFailed(origin);
    }
  }
  return clientBlob(path, signal);
}

function readPreference(): StreamRoutePreference {
  if (typeof window === 'undefined') return { mode: 'auto' };
  try {
    const parsed = JSON.parse(window.localStorage.getItem(STREAM_ROUTE_PREFERENCE_KEY) ?? 'null') as Partial<{ mode: string; origin: unknown }> | null;
    if (parsed?.mode === 'manual' && typeof parsed.origin === 'string') return { mode: 'manual', origin: parsed.origin };
  } catch {
    // A corrupt preference falls back to automatic selection.
  }
  return { mode: 'auto' };
}

function readProbeCache(): ProbeCache | undefined {
  try {
    const parsed = JSON.parse(window.sessionStorage.getItem(STREAM_ROUTE_PROBE_KEY) ?? 'null') as ProbeCache | null;
    if (parsed?.apiBase !== getApiBaseUrl() || !Array.isArray(parsed.routes)) return undefined;
    if (typeof parsed.measuredAt !== 'number' || !isFresh(parsed.measuredAt)) return undefined;
    return parsed;
  } catch {
    return undefined;
  }
}

function writeProbeCache(): void {
  try {
    const cache: ProbeCache = { apiBase: routesApiBase, measuredAt, routes };
    window.sessionStorage.setItem(STREAM_ROUTE_PROBE_KEY, JSON.stringify(cache));
  } catch {
    // Without storage the next page load simply measures again.
  }
}

/** Test hook: drops in-memory state so each test starts from storage. */
export function resetStreamRoutesForTests(): void {
  routes = [];
  routesApiBase = '';
  measuredAt = 0;
  preference = readPreference();
  probing = false;
  inFlight = undefined;
  emit();
}
