import { invoke } from '@tauri-apps/api/core';

export async function playExternalStream(url: string, title: string): Promise<void> {
  await invoke('play_stream', { url, title });
}

export async function stopExternalPlayer(): Promise<void> {
  await invoke('stop_player');
}
