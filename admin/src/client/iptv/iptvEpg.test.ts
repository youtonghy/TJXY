import { currentProgrammes, parseProgrammes } from './iptvEpg';

const XML = `<?xml version="1.0"?>
<tv>
  <programme channel="CCTV1" start="20250101180000 +0800" stop="20250101190000 +0800">
    <title lang="zh">晚间新闻</title>
  </programme>
  <programme channel="CCTV1" start="20250101190000 +0800" stop="20250101200000 +0800">
    <title lang="zh">联播 &amp; 天气</title>
  </programme>
  <programme channel="北京卫视" start="20250101110000 +0000" stop="20250101120000 +0000">
    <title><![CDATA[Beijing News]]></title>
  </programme>
</tv>`;

it('picks the programme airing at the given time', () => {
  const now = Date.UTC(2025, 0, 1, 10, 30); // 18:30 +0800
  const guide = currentProgrammes(XML, now);
  expect(guide.get('CCTV1')?.title).toBe('晚间新闻');
  expect(guide.get('北京卫视')).toBeUndefined();
});

it('decodes entities, cdata, and timezone offsets', () => {
  const now = Date.UTC(2025, 0, 1, 11, 30); // 19:30 +0800
  const guide = currentProgrammes(XML, now);
  expect(guide.get('CCTV1')?.title).toBe('联播 & 天气');
  expect(guide.get('北京卫视')?.title).toBe('Beijing News');
});

it('ignores malformed or future programmes', () => {
  const now = Date.UTC(2025, 0, 1, 10, 30);
  const guide = currentProgrammes('<tv><programme channel="X" start="bad" stop="20250101190000 +0800"><title>T</title></programme></tv>', now);
  expect(guide.size).toBe(0);
});

describe('parseProgrammes', () => {
  it('groups every programme by channel sorted by start time', () => {
    const schedules = parseProgrammes(XML);
    const cctv1 = schedules.get('CCTV1');
    expect(cctv1?.length).toBe(2);
    expect(cctv1?.[0]?.title).toBe('晚间新闻');
    expect(cctv1?.[1]?.title).toBe('联播 & 天气');
    expect(cctv1?.[0]?.start).toBeLessThan(cctv1?.[1]?.start ?? 0);
    expect(schedules.get('北京卫视')?.[0]?.title).toBe('Beijing News');
  });
});
