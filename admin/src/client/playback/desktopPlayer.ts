import { invoke } from '@tauri-apps/api/core';

export async function startDesktopStream(
  url: string,
  serverOrigin: string,
  startPositionSeconds: number,
): Promise<string> {
  return invoke('start_stream_proxy', { url, serverOrigin, startPositionSeconds });
}

export async function stopDesktopStream(): Promise<void> {
  await invoke('stop_stream_proxy');
}
