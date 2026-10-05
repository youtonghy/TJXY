// Best-effort XMLTV EPG lookup. The upstream EPG host is an external free
// source that may be unreachable; failures degrade to an empty guide so the
// channel list still works without it.

import { desktopAwareFetch } from '../api/apiBase';
import { IPTV_EPG_URL } from './iptvChannels';

const EPG_TTL_MS = 30 * 60 * 1000;
const EPG_TIMEOUT_MS = 20_000;

export interface IptvProgramme {
  title: string;
  start: number;
  stop: number;
}

interface CachedGuide {
  at: number;
  schedules: Map<string, IptvProgramme[]>;
}

let cached: CachedGuide | undefined;
let pending: Promise<Map<string, IptvProgramme[]>> | undefined;

/** Full programme list per channel id, sorted by start time. */
export async function loadIptvProgrammes(
  options: { fetchImpl?: typeof desktopAwareFetch; now?: number } = {},
): Promise<Map<string, IptvProgramme[]>> {
  const now = options.now ?? Date.now();
  if (cached && now - cached.at < EPG_TTL_MS) return cached.schedules;
  pending ??= fetchGuide(options.fetchImpl ?? desktopAwareFetch)
    .then(parseProgrammes)
    .then((schedules) => {
      cached = { at: now, schedules };
      return schedules;
    })
    .catch(() => new Map<string, IptvProgramme[]>())
    .finally(() => {
      pending = undefined;
    });
  return pending;
}

export async function loadIptvGuide(
  options: { fetchImpl?: typeof desktopAwareFetch; now?: number } = {},
): Promise<Map<string, IptvProgramme>> {
  const now = options.now ?? Date.now();
  const schedules = await loadIptvProgrammes(options);
  const current = new Map<string, IptvProgramme>();
  for (const [channel, programmes] of schedules) {
    for (let i = programmes.length - 1; i >= 0; i--) {
      const programme = programmes[i];
      if (programme && now >= programme.start && now < programme.stop) {
        current.set(channel, programme);
        break;
      }
    }
  }
  return current;
}

export function currentProgrammes(xml: string, now: number): Map<string, IptvProgramme> {
  const current = new Map<string, IptvProgramme>();
  for (const [channel, programmes] of parseProgrammes(xml)) {
    for (let i = programmes.length - 1; i >= 0; i--) {
      const programme = programmes[i];
      if (programme && now >= programme.start && now < programme.stop) {
        current.set(channel, programme);
        break;
      }
    }
  }
  return current;
}

export function parseProgrammes(xml: string): Map<string, IptvProgramme[]> {
  const schedules = new Map<string, IptvProgramme[]>();
  const programmeRe = /<programme\b([^>]*)>([\s\S]*?)<\/programme>/g;
  for (const match of xml.matchAll(programmeRe)) {
    const attrs = match[1] ?? '';
    const start = parseXmltvTime(attr(attrs, 'start'));
    const stop = parseXmltvTime(attr(attrs, 'stop'));
    if (!Number.isFinite(start) || !Number.isFinite(stop)) continue;
    const channel = attr(attrs, 'channel');
    if (!channel) continue;
    const title = decodeEntities(/<title\b[^>]*>([\s\S]*?)<\/title>/.exec(match[2] ?? '')?.[1] ?? '');
    if (!title) continue;
    const list = schedules.get(channel) ?? [];
    list.push({ title, start, stop });
    schedules.set(channel, list);
  }
  for (const list of schedules.values()) list.sort((a, b) => a.start - b.start);
  return schedules;
}

async function fetchGuide(fetchImpl: typeof desktopAwareFetch): Promise<string> {
  const response = await fetchImpl(IPTV_EPG_URL, { signal: AbortSignal.timeout(EPG_TIMEOUT_MS) });
  if (!response.ok) throw new Error(`epg http ${String(response.status)}`);
  return response.text();
}

function attr(source: string, name: string): string {
  return new RegExp(`\\b${name}="([^"]*)"`).exec(source)?.[1] ?? '';
}

function parseXmltvTime(value: string): number {
  const match = /^(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})\s*([+-]\d{4})?/.exec(value);
  if (!match) return Number.NaN;
  const utc = Date.UTC(
    Number(match[1]), Number(match[2]) - 1, Number(match[3]),
    Number(match[4]), Number(match[5]), Number(match[6]),
  );
  const zone = match[7];
  if (!zone) return utc;
  const offset = (Number(zone.slice(1, 3)) * 60 + Number(zone.slice(3))) * 60_000;
  return zone.startsWith('+') ? utc - offset : utc + offset;
}

function decodeEntities(value: string): string {
  const cdata = /<!\[CDATA\[([\s\S]*?)\]\]>/.exec(value);
  const raw = cdata?.[1] ?? value;
  return raw
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&quot;', '"')
    .replaceAll('&apos;', "'")
    .replaceAll('&amp;', '&')
    .trim();
}
