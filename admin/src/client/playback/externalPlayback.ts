import { resolveApiUrl } from '../api/apiBase';
import { getPlaybackInfo, issuePlaybackTicket } from '../api/playbackApi';
import { selectNativeSource } from './sourceSelection';

export type ExternalPlayerId = 'potplayer' | 'vlc' | 'iina' | 'infuse' | 'nplayer';
export type ExternalPlayerPlatform = 'windows' | 'macos' | 'ios' | 'android' | 'linux' | 'unknown';

export interface ExternalPlayerOption {
  id: ExternalPlayerId;
  label: string;
}

export interface ExternalPlaybackLink {
  expiresAt: string;
  streamUrl: string;
}

const PLAYER_LABELS: Record<ExternalPlayerId, string> = {
  potplayer: 'PotPlayer',
  vlc: 'VLC',
  iina: 'IINA',
  infuse: 'Infuse',
  nplayer: 'nPlayer',
};

const PLATFORM_PLAYERS: Record<ExternalPlayerPlatform, ExternalPlayerId[]> = {
  windows: ['potplayer', 'vlc'],
  macos: ['iina', 'vlc', 'infuse', 'nplayer'],
  ios: ['vlc', 'infuse', 'nplayer'],
  android: ['vlc', 'nplayer'],
  linux: ['vlc'],
  unknown: ['vlc'],
};

export class ExternalPlaybackUnavailableError extends Error {
  constructor() {
    super('external playback is unavailable');
    this.name = 'ExternalPlaybackUnavailableError';
  }
}

export async function createExternalPlaybackLink(itemId: string): Promise<ExternalPlaybackLink> {
  const playback = await getPlaybackInfo(itemId);
  const source = selectNativeSource(playback.MediaSources ?? []);
  if (!source || !playback.PlaySessionId) throw new ExternalPlaybackUnavailableError();
  const ticket = await issuePlaybackTicket(itemId, source.Id, playback.PlaySessionId);
  return {
    expiresAt: ticket.ExpiresAt,
    streamUrl: resolveApiUrl(ticket.StreamUrl),
  };
}

export function detectExternalPlayerPlatform(
  userAgent = typeof navigator === 'undefined' ? '' : navigator.userAgent,
  platform = typeof navigator === 'undefined' ? '' : navigator.platform,
): ExternalPlayerPlatform {
  const normalizedAgent = userAgent.toLowerCase();
  const normalizedPlatform = platform.toLowerCase();
  if (/iphone|ipad|ipod/.test(normalizedAgent)
    || (normalizedPlatform === 'macintel' && normalizedAgent.includes('mobile'))) return 'ios';
  if (normalizedAgent.includes('android')) return 'android';
  if (normalizedPlatform.includes('win') || normalizedAgent.includes('windows')) return 'windows';
  if (normalizedPlatform.includes('mac') || normalizedAgent.includes('mac os')) return 'macos';
  if (normalizedPlatform.includes('linux') || normalizedAgent.includes('linux')) return 'linux';
  return 'unknown';
}

export function externalPlayersForPlatform(platform: ExternalPlayerPlatform): ExternalPlayerOption[] {
  return PLATFORM_PLAYERS[platform].map((id) => ({ id, label: PLAYER_LABELS[id] }));
}

export function buildExternalPlayerUrl(
  player: ExternalPlayerId,
  streamUrl: string,
  title: string,
  platform: ExternalPlayerPlatform,
): string {
  const parsed = new URL(streamUrl);
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') throw new Error('invalid playback url');
  switch (player) {
    case 'potplayer':
      return `potplayer://${encodeURI(streamUrl)}`;
    case 'vlc':
      return platform === 'ios'
        ? `vlc-x-callback://x-callback-url/stream?url=${encodeURIComponent(streamUrl)}`
        : `vlc://${encodeURI(streamUrl)}`;
    case 'iina':
      return `iina://weblink?url=${encodeURIComponent(streamUrl)}&new_window=1`;
    case 'infuse':
      return `infuse://x-callback-url/play?url=${encodeURIComponent(streamUrl)}&filename=${encodeURIComponent(title)}`;
    case 'nplayer':
      return `nplayer-${streamUrl}`;
  }
}

export function openExternalPlayer(url: string): void {
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.rel = 'noopener noreferrer';
  anchor.click();
}
