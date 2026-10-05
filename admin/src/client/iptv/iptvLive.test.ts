import type { IptvChannel } from './iptvChannels';
import { JceDeadHostError } from './iptvJce';
import { IptvLiveSession } from './iptvLive';

const channel: IptvChannel = {
  defn: 'fhd',
  name: 'CCTV-1 综合',
  pid: '600001859',
  sid: '2024078201',
  slug: 'cctv1',
  timeshift: true,
  tvgId: 'CCTV1',
};

const bkChannel: IptvChannel = { ...channel, slug: 'cctv11', timeshift: false };

function windowPlaylist(segments: [pdt: string, url: string][]): string {
  const lines = ['#EXTM3U', '#EXT-X-VERSION:3', '#EXT-X-TARGETDURATION:6'];
  for (const [pdt, url] of segments) {
    lines.push(`#EXT-X-PROGRAM-DATE-TIME:${pdt}`);
    lines.push('#EXTINF:6.000,', url);
  }
  return `${lines.join('\n')}\n`;
}

function textResponse(body: string): Response {
  return new Response(body, { status: 200 });
}

describe('IptvLiveSession', () => {
  it('merges successive JCE windows into a rolling playlist', async () => {
    let tick = 0;
    const windows = [
      windowPlaylist([
        ['2026-10-21T08:00:00Z', 'http://cdn/a.ts'],
        ['2026-10-21T08:00:06Z', 'http://cdn/b.ts?old=1'],
      ]),
      windowPlaylist([
        ['2026-10-21T08:00:06Z', 'http://cdn/b.ts?new=1'],
        ['2026-10-21T08:00:12Z', 'http://cdn/c.ts'],
      ]),
    ];
    const session = new IptvLiveSession(channel, {
      fetchImpl: () => Promise.resolve(textResponse(windows[Math.min(tick, 1)] ?? '')),
      timeshiftUrl: () => Promise.resolve('http://tlivecloud-playback-cdn.ysp.cctv.cn/win.m3u8'),
    });

    const first = await session.manifest();
    expect(first).toContain('#EXT-X-MEDIA-SEQUENCE:1');
    expect(first).toContain('http://cdn/a.ts');
    expect(first).toContain('http://cdn/b.ts?old=1');

    // Advance past the refresh interval so the next poll pulls window 2.
    (session as unknown as { lastRefresh: number }).lastRefresh = 0;
    tick = 1;
    const second = await session.manifest();
    // b.ts deduped by PDT key and its URL refreshed; a.ts still visible.
    expect(second).toContain('http://cdn/a.ts');
    expect(second).toContain('http://cdn/b.ts?new=1');
    expect(second).toContain('http://cdn/c.ts');
    expect(second.match(/#EXTINF/g)?.length).toBe(3);
    expect(second).toContain('#EXT-X-MEDIA-SEQUENCE:1');
  });

  it('falls back to bkliveinfo when the JCE host is dead', async () => {
    const bkPlaylist = '#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.0,\nseg.ts\n';
    const session = new IptvLiveSession(channel, {
      fetchImpl: () => Promise.resolve(textResponse(bkPlaylist)),
      resolveBk: () => Promise.resolve(['https://bklive-b.ysp.cctv.cn/x.m3u8']),
      timeshiftUrl: () => Promise.reject(new JceDeadHostError('dead cdn host')),
    });
    const playlist = await session.manifest();
    expect(playlist).toContain('seg.ts');
    expect(playlist).toContain('https://bklive-b.ysp.cctv.cn/seg.ts');
  });

  it('serves the last playlist when a refresh fails but content exists', async () => {
    let calls = 0;
    const session = new IptvLiveSession(channel, {
      fetchImpl: () => {
        calls += 1;
        if (calls === 1) return Promise.resolve(textResponse(windowPlaylist([['2026-10-21T08:00:00Z', 'http://cdn/a.ts']])));
        return Promise.resolve(new Response('nope', { status: 500 }));
      },
      timeshiftUrl: () => Promise.resolve('http://cdn/win.m3u8'),
    });
    const first = await session.manifest();
    expect(first).toContain('http://cdn/a.ts');
    (session as unknown as { lastRefresh: number }).lastRefresh = 0;
    const second = await session.manifest();
    expect(second).toContain('http://cdn/a.ts');
  });

  it('rejects when nothing has ever loaded', async () => {
    const session = new IptvLiveSession(channel, {
      fetchImpl: () => Promise.resolve(new Response('err', { status: 500 })),
      timeshiftUrl: () => Promise.resolve('http://cdn/win.m3u8'),
      resolveBk: () => Promise.reject(new Error('bk down')),
    });
    await expect(session.manifest()).rejects.toThrow();
  });

  it('uses bk mode directly for channels without timeshift coverage', async () => {
    const bkPlaylist = '#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.0,\nhttp://cdn/seg.ts\n';
    const session = new IptvLiveSession(bkChannel, {
      fetchImpl: () => Promise.resolve(textResponse(bkPlaylist)),
      resolveBk: () => Promise.resolve(['https://bk/x.m3u8']),
      timeshiftUrl: () => Promise.reject(new Error('must not be called')),
    });
    expect(await session.manifest()).toContain('http://cdn/seg.ts');
  });
});
