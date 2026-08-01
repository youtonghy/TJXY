import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { PlayerPage } from './PlayerPage';

const catalog = vi.hoisted(() => ({
  getItem: vi.fn(),
  togglePlayed: vi.fn(),
}));
const playback = vi.hoisted(() => ({
  getPlaybackInfo: vi.fn(),
  getSubtitleBlob: vi.fn(),
  issuePlaybackTicket: vi.fn(),
  reportPlaybackProgress: vi.fn(),
  revokePlaybackTicket: vi.fn(),
  startPlayback: vi.fn(),
  stopPlayback: vi.fn(),
}));
const auth = vi.hoisted(() => ({
  useClientAuth: vi.fn(() => ({
    user: { Id: 'user-1', Name: 'Admin' },
    isLoading: false,
  })),
}));

vi.mock('../api/catalogApi', () => catalog);
vi.mock('../api/playbackApi', () => playback);
vi.mock('../auth/ClientAuthContext', () => auth);

beforeEach(() => {
  vi.clearAllMocks();
  Object.defineProperty(globalThis.crypto, 'randomUUID', {
    configurable: true,
    value: vi.fn(() => 'session-2'),
  });
  playback.getSubtitleBlob.mockResolvedValue(new Blob(['WEBVTT']));
  playback.revokePlaybackTicket.mockResolvedValue(undefined);
  playback.startPlayback.mockResolvedValue(undefined);
  playback.reportPlaybackProgress.mockResolvedValue(undefined);
  playback.stopPlayback.mockResolvedValue(undefined);
  catalog.togglePlayed.mockResolvedValue(undefined);
  Object.defineProperty(URL, 'createObjectURL', {
    configurable: true,
    value: vi.fn(() => 'blob:subtitle'),
  });
  Object.defineProperty(URL, 'revokeObjectURL', {
    configurable: true,
    value: vi.fn(),
  });
});

function renderPlayer() {
  return render(
    <MemoryRouter initialEntries={['/app/play/movie-1']}>
      <Routes>
        <Route element={<PlayerPage />} path="/app/play/:id" />
      </Routes>
    </MemoryRouter>,
  );
}

it('shows a neutral no-source state without requesting playback', async () => {
  catalog.getItem.mockResolvedValue({
    Id: 'movie-1',
    Name: 'Arrival',
    Type: 'Movie',
    HasMediaSources: false,
  });

  renderPlayer();

  expect(await screen.findByText('No video source available')).toBeInTheDocument();
  expect(screen.queryByText(/demo/i)).not.toBeInTheDocument();
  expect(screen.getByRole('link', { name: 'Back to details' })).toHaveAttribute('href', '/app/items/movie-1');
  expect(document.querySelector('video')).toBeNull();
  expect(playback.getPlaybackInfo).not.toHaveBeenCalled();
  expect(playback.issuePlaybackTicket).not.toHaveBeenCalled();
});

