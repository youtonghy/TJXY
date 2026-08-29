import { invoke } from '@tauri-apps/api/core';

export type DesktopPlayerPhase = 'loading' | 'playing' | 'paused' | 'buffering' | 'ended';

export interface DesktopPlayerOpenRequest {
  loadId: string;
  url: string;
  serverOrigin: string;
  startPositionTicks: number;
  autoplay: boolean;
}

export interface DesktopPlayerViewport {
  x: number;
  y: number;
  width: number;
  height: number;
  visible: boolean;
}

export interface DesktopPlayerSnapshot {
  loadId: string;
  phase: DesktopPlayerPhase;
  positionTicks: number;
  durationTicks?: number;
  seekable: boolean;
  volume: number;
  muted: boolean;
  playbackRate: number;
}

export type DesktopPlayerEvent =
  | { kind: 'state'; snapshot: DesktopPlayerSnapshot }
  | { kind: 'ended'; loadId: string; positionTicks: number }
  | { kind: 'error'; loadId: string; code: string; message: string };

export type DesktopPlayerCommand =
  | { kind: 'play' }
  | { kind: 'pause' }
  | { kind: 'seek'; positionTicks: number }
  | { kind: 'setVolume'; volume: number }
  | { kind: 'setMuted'; muted: boolean }
  | { kind: 'setPlaybackRate'; playbackRate: number };

export function openDesktopPlayer(
  request: DesktopPlayerOpenRequest,
  viewport: DesktopPlayerViewport,
): Promise<DesktopPlayerSnapshot> {
  return invoke('desktop_player_open', { request, viewport });
}

export function commandDesktopPlayer(loadId: string, command: DesktopPlayerCommand): Promise<void> {
  return invoke('desktop_player_command', { loadId, command });
}

export function updateDesktopPlayerViewport(
  loadId: string,
  viewport: DesktopPlayerViewport,
): Promise<void> {
  return invoke('desktop_player_set_viewport', { loadId, viewport });
}

export function closeDesktopPlayer(loadId: string): Promise<void> {
  return invoke('desktop_player_close', { loadId });
}
