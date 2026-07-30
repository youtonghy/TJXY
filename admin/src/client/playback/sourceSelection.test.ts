import { selectBrowserSource } from './sourceSelection';

it('prefers compatible MP4 direct play and rejects an MKV-only source list', () => {
  const mkv = { Id: 'mkv', Container: 'mkv', SupportsDirectPlay: true, DirectStreamUrl: '/Videos/item/stream' };
  const mp4 = { Id: 'mp4', Container: 'mp4', SupportsDirectPlay: true, DirectStreamUrl: '/Videos/item/stream' };
  expect(selectBrowserSource([mkv, mp4])?.Id).toBe('mp4');
  expect(selectBrowserSource([mkv])).toBeNull();
});
