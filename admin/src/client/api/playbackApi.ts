import { ClientApiError, clientBlob, clientRequest } from './clientApi';

export interface PlaybackStream {
  Type?: 'Audio' | 'Video' | 'Subtitle';
  Codec?: string;
  Language?: string;
  Width?: number;
  Height?: number;
  Channels?: number;
  DeliveryUrl?: string;
  IsExternal?: boolean;
  IsDefault?: boolean;
  IsForced?: boolean;
  Index?: number;
}

export interface PlaybackSource {
  Id: string;
  Name?: string;
  Container?: string;
  Bitrate?: number;
  RunTimeTicks?: number;
  IsDefault?: boolean;
  IsLive?: boolean;
  SupportsDirectPlay?: boolean;
  DirectStreamUrl?: string;
  MediaStreams?: PlaybackStream[];
}

export interface PlaybackInfo {
  MediaSources?: PlaybackSource[];
  PlaySessionId?: string;
}

export interface PlaybackTicket {
  Id: string;
  Ticket: string;
  ExpiresAt: string;
  StreamUrl: string;
}

export interface PlaybackState {
  itemId: string;
  mediaSourceId: string;
  playSessionId: string;
  positionTicks: number;
}

export interface StopPlaybackOptions {
  keepalive?: boolean;
}

const DEFAULT_PREPARING_RETRY_SECONDS = 2;
const MIN_PREPARING_RETRY_SECONDS = 1;
const MAX_PREPARING_RETRY_SECONDS = 10;
/** Upper bound for waiting on a title the server reports as still preparing. */
export const PLAYBACK_PREPARING_BUDGET_MS = 45_000;

/**
 * Requests playback info. The server answers 503 with `Retry-After` while it
 * is still probing media; those responses are retried within a bounded
 * budget. Other failures, including 503 without `Retry-After`, are final.
 */
export async function getPlaybackInfo(itemId: string, signal?: AbortSignal): Promise<PlaybackInfo> {
  const startedAt = Date.now();
  for (;;) {
    try {
      return await clientRequest<PlaybackInfo>(`/Items/${encodeURIComponent(itemId)}/PlaybackInfo`, {
        method: 'POST',
        body: JSON.stringify({}),
        ...(signal ? { signal } : {}),
      });
    } catch (error) {
      const delayMs = preparingRetryDelayMs(error);
      if (delayMs === undefined || Date.now() + delayMs - startedAt > PLAYBACK_PREPARING_BUDGET_MS) throw error;
      await waitForRetry(delayMs, signal);
    }
  }
}

export async function issuePlaybackTicket(
  itemId: string,
  mediaSourceId: string,
  playSessionId: string,
): Promise<PlaybackTicket> {
  return clientRequest<PlaybackTicket>(`/Items/${encodeURIComponent(itemId)}/PlaybackTicket`, {
    method: 'POST',
    body: JSON.stringify({ MediaSourceId: mediaSourceId, PlaySessionId: playSessionId }),
  });
}

export async function revokePlaybackTicket(id: string): Promise<void> {
  await clientRequest(`/PlaybackTickets/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

export async function startPlayback(state: PlaybackState): Promise<void> {
  await sendPlaybackState('/Sessions/Playing', state);
}

export async function reportPlaybackProgress(state: PlaybackState): Promise<void> {
  await sendPlaybackState('/Sessions/Playing/Progress', state);
}

export async function stopPlayback(
  state: PlaybackState,
  options: StopPlaybackOptions = {},
): Promise<void> {
  await sendPlaybackState('/Sessions/Playing/Stopped', state, options);
}

export async function getSubtitleBlob(path: string, signal?: AbortSignal): Promise<Blob> {
  return clientBlob(path, signal);
}

async function sendPlaybackState(
  path: string,
  state: PlaybackState,
  options: StopPlaybackOptions = {},
): Promise<void> {
  await clientRequest(path, {
    method: 'POST',
    body: JSON.stringify({
      ItemId: state.itemId,
      MediaSourceId: state.mediaSourceId,
      PlaySessionId: state.playSessionId,
      PositionTicks: state.positionTicks,
    }),
    ...(options.keepalive ? { keepalive: true } : {}),
  });
}

function preparingRetryDelayMs(error: unknown): number | undefined {
  if (!(error instanceof ClientApiError) || error.status !== 503 || error.retryAfter === undefined) return undefined;
  const seconds = Number(error.retryAfter.trim());
  const valid = error.retryAfter.trim() !== '' && Number.isFinite(seconds) ? seconds : DEFAULT_PREPARING_RETRY_SECONDS;
  return Math.min(Math.max(valid, MIN_PREPARING_RETRY_SECONDS), MAX_PREPARING_RETRY_SECONDS) * 1000;
}

function waitForRetry(delayMs: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(new DOMException('The playback request was aborted.', 'AbortError'));
      return;
    }
    const onAbort = () => {
      clearTimeout(timer);
      reject(new DOMException('The playback request was aborted.', 'AbortError'));
    };
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', onAbort);
      resolve();
    }, delayMs);
    signal?.addEventListener('abort', onAbort, { once: true });
  });
}
