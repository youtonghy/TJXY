import {
  getSubtitleBlob,
  reportPlaybackProgress,
  startPlayback,
  stopPlayback,
} from './playbackApi';

const client = vi.hoisted(() => ({
  clientBlob: vi.fn(),
  clientRequest: vi.fn(),
}));

vi.mock('./clientApi', () => client);

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
