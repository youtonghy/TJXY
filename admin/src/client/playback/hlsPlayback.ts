export function isHlsSource(source: string | undefined): boolean {
  if (!source) return false;
  try {
    return new URL(source, window.location.href).pathname.toLowerCase().endsWith('.m3u8');
  } catch {
    return false;
  }
}

export async function attachHlsSource(
  video: HTMLVideoElement,
  source: string,
  onFatalError: () => void,
): Promise<() => void> {
  // Prefer hls.js over native playback: Chromium reports "maybe" for
  // application/vnd.apple.mpegurl but its built-in demuxer fails on many live
  // playlists, so trusting canPlayType there breaks playback entirely. Engines
  // without MediaSource (older iOS WebViews) fall through to native playback.
  const { default: Hls } = await import('hls.js');
  if (Hls.isSupported()) {
    const hls = new Hls({ enableWorker: true });
    let mediaRecoveries = 0;
    let networkRecoveries = 0;
    hls.on(Hls.Events.ERROR, (_event, data) => {
      if (!data.fatal) return;
      if (data.type === Hls.ErrorTypes.MEDIA_ERROR && mediaRecoveries < 1) {
        mediaRecoveries += 1;
        hls.recoverMediaError();
        return;
      }
      if (data.type === Hls.ErrorTypes.NETWORK_ERROR && networkRecoveries < 1) {
        networkRecoveries += 1;
        hls.startLoad();
        return;
      }
      onFatalError();
    });
    hls.loadSource(source);
    hls.attachMedia(video);
    return () => {
      hls.destroy();
      video.removeAttribute('src');
      video.load();
    };
  }
  if (video.canPlayType('application/vnd.apple.mpegurl')) {
    video.src = source;
    return () => {
      video.removeAttribute('src');
      video.load();
    };
  }
  onFatalError();
  return () => undefined;
}
