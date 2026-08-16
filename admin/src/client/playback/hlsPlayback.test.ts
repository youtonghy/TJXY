import { isHlsSource } from './hlsPlayback';

it('recognizes HLS playlists with query parameters', () => {
  expect(isHlsSource('http://127.0.0.1:1234/index.m3u8?token=one')).toBe(true);
  expect(isHlsSource('/stream/video.mp4')).toBe(false);
  expect(isHlsSource(undefined)).toBe(false);
});
