// Best-effort XMLTV EPG lookup. The upstream EPG host is an external free
// source that may be unreachable; failures degrade to an empty guide so the
// channel list still works without it.

import { gunzipSync } from 'fflate';
import { desktopAwareFetch } from '../api/apiBase';
import { IPTV_EPG_URLS } from './iptvChannels';

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
  pending ??= fetchGuides(options.fetchImpl ?? desktopAwareFetch)
    .then((texts) => mergeSchedules(texts.map(parseProgrammes)))
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

// The upstream url-tvg list mixes plain and gzip-compressed sources; a dead
// source must not take down the rest of the guide.
async function fetchGuides(fetchImpl: typeof desktopAwareFetch): Promise<string[]> {
  const texts = await Promise.all(
    IPTV_EPG_URLS.map((url) => fetchGuide(fetchImpl, url).catch(() => '')),
  );
  return texts.filter(Boolean);
}

async function fetchGuide(fetchImpl: typeof desktopAwareFetch, url: string): Promise<string> {
  const response = await fetchImpl(url, { signal: AbortSignal.timeout(EPG_TIMEOUT_MS) });
  if (!response.ok) throw new Error(`epg http ${String(response.status)}`);
  return decodeEpgBody(await response.arrayBuffer());
}

// .gz sources arrive as opaque file content (no transport Content-Encoding),
// so sniff the gzip magic instead of trusting the extension.
export function decodeEpgBody(body: ArrayBuffer): string {
  const bytes = new Uint8Array(body);
  const data = bytes[0] === 0x1f && bytes[1] === 0x8b ? gunzipSync(bytes) : bytes;
  return new TextDecoder().decode(data);
}

export function mergeSchedules(
  sources: Map<string, IptvProgramme[]>[],
): Map<string, IptvProgramme[]> {
  const merged = new Map<string, IptvProgramme[]>();
  const seen = new Map<string, Set<string>>();
  for (const source of sources) {
    for (const [channel, programmes] of source) {
      const list = merged.get(channel) ?? [];
      const keys = seen.get(channel) ?? new Set<string>();
      for (const programme of programmes) {
        const key = `${String(programme.start)}:${String(programme.stop)}:${programme.title}`;
        if (keys.has(key)) continue;
        keys.add(key);
        list.push(programme);
      }
      merged.set(channel, list);
      seen.set(channel, keys);
    }
  }
  for (const list of merged.values()) list.sort((a, b) => a.start - b.start);
  return merged;
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
