import { extractPlayUrls, IptvResolveError, resolveIptvChannel } from './iptvApi';
import type { IptvChannel } from './iptvChannels';

const channel: IptvChannel = {
  defn: 'fhd',
  name: 'CCTV-1 综合',
  pid: '600001859',
  sid: '2024078201',
  slug: 'cctv1',
  timeshift: true,
  tvgId: 'CCTV1',
};

function jsonResponse(payload: unknown, status = 200): Response {
  return new Response(JSON.stringify(payload), {
    headers: { 'Content-Type': 'application/json' },
    status,
  });
}

describe('extractPlayUrls', () => {
  it('collects playurl and backurl entries, filters non-cctv hosts, and prefers bklive-', () => {
    const urls = extractPlayUrls({
      backurl_list: [
        { url: 'https://cdn.other.com/a.m3u8' },
        'https://bklive-b.ysp.cctv.cn/b.m3u8',
        { playurl: 'https://vdn.live.cctv.cn/c.m3u8' },
      ],
      iretcode: 0,
      playurl: 'https://bktlivecloud-cdn.ysp.cctv.cn/a.m3u8',
    });
    expect(urls).toEqual([
      'https://bklive-b.ysp.cctv.cn/b.m3u8',
      'https://bktlivecloud-cdn.ysp.cctv.cn/a.m3u8',
      'https://vdn.live.cctv.cn/c.m3u8',
    ]);
  });

  it('splits string backurl lists and dedupes', () => {
    const urls = extractPlayUrls({
      backurl: 'https://a.cctv.cn/1.m3u8; https://a.cctv.cn/1.m3u8, https://b.cctv.cn/2.m3u8',
      iretcode: 0,
    });
    expect(urls).toEqual(['https://a.cctv.cn/1.m3u8', 'https://b.cctv.cn/2.m3u8']);
  });

  it('rejects non-zero iretcode', () => {
    expect(() => extractPlayUrls({ errinfo: 'bad key', iretcode: -1 }))
      .toThrow(IptvResolveError);
  });

  it('rejects responses without a cctv url', () => {
    expect(() => extractPlayUrls({ iretcode: 0, playurl: 'https://example.com/x.m3u8' }))
      .toThrow(IptvResolveError);
  });
});

describe('resolveIptvChannel', () => {
  it('queries bkliveinfo with the channel ids and signed token', async () => {
    const fetchImpl = vi.fn((url: string, init?: RequestInit) => {
      expect(url.startsWith('https://bkliveinfo.ysp.cctv.cn/?')).toBe(true);
      const params = new URL(url).searchParams;
      expect(params.get('cnlid')).toBe('2024078201');
      expect(params.get('livepid')).toBe('600001859');
      expect(params.get('defn')).toBe('fhd');
      expect(params.get('cKey')).toMatch(/^--01[-\w]+$/);
      expect(params.get('fntick')).toMatch(/^\d{9,}$/);
      expect((init?.headers as Record<string, string>)['User-Agent']).toBe('qqlive');
      return Promise.resolve(jsonResponse({ iretcode: 0, playurl: 'https://bklive-a.ysp.cctv.cn/x.m3u8' }));
    });
    await expect(resolveIptvChannel(channel, { fetchImpl })).resolves.toEqual([
      'https://bklive-a.ysp.cctv.cn/x.m3u8',
    ]);
    expect(fetchImpl).toHaveBeenCalledTimes(1);
  });

  it('wraps network failures in IptvResolveError', async () => {
    const fetchImpl = vi.fn(() => Promise.reject(new Error('offline')));
    await expect(resolveIptvChannel(channel, { fetchImpl })).rejects.toBeInstanceOf(IptvResolveError);
  });

  it('wraps non-200 responses', async () => {
    const fetchImpl = vi.fn(() => Promise.resolve(jsonResponse({}, 502)));
    await expect(resolveIptvChannel(channel, { fetchImpl })).rejects.toBeInstanceOf(IptvResolveError);
  });
});