it('switches sources, loads subtitles, and reports the playback lifecycle', async () => {
  const user = userEvent.setup();
  catalog.getItem.mockResolvedValue({
    Id: 'movie-1',
    Name: 'Arrival',
    Type: 'Movie',
    HasMediaSources: true,
    UserData: { PlaybackPositionTicks: 20_000_000 },
  });
  playback.getPlaybackInfo.mockResolvedValue({
    PlaySessionId: 'session-1',
    MediaSources: [
      {
        Id: 'source-1080',
        Name: '1080p',
        Container: 'mp4',
        IsDefault: true,
        SupportsDirectPlay: true,
        DirectStreamUrl: '/Videos/movie-1/stream',
        MediaStreams: [
          { Type: 'Video', Codec: 'h264', Width: 1920, Height: 1080 },
          { Type: 'Subtitle', Language: 'zh-CN', Index: 0, IsExternal: true, IsDefault: true, DeliveryUrl: '/Videos/movie-1/source-1080/Subtitles/0/Stream.vtt' },
          { Type: 'Subtitle', Language: 'en', Index: 1, IsExternal: true, DeliveryUrl: '/Videos/movie-1/source-1080/Subtitles/1/Stream.vtt' },
        ],
      },
      {
        Id: 'source-720',
        Name: '720p',
        Container: 'mp4',
        SupportsDirectPlay: true,
        DirectStreamUrl: '/Videos/movie-1/stream',
        MediaStreams: [{ Type: 'Video', Codec: 'h264', Width: 1280, Height: 720 }],
      },
    ],
  });
  playback.issuePlaybackTicket
    .mockResolvedValueOnce({ Id: 'ticket-1080', StreamUrl: '/stream/1080' })
    .mockResolvedValueOnce({ Id: 'ticket-720', StreamUrl: '/stream/720' });

  renderPlayer();

  const video = await screen.findByLabelText('Playing Arrival');
  expect(video).toHaveAttribute('src', '/stream/1080');
  expect(playback.getSubtitleBlob).toHaveBeenCalledTimes(2);

  Object.defineProperty(video, 'currentTime', { configurable: true, writable: true, value: 2 });
  fireEvent.play(video);
  expect(playback.startPlayback).toHaveBeenCalledWith({
    itemId: 'movie-1',
    mediaSourceId: 'source-1080',
    playSessionId: 'session-1',
    positionTicks: 20_000_000,
  });
  Object.defineProperty(video, 'currentTime', { configurable: true, writable: true, value: 18 });
  fireEvent.timeUpdate(video);
  expect(playback.reportPlaybackProgress).toHaveBeenCalledWith({
    itemId: 'movie-1',
    mediaSourceId: 'source-1080',
    playSessionId: 'session-1',
    positionTicks: 180_000_000,
  });

  await user.click(screen.getByRole('button', { name: /Video source/ }));
  await user.click(await screen.findByRole('option', { name: /720p/ }));
  expect(await screen.findByLabelText('Playing Arrival')).toHaveAttribute('src', '/stream/720');
  expect(playback.issuePlaybackTicket).toHaveBeenNthCalledWith(
    2,
    'movie-1',
    'source-720',
    'session-2',
  );
  expect(playback.revokePlaybackTicket).toHaveBeenCalledWith('ticket-1080');

  fireEvent.loadedMetadata(screen.getByLabelText('Playing Arrival'));
  fireEvent.play(screen.getByLabelText('Playing Arrival'));
  expect(playback.startPlayback).toHaveBeenLastCalledWith({
    itemId: 'movie-1',
    mediaSourceId: 'source-720',
    playSessionId: 'session-2',
    positionTicks: 180_000_000,
  });

  fireEvent.ended(screen.getByLabelText('Playing Arrival'));
  expect(playback.stopPlayback).toHaveBeenCalled();
  expect(catalog.togglePlayed).toHaveBeenCalledWith('user-1', 'movie-1', true);
});

it('stops an active session with keepalive when the player leaves the page', async () => {
  catalog.getItem.mockResolvedValue({
    Id: 'movie-1',
    Name: 'Arrival',
    Type: 'Movie',
    HasMediaSources: true,
  });
  playback.getPlaybackInfo.mockResolvedValue({
    PlaySessionId: 'session-1',
    MediaSources: [{
      Id: 'source-1080',
      Container: 'mp4',
      IsDefault: true,
      SupportsDirectPlay: true,
      DirectStreamUrl: '/Videos/movie-1/stream',
      MediaStreams: [{ Type: 'Video', Codec: 'h264' }],
    }],
  });
  playback.issuePlaybackTicket.mockResolvedValue({
    Id: 'ticket-1080',
    StreamUrl: '/stream/1080',
  });

  const rendered = renderPlayer();
  const video = await screen.findByLabelText('Playing Arrival');
  Object.defineProperty(video, 'currentTime', {
    configurable: true,
    writable: true,
    value: 7,
  });
  fireEvent.play(video);

  window.dispatchEvent(new Event('pagehide'));

  expect(playback.stopPlayback).toHaveBeenCalledWith({
    itemId: 'movie-1',
    mediaSourceId: 'source-1080',
    playSessionId: 'session-1',
    positionTicks: 70_000_000,
  }, { keepalive: true });
  rendered.unmount();
  expect(playback.stopPlayback).toHaveBeenCalledTimes(1);
});
