import { attachHlsSource, isHlsSource } from './hlsPlayback';

const loaded: string[] = [];
let mseSupported = true;

vi.mock('hls.js', () => ({
  default: class FakeHls {
    static isSupported() {
      return mseSupported;
    }
    static Events = { ERROR: 'hlsError' };
    static ErrorTypes = { MEDIA_ERROR: 'mediaError', NETWORK_ERROR: 'networkError' };
    on() {
      // noop
    }
    loadSource(source: string) {
      loaded.push(source);
    }
    attachMedia() {
      // noop
    }
    destroy() {
      // noop
    }
  },
}));

function fakeVideo(canPlay: CanPlayTypeResult): HTMLVideoElement {
  const video = document.createElement('video');
  vi.spyOn(video, 'canPlayType').mockReturnValue(canPlay);
  return video;
}

it('recognizes HLS playlists with query parameters', () => {
  expect(isHlsSource('http://127.0.0.1:1234/index.m3u8?token=one')).toBe(true);
  expect(isHlsSource('/stream/video.mp4')).toBe(false);
  expect(isHlsSource(undefined)).toBe(false);
});

describe('attachHlsSource', () => {
  beforeEach(() => {
    loaded.length = 0;
    mseSupported = true;
  });

  // Chromium reports "maybe" for native HLS but its demuxer fails on many live
  // playlists; hls.js must win whenever MediaSource is available.
  it('prefers hls.js over a "maybe" native canPlayType answer', async () => {
    const video = fakeVideo('maybe');
    await attachHlsSource(video, 'https://cdn.example.com/live.m3u8', () => undefined);
    expect(loaded).toEqual(['https://cdn.example.com/live.m3u8']);
    expect(video.getAttribute('src')).toBeNull();
  });

  it('uses native playback when MediaSource is unavailable', async () => {
    mseSupported = false;
    const video = fakeVideo('probably');
    await attachHlsSource(video, 'https://cdn.example.com/live.m3u8', () => undefined);
    expect(loaded).toEqual([]);
    expect(video.getAttribute('src')).toBe('https://cdn.example.com/live.m3u8');
  });

  it('reports a fatal error when neither path can play', async () => {
    mseSupported = false;
    const video = fakeVideo('');
    const onFatal = vi.fn();
    await attachHlsSource(video, 'https://cdn.example.com/live.m3u8', onFatal);
    expect(onFatal).toHaveBeenCalledTimes(1);
  });
});
