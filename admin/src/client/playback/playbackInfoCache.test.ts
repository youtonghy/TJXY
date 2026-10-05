import {
  clearPlaybackInfoCache,
  PLAYBACK_INFO_TTL_MS,
  prefetchPlaybackInfo,
  takePlaybackInfo,
} from './playbackInfoCache';

const playback = vi.hoisted(() => ({
  getPlaybackInfo: vi.fn(),
}));

vi.mock('../api/playbackApi', () => playback);

const info = { PlaySessionId: 'session-1', MediaSources: [{ Id: 'source-1' }] };

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function requestSignal(call = 0): AbortSignal {
  return playback.getPlaybackInfo.mock.calls[call]?.[1] as AbortSignal;
}

beforeEach(() => {
  clearPlaybackInfoCache();
  playback.getPlaybackInfo.mockReset();
  playback.getPlaybackInfo.mockResolvedValue(info);
});

afterEach(() => {
  vi.useRealTimers();
});

it('shares one in-flight prefetch and hands it to the next play action', async () => {
  prefetchPlaybackInfo('movie-1');
  prefetchPlaybackInfo('movie-1');

  await expect(takePlaybackInfo('movie-1')).resolves.toBe(info);
  expect(playback.getPlaybackInfo).toHaveBeenCalledTimes(1);
  expect(playback.getPlaybackInfo).toHaveBeenCalledWith('movie-1', expect.any(AbortSignal));
});

it('gives every play action its own play session', async () => {
  prefetchPlaybackInfo('movie-1');
  await takePlaybackInfo('movie-1');

  await takePlaybackInfo('movie-1');
  expect(playback.getPlaybackInfo).toHaveBeenCalledTimes(2);
});

it('expires an unused prefetch after the TTL', async () => {
  vi.useFakeTimers();
  prefetchPlaybackInfo('movie-1');
  await vi.waitFor(() => {
    expect(playback.getPlaybackInfo).toHaveBeenCalledTimes(1);
  });
  await Promise.resolve();

  vi.advanceTimersByTime(PLAYBACK_INFO_TTL_MS);
  await takePlaybackInfo('movie-1');
  expect(playback.getPlaybackInfo).toHaveBeenCalledTimes(2);
});

it('drops a failed prefetch so the play action asks the server again', async () => {
  playback.getPlaybackInfo.mockRejectedValueOnce(new Error('preparation failed'));
  prefetchPlaybackInfo('movie-1');
  await Promise.resolve();
  await Promise.resolve();

  await expect(takePlaybackInfo('movie-1')).resolves.toBe(info);
  expect(playback.getPlaybackInfo).toHaveBeenCalledTimes(2);
});

it('cancels a page-scoped prefetch when its page goes away', async () => {
  const pending = deferred<typeof info>();
  playback.getPlaybackInfo.mockReturnValueOnce(pending.promise);
  const page = new AbortController();

  prefetchPlaybackInfo('movie-1', page.signal);
  page.abort();

  expect(requestSignal().aborted).toBe(true);
  pending.reject(new DOMException('aborted', 'AbortError'));
  await expect(takePlaybackInfo('movie-1')).resolves.toBe(info);
  expect(playback.getPlaybackInfo).toHaveBeenCalledTimes(2);
});

it('keeps a page-scoped prefetch alive once the user showed intent to play', async () => {
  const pending = deferred<typeof info>();
  playback.getPlaybackInfo.mockReturnValueOnce(pending.promise);
  const page = new AbortController();

  prefetchPlaybackInfo('movie-1', page.signal);
  prefetchPlaybackInfo('movie-1');
  page.abort();

  expect(requestSignal().aborted).toBe(false);
  pending.resolve(info);
  await expect(takePlaybackInfo('movie-1')).resolves.toBe(info);
  expect(playback.getPlaybackInfo).toHaveBeenCalledTimes(1);
});

it('cancels a taken request when its play action aborts', async () => {
  const pending = deferred<typeof info>();
  playback.getPlaybackInfo.mockReturnValueOnce(pending.promise);
  prefetchPlaybackInfo('movie-1');
  const player = new AbortController();

  const taken = takePlaybackInfo('movie-1', player.signal);
  player.abort();

  expect(requestSignal().aborted).toBe(true);
  pending.reject(new DOMException('aborted', 'AbortError'));
  await expect(taken).rejects.toMatchObject({ name: 'AbortError' });
});

it('rejects an already aborted play action without starting a request', async () => {
  const controller = new AbortController();
  controller.abort();

  await expect(takePlaybackInfo('movie-1', controller.signal)).rejects.toMatchObject({ name: 'AbortError' });
  expect(playback.getPlaybackInfo).not.toHaveBeenCalled();
});
