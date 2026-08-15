import type { PlaybackSource } from '../api/playbackApi';

const BROWSER_CONTAINERS = new Set(['mp4', 'm4v', 'webm', 'mp3', 'm4a', 'ogg']);

function isDirectPlaySource(source: PlaybackSource): boolean {
  return source.SupportsDirectPlay !== false && Boolean(source.DirectStreamUrl);
}

export function browserSources(sources: PlaybackSource[]): PlaybackSource[] {
  return sources.filter((source) => (
    isDirectPlaySource(source)
    && BROWSER_CONTAINERS.has((source.Container ?? '').toLowerCase())
  ));
}

export function nativeSources(sources: PlaybackSource[]): PlaybackSource[] {
  return sources.filter(isDirectPlaySource);
}

export function selectBrowserSource(sources: PlaybackSource[]): PlaybackSource | null {
  return browserSources(sources)[0] ?? null;
}

export function selectNativeSource(sources: PlaybackSource[]): PlaybackSource | null {
  return nativeSources(sources)[0] ?? null;
}

export function sourceLabel(source: PlaybackSource): string {
  const video = source.MediaStreams?.find((stream) => stream.Type === 'Video');
  const parts = [
    source.Name,
    video?.Width && video.Height ? `${String(video.Width)}×${String(video.Height)}` : undefined,
    video?.Codec?.toUpperCase(),
    source.Bitrate ? `${(source.Bitrate / 1_000_000).toFixed(1)} Mbps` : undefined,
  ].filter((part): part is string => Boolean(part));
  if (parts.length) return parts.join(' · ');
  return source.Container?.toUpperCase() ?? 'Source';
}
