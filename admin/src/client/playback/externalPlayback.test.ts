import {
  buildExternalPlayerUrl,
  createExternalPlaybackLink,
  detectExternalPlayerPlatform,
  externalPlayersForPlatform,
  ExternalPlaybackUnavailableError,
} from './externalPlayback';

const playback = vi.hoisted(() => ({
  getPlaybackInfo: vi.fn(),
  issuePlaybackTicket: vi.fn(),
}));

vi.mock('../api/playbackApi', () => playback);

beforeEach(() => {
  playback.getPlaybackInfo.mockReset();
  playback.issuePlaybackTicket.mockReset();
});

it('creates an absolute temporary playback link for the preferred native source', async () => {
  playback.getPlaybackInfo.mockResolvedValue({
    PlaySessionId: 'session-1',
    MediaSources: [
      { Id: 'fallback', DirectStreamUrl: '/fallback', SupportsDirectPlay: true },
      { Id: 'preferred', DirectStreamUrl: '/preferred', SupportsDirectPlay: true, IsDefault: true },
    ],
  });
  playback.issuePlaybackTicket.mockResolvedValue({
    Id: 'ticket-1',
    Ticket: 'secret-ticket',
    ExpiresAt: '2026-08-29T20:00:00Z',
    StreamUrl: '/Videos/movie-1/stream?PlaybackTicket=secret-ticket',
  });

  await expect(createExternalPlaybackLink('movie-1')).resolves.toEqual({
    expiresAt: '2026-08-29T20:00:00Z',
    streamUrl: 'http://localhost:3000/Videos/movie-1/stream?PlaybackTicket=secret-ticket',
  });
  expect(playback.issuePlaybackTicket).toHaveBeenCalledWith('movie-1', 'preferred', 'session-1');
});

it('rejects external playback when no direct-play source is available', async () => {
  playback.getPlaybackInfo.mockResolvedValue({
    PlaySessionId: 'session-1',
    MediaSources: [{ Id: 'transcode-only', SupportsDirectPlay: false }],
  });

  await expect(createExternalPlaybackLink('movie-1')).rejects.toBeInstanceOf(ExternalPlaybackUnavailableError);
  expect(playback.issuePlaybackTicket).not.toHaveBeenCalled();
});

it.each([
  ['Mozilla/5.0 (iPhone)', 'iPhone', 'ios'],
  ['Mozilla/5.0 (Linux; Android 15)', 'Linux armv8l', 'android'],
  ['Mozilla/5.0 (Windows NT 10.0)', 'Win32', 'windows'],
  ['Mozilla/5.0 (Macintosh; Intel Mac OS X)', 'MacIntel', 'macos'],
  ['Mozilla/5.0 (X11; Linux x86_64)', 'Linux x86_64', 'linux'],
] as const)('detects %s as %s', (userAgent, platform, expected) => {
  expect(detectExternalPlayerPlatform(userAgent, platform)).toBe(expected);
});

it('offers the approved player set for macOS', () => {
  expect(externalPlayersForPlatform('macos').map((player) => player.id))
    .toEqual(['iina', 'vlc', 'infuse', 'nplayer']);
});

it('fully encodes nested playback URLs for query-based player protocols', () => {
  const streamUrl = 'https://media.example/Video/1?PlaybackTicket=a+b&mediaSourceId=source/1';

  expect(buildExternalPlayerUrl('iina', streamUrl, 'Demo Movie', 'macos'))
    .toBe(`iina://weblink?url=${encodeURIComponent(streamUrl)}&new_window=1`);
  expect(buildExternalPlayerUrl('infuse', streamUrl, 'Demo Movie', 'macos'))
    .toBe(`infuse://x-callback-url/play?url=${encodeURIComponent(streamUrl)}&filename=Demo%20Movie`);
  expect(buildExternalPlayerUrl('vlc', streamUrl, 'Demo Movie', 'ios'))
    .toBe(`vlc-x-callback://x-callback-url/stream?url=${encodeURIComponent(streamUrl)}`);
});

it('uses the native path-style schemes expected by PotPlayer and nPlayer', () => {
  const streamUrl = 'https://media.example/Video/1?PlaybackTicket=ticket';

  expect(buildExternalPlayerUrl('potplayer', streamUrl, 'Demo', 'windows'))
    .toBe(`potplayer://${encodeURI(streamUrl)}`);
  expect(buildExternalPlayerUrl('nplayer', streamUrl, 'Demo', 'ios'))
    .toBe(`nplayer-${streamUrl}`);
});

it('refuses to wrap a non-http playback URL', () => {
  expect(() => buildExternalPlayerUrl('iina', 'file:///private/movie.mkv', 'Demo', 'macos'))
    .toThrow('invalid playback url');
});
