import { clientBlob, clientRequest } from './clientApi';

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

export async function getPlaybackInfo(itemId: string): Promise<PlaybackInfo> {
  return clientRequest<PlaybackInfo>(`/Items/${itemId}/PlaybackInfo`, {
    method: 'POST',
    body: JSON.stringify({}),
  });
}

export async function issuePlaybackTicket(
  itemId: string,
  mediaSourceId: string,
  playSessionId: string,
): Promise<PlaybackTicket> {
  return clientRequest<PlaybackTicket>(`/Items/${itemId}/PlaybackTicket`, {
    method: 'POST',
    body: JSON.stringify({ MediaSourceId: mediaSourceId, PlaySessionId: playSessionId }),
  });
}

export async function revokePlaybackTicket(id: string): Promise<void> {
  await clientRequest(`/PlaybackTickets/${id}`, { method: 'DELETE' });
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
