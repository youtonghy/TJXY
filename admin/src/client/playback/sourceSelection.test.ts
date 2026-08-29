import { selectBrowserSource, selectNativeSource, sourceLabel } from './sourceSelection';

it('keeps MKV sources for native desktop playback', () => {
  const mkv = { Id: 'mkv', Container: 'mkv', SupportsDirectPlay: true, DirectStreamUrl: '/Videos/item/stream' };
  expect(selectNativeSource([mkv])?.Id).toBe('mkv');
  expect(selectBrowserSource([mkv])).toBeNull();
});

it('keeps native server priority when both MKV and MP4 exist', () => {
  const mkv = { Id: 'mkv', Container: 'mkv', SupportsDirectPlay: true, DirectStreamUrl: '/Videos/item/stream' };
  const mp4 = { Id: 'mp4', Container: 'mp4', SupportsDirectPlay: true, DirectStreamUrl: '/Videos/item/stream' };
  expect(selectNativeSource([mkv, mp4])?.Id).toBe('mkv');
});

it('keeps server source priority and rejects an MKV-only source list', () => {
  const mkv = { Id: 'mkv', Container: 'mkv', SupportsDirectPlay: true, DirectStreamUrl: '/Videos/item/stream' };
  const mp4 = { Id: 'mp4', Container: 'mp4', SupportsDirectPlay: true, DirectStreamUrl: '/Videos/item/stream' };
  const webm = { Id: 'webm', Container: 'webm', SupportsDirectPlay: true, DirectStreamUrl: '/Videos/item/stream' };
  expect(selectBrowserSource([webm, mp4])?.Id).toBe('webm');
  expect(selectBrowserSource([mkv])).toBeNull();
});

it('formats edition, video dimensions, codec, and bitrate without exposing fixture language', () => {
  expect(sourceLabel({
    Id: 'source-1',
    Name: '1080p',
    Container: 'mp4',
    Bitrate: 2_000_000,
    MediaStreams: [{ Type: 'Video', Codec: 'h264', Width: 1920, Height: 1080 }],
  })).toBe('1080p · 1920×1080 · H264 · 2.0 Mbps');
});
