import { getPlaybackInfo, type PlaybackInfo } from '../api/playbackApi';
import { getClientToken } from '../auth/clientSession';

/** How long a prefetched PlaybackInfo response may wait for a play action. */
export const PLAYBACK_INFO_TTL_MS = 60_000;

interface CacheEntry {
  promise: Promise<PlaybackInfo>;
  controller: AbortController;
  token: string | null;
  /** `Infinity` while the request is in flight. */
  expiresAt: number;
  settled: boolean;
  /** Set by user intent (hover/focus/press); the request then outlives page-scoped prefetches. */
  pinned: boolean;
  /** Page-scoped prefetches that may still cancel the request. */
  holders: number;
}

// Only PlaybackInfo is prefetched: PlaybackTicket creates a playback session and
// must never be issued ahead of a play action. Each response carries a server
// PlaySessionId, and a stopped play session rejects further progress, so a
// response is handed to exactly one play action and then leaves the cache.
const entries = new Map<string, CacheEntry>();

/**
 * Starts loading PlaybackInfo ahead of an expected play action. Without a
 * signal the request completes even if the page goes away; with a signal it is
 * cancelled once every such prefetch aborted and no user intent pinned it.
 */
export function prefetchPlaybackInfo(itemId: string, signal?: AbortSignal): void {
  if (signal?.aborted) return;
  const entry = freshEntry(itemId) ?? startRequest(itemId);
  if (!signal) {
    entry.pinned = true;
    return;
  }
  entry.holders += 1;
  signal.addEventListener('abort', () => {
    entry.holders -= 1;
    if (!entry.settled && !entry.pinned && entry.holders === 0) {
      evict(itemId, entry);
      entry.controller.abort();
    }
  }, { once: true });
}

/**
 * Returns PlaybackInfo for one play action, reusing a fresh prefetch when one
 * exists. The response is removed from the cache so the next play action gets
 * its own play session. Aborting `signal` cancels the request it now owns.
 */
export function takePlaybackInfo(itemId: string, signal?: AbortSignal): Promise<PlaybackInfo> {
  if (signal?.aborted) return Promise.reject(abortError());
  const entry = freshEntry(itemId);
  if (!entry) return getPlaybackInfo(itemId, signal);
  evict(itemId, entry);
  entry.pinned = true;
  signal?.addEventListener('abort', () => {
    entry.controller.abort();
  }, { once: true });
  return entry.promise;
}

/** Drops all cached PlaybackInfo responses. */
export function clearPlaybackInfoCache(): void {
  entries.clear();
}

function freshEntry(itemId: string): CacheEntry | undefined {
  const entry = entries.get(itemId);
  if (!entry) return undefined;
  if (entry.expiresAt > Date.now() && entry.token === getClientToken()) return entry;
  evict(itemId, entry);
  return undefined;
}

function startRequest(itemId: string): CacheEntry {
  pruneExpired();
  const controller = new AbortController();
  const entry: CacheEntry = {
    promise: getPlaybackInfo(itemId, controller.signal),
    controller,
    token: getClientToken(),
    expiresAt: Number.POSITIVE_INFINITY,
    settled: false,
    pinned: false,
    holders: 0,
  };
  entries.set(itemId, entry);
  entry.promise.then(
    () => {
      entry.settled = true;
      entry.expiresAt = Date.now() + PLAYBACK_INFO_TTL_MS;
    },
    () => {
      entry.settled = true;
      evict(itemId, entry);
    },
  );
  return entry;
}

function pruneExpired(): void {
  const now = Date.now();
  for (const [itemId, entry] of entries) {
    if (entry.expiresAt <= now) entries.delete(itemId);
  }
}

function evict(itemId: string, entry: CacheEntry): void {
  if (entries.get(itemId) === entry) entries.delete(itemId);
}

function abortError(): DOMException {
  return new DOMException('The playback request was aborted.', 'AbortError');
}
