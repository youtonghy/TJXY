import type { PlaybackSource } from '../api/playbackApi';
export function selectBrowserSource(sources: PlaybackSource[]): PlaybackSource | null { return sources.filter((source) => source.SupportsDirectPlay !== false && !!source.DirectStreamUrl && ['mp4', 'm4v', 'webm', 'mp3', 'm4a', 'ogg'].includes((source.Container ?? '').toLowerCase())).sort((a, b) => rank(a) - rank(b))[0] ?? null; }
function rank(source: PlaybackSource): number { const container = (source.Container ?? '').toLowerCase(); return container === 'mp4' ? 0 : container === 'webm' ? 1 : container === 'm4a' ? 2 : 3; }
