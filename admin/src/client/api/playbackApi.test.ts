import { ClientApiError } from './clientApi';
import {
  getPlaybackInfo,
  getSubtitleBlob,
  issuePlaybackTicket,
  reportPlaybackProgress,
  startPlayback,
  stopPlayback,
} from './playbackApi';

const client = vi.hoisted(() => ({
  clientBlob: vi.fn(),
  clientRequest: vi.fn(),
}));

vi.mock('./clientApi', async (importOriginal) => ({
  ...await importOriginal<typeof import('./clientApi')>(),
  ...client,
}));

beforeEach(() => {
  client.clientBlob.mockReset();
  client.clientRequest.mockReset();
});

afterEach(() => {
  vi.useRealTimers();
});

const preparing = (retryAfter = '2') => new ClientApiError(503, 'unavailable', retryAfter);

it('uses authenticated playstate and subtitle routes', async () => {
  client.clientRequest.mockResolvedValue(undefined);
  client.clientBlob.mockResolvedValue(new Blob(['WEBVTT']));
  const state = {
    itemId: 'item-1',
    mediaSourceId: 'source-1',
    playSessionId: 'session-1',
    positionTicks: 15_000_000,
  };

  await startPlayback(state);
  await reportPlaybackProgress(state);
  await stopPlayback(state);
  await getSubtitleBlob('/Videos/item-1/source-1/Subtitles/0/Stream.vtt');

  const body = JSON.stringify({
    ItemId: 'item-1',
    MediaSourceId: 'source-1',
    PlaySessionId: 'session-1',
    PositionTicks: 15_000_000,
  });
  expect(client.clientRequest).toHaveBeenNthCalledWith(1, '/Sessions/Playing', { method: 'POST', body });
  expect(client.clientRequest).toHaveBeenNthCalledWith(2, '/Sessions/Playing/Progress', { method: 'POST', body });
  expect(client.clientRequest).toHaveBeenNthCalledWith(3, '/Sessions/Playing/Stopped', { method: 'POST', body });
  expect(client.clientBlob).toHaveBeenCalledWith('/Videos/item-1/source-1/Subtitles/0/Stream.vtt', undefined);
});

it('keeps the final stop request alive while the page is unloading', async () => {
  client.clientRequest.mockResolvedValue(undefined);
  const state = {
    itemId: 'item-1',
    mediaSourceId: 'source-1',
    playSessionId: 'session-1',
    positionTicks: 15_000_000,
  };

  await stopPlayback(state, { keepalive: true });

  expect(client.clientRequest).toHaveBeenCalledWith('/Sessions/Playing/Stopped', {
    method: 'POST',
    body: JSON.stringify({
      ItemId: 'item-1',
      MediaSourceId: 'source-1',
      PlaySessionId: 'session-1',
      PositionTicks: 15_000_000,
    }),
    keepalive: true,
  });
});

it('encodes item ids in playback info and ticket routes', async () => {
  client.clientRequest.mockResolvedValue({ PlaySessionId: 'session-1' });

  await getPlaybackInfo('item/1?x');
  await issuePlaybackTicket('item/1?x', 'source-1', 'session-1');

  expect(client.clientRequest).toHaveBeenNthCalledWith(1, '/Items/item%2F1%3Fx/PlaybackInfo', {
    method: 'POST',
    body: JSON.stringify({}),
  });
  expect(client.clientRequest.mock.calls[1]?.[0]).toBe('/Items/item%2F1%3Fx/PlaybackTicket');
});

it('retries playback info while the server reports it is preparing, honoring Retry-After', async () => {
  vi.useFakeTimers();
  client.clientRequest
    .mockRejectedValueOnce(preparing('3'))
    .mockResolvedValueOnce({ PlaySessionId: 'session-1' });
  const signal = new AbortController().signal;

  const result = getPlaybackInfo('movie-1', signal);
  await vi.advanceTimersByTimeAsync(2_999);
  expect(client.clientRequest).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(1);

  await expect(result).resolves.toEqual({ PlaySessionId: 'session-1' });
  expect(client.clientRequest).toHaveBeenCalledTimes(2);
  expect(client.clientRequest).toHaveBeenLastCalledWith('/Items/movie-1/PlaybackInfo', {
    method: 'POST',
    body: JSON.stringify({}),
    signal,
  });
});

it('clamps Retry-After and falls back to the default for unparseable values', async () => {
  vi.useFakeTimers();
  client.clientRequest
    .mockRejectedValueOnce(preparing('0'))
    .mockRejectedValueOnce(preparing('120'))
    .mockRejectedValueOnce(preparing('soon'))
    .mockResolvedValueOnce({ PlaySessionId: 'session-1' });

  const result = getPlaybackInfo('movie-1');
  await vi.advanceTimersByTimeAsync(1_000);
  expect(client.clientRequest).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(9_999);
  expect(client.clientRequest).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(1);
  expect(client.clientRequest).toHaveBeenCalledTimes(3);
  await vi.advanceTimersByTimeAsync(2_000);

  await expect(result).resolves.toEqual({ PlaySessionId: 'session-1' });
  expect(client.clientRequest).toHaveBeenCalledTimes(4);
});

it('does not retry final playback info failures', async () => {
  const failed = new ClientApiError(503, 'unavailable');
  client.clientRequest.mockRejectedValueOnce(failed);
  await expect(getPlaybackInfo('movie-1')).rejects.toBe(failed);

  const missing = new ClientApiError(404, 'not-found', '2');
  client.clientRequest.mockRejectedValueOnce(missing);
  await expect(getPlaybackInfo('movie-1')).rejects.toBe(missing);

  expect(client.clientRequest).toHaveBeenCalledTimes(2);
});

it('gives up once the preparing budget would be exceeded', async () => {
  vi.useFakeTimers();
  const lastError = preparing('10');
  client.clientRequest.mockRejectedValue(lastError);

  const result = getPlaybackInfo('movie-1');
  const assertion = expect(result).rejects.toBe(lastError);
  await vi.advanceTimersByTimeAsync(60_000);

  await assertion;
  expect(client.clientRequest).toHaveBeenCalledTimes(5);
});

it('stops retrying when the caller aborts', async () => {
  vi.useFakeTimers();
  client.clientRequest.mockRejectedValue(preparing('2'));
  const controller = new AbortController();

  const result = getPlaybackInfo('movie-1', controller.signal);
  const assertion = expect(result).rejects.toMatchObject({ name: 'AbortError' });
  await vi.advanceTimersByTimeAsync(500);
  controller.abort();
  await assertion;
  await vi.advanceTimersByTimeAsync(10_000);

  expect(client.clientRequest).toHaveBeenCalledTimes(1);
});
