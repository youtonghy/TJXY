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
  programmes: Map<string, IptvProgramme>;
}

let cached: CachedGuide | undefined;
let pending: Promise<Map<string, IptvProgramme>> | undefined;

export async function loadIptvGuide(options: { fetchImpl?: typeof desktopAwareFetch; now?: number } = {}): Promise<Map<string, IptvProgramme>> {
  const now = options.now ?? Date.now();
  if (cached && now - cached.at < EPG_TTL_MS) return cached.programmes;
  pending ??= fetchGuide(options.fetchImpl ?? desktopAwareFetch)
    .then((xml) => currentProgrammes(xml, now))
    .then((programmes) => {
      cached = { at: now, programmes };
      return programmes;
    })
    .catch(() => new Map<string, IptvProgramme>())
    .finally(() => {
      pending = undefined;
    });
  return pending;
}

export function currentProgrammes(xml: string, now: number): Map<string, IptvProgramme> {
  const programmes = new Map<string, IptvProgramme>();
  const programmeRe = /<programme\b([^>]*)>([\s\S]*?)<\/programme>/g;
  for (const match of xml.matchAll(programmeRe)) {
    const attrs = match[1] ?? '';
    const start = parseXmltvTime(attr(attrs, 'start'));
    const stop = parseXmltvTime(attr(attrs, 'stop'));
    if (!Number.isFinite(start) || !Number.isFinite(stop) || now < start || now >= stop) continue;
    const channel = attr(attrs, 'channel');
    if (!channel) continue;
    const title = decodeEntities(/<title\b[^>]*>([\s\S]*?)<\/title>/.exec(match[2] ?? '')?.[1] ?? '');
    if (!title) continue;
    const existing = programmes.get(channel);
    if (!existing || start > existing.start) programmes.set(channel, { title, start, stop });
  }
  return programmes;
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
