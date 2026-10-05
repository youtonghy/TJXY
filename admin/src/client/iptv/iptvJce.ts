import { gunzipSync, gzipSync } from 'fflate';
import { desktopAwareFetch } from '../api/apiBase';
import { IptvResolveError } from './iptvApi';

// Port of the JCE timeshift protocol from the upstream ysp-live.py: a
// JCE-encoded request wrapped in the qmf packet format, gzipped, and POSTed to
// jacc.ysp.cctv.cn. It returns a tcloud catchup playlist URL covering the
// requested window without any login or device session.
const JCE_URL = 'https://jacc.ysp.cctv.cn';
const JCE_CMD_TIMESHIFT = 25312;
const JCE_VER_NAME = '3.2.7.26212';
const JCE_VER_CODE = 302070;
const JCE_APP_ID = '1200013';
const JCE_QMF_APP_ID = 10012;
const JCE_QMF_PLATFORM = 1;
const JCE_BIZ_ID = 0;
const JCE_CHAN_ID = '10070';
const JCE_DEAD_HOST = 'liverecord.video.cloud.cctv.com';
const JCE_WINDOW_SECONDS = 300;
const JCE_MIN_INTERVAL_MS = 8000;

export class JceDeadHostError extends Error {}

class JceWriter {
  private bytes: number[] = [];

  private head(type: number, tag: number): void {
    if (tag < 15) {
      this.bytes.push(((tag & 15) << 4) | (type & 15));
    } else {
      this.bytes.push(0xf0 | (type & 15), tag);
    }
  }

  private raw(bytes: number[]): void {
    this.bytes.push(...bytes);
  }

  byte(value: number, tag: number): void {
    const v = value | 0;
    if (v === 0) {
      this.head(12, tag);
      return;
    }
    this.head(0, tag);
    this.raw([v & 0xff]);
  }

  short(value: number, tag: number): void {
    if (value >= -128 && value <= 127) {
      this.byte(value, tag);
      return;
    }
    this.head(1, tag);
    this.raw([(value >> 8) & 0xff, value & 0xff]);
  }

  int(value: number, tag: number): void {
    if (value >= -32768 && value <= 32767) {
      this.short(value, tag);
      return;
    }
    this.head(2, tag);
    this.raw([(value >>> 24) & 0xff, (value >>> 16) & 0xff, (value >>> 8) & 0xff, value & 0xff]);
  }

  long(value: number, tag: number): void {
    if (value >= -2147483648 && value <= 2147483647) {
      this.int(value, tag);
      return;
    }
    this.head(3, tag);
    const v = BigInt(Math.trunc(value)) & 0xffffffffffffffffn;
    const raw: number[] = [];
    for (let i = 7; i >= 0; i--) raw.push(Number((v >> BigInt(i * 8)) & 0xffn));
    this.raw(raw);
  }

  float(value: number, tag: number): void {
    this.head(4, tag);
    const view = new DataView(new ArrayBuffer(4));
    view.setFloat32(0, value);
    this.raw([view.getUint8(0), view.getUint8(1), view.getUint8(2), view.getUint8(3)]);
  }

  double(value: number, tag: number): void {
    this.head(5, tag);
    const view = new DataView(new ArrayBuffer(8));
    view.setFloat64(0, value);
    const raw: number[] = [];
    for (let i = 0; i < 8; i++) raw.push(view.getUint8(i));
    this.raw(raw);
  }

  string(value: string | null | undefined, tag: number): void {
    if (value === null || value === undefined) return;
    const data = new TextEncoder().encode(value);
    if (data.length > 255) {
      this.head(7, tag);
      this.raw([(data.length >>> 24) & 0xff, (data.length >>> 16) & 0xff, (data.length >>> 8) & 0xff, data.length & 0xff]);
    } else {
      this.head(6, tag);
      this.raw([data.length]);
    }
    this.raw(Array.from(data));
  }

  bytesField(data: Uint8Array, tag: number): void {
    this.head(13, tag);
    this.head(0, 0);
    this.int(data.length, 0);
    this.raw(Array.from(data));
  }

  struct(fn: (writer: JceWriter) => void, tag: number): void {
    this.head(10, tag);
    fn(this);
    this.head(11, 0);
  }

  emptyList(tag: number): void {
    this.head(9, tag);
    this.int(0, 0);
  }

  out(): Uint8Array {
    return Uint8Array.from(this.bytes);
  }
}

type JceValue = number | bigint | string | Uint8Array | Map<unknown, unknown> | JceValue[] | null;

class JceReader {
  private pos = 0;
  private readonly view: DataView;

  constructor(private readonly data: Uint8Array) {
    this.view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  }

  private remaining(): number {
    return this.data.length - this.pos;
  }

  private get(length: number): Uint8Array {
    if (this.pos + length > this.data.length) throw new Error('jce eof');
    const out = this.data.subarray(this.pos, this.pos + length);
    this.pos += length;
    return out;
  }

  private u8(): number {
    return this.get(1)[0] ?? 0;
  }

  private head(): [number, number] {
    const b = this.u8();
    const type = b & 15;
    let tag = (b & 240) >> 4;
    if (tag === 15) tag = this.u8();
    return [type, tag];
  }

  private value(type: number): JceValue {
    switch (type) {
      case 0: return this.view.getInt8(this.bump(1));
      case 1: return this.view.getInt16(this.bump(2));
      case 2: return this.view.getInt32(this.bump(4));
      case 3: return this.view.getBigInt64(this.bump(8));
      case 4: return this.view.getFloat32(this.bump(4));
      case 5: return this.view.getFloat64(this.bump(8));
      case 6: {
        const len = this.u8();
        return new TextDecoder().decode(this.get(len));
      }
      case 7: {
        const len = this.view.getInt32(this.bump(4));
        return new TextDecoder().decode(this.get(len));
      }
      case 8: {
        const size = Number(this.numberValue());
        const map = new Map<unknown, unknown>();
        for (let i = 0; i < size; i++) map.set(this.fieldValue(), this.fieldValue());
        return map;
      }
      case 9: {
        const size = Number(this.numberValue());
        const list: JceValue[] = [];
        for (let i = 0; i < size; i++) list.push(this.fieldValue());
        return list;
      }
      case 10: return this.struct();
      case 11: return null;
      case 12: return 0;
      case 13: {
        this.head();
        const len = Number(this.numberValue());
        return this.get(len).slice();
      }
      default: throw new Error(`jce type ${String(type)}`);
    }
  }

  private bump(length: number): number {
    const at = this.pos;
    this.pos += length;
    if (this.pos > this.data.length) throw new Error('jce eof');
    return at;
  }

  private fieldValue(): JceValue {
    const [type] = this.head();
    return this.value(type);
  }

  private numberValue(): JceValue {
    const value = this.fieldValue();
    if (typeof value === 'bigint') return Number(value);
    if (typeof value === 'number') return value;
    return 0;
  }

  struct(): Map<number, JceValue> {
    const map = new Map<number, JceValue>();
    while (this.remaining() > 0) {
      const [type, tag] = this.head();
      if (type === 11) break;
      map.set(tag, this.value(type));
    }
    return map;
  }
}

function concatBytes(...parts: Uint8Array[]): Uint8Array<ArrayBuffer> {
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(new ArrayBuffer(total));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function i16be(value: number): Uint8Array {
  return Uint8Array.from([(value >>> 8) & 0xff, value & 0xff]);
}

function i32be(value: number): Uint8Array {
  return Uint8Array.from([(value >>> 24) & 0xff, (value >>> 16) & 0xff, (value >>> 8) & 0xff, value & 0xff]);
}

function i64be(value: number): Uint8Array {
  const v = BigInt(value) & 0xffffffffffffffffn;
  const out = new Uint8Array(8);
  for (let i = 7; i >= 0; i--) out[i] = Number((v >> BigInt((7 - i) * 8)) & 0xffn);
  return out;
}

function writeQua(writer: JceWriter, guid: string): void {
  writer.string(JCE_VER_NAME, 0);
  writer.string(String(JCE_VER_CODE), 1);
  writer.int(1080, 2);
  writer.int(2400, 3);
  writer.int(3, 4);
  writer.string('12', 5);
  writer.int(1, 6);
  writer.int(1, 7);
  writer.int(420, 8);
  writer.string(JCE_CHAN_ID, 9);
  for (let i = 10; i <= 14; i++) writer.string('', i);
  writer.struct((ww) => {
    ww.int(0, 0);
    ww.byte(0, 1);
    ww.string('', 2);
  }, 15);
  writer.string('', 16);
  writer.string('', 17);
  writer.string('', 18);
  writer.struct((ww) => {
    ww.int(0, 0);
    ww.float(0, 1);
    ww.float(0, 2);
    ww.double(0, 3);
  }, 19);
  writer.string(guid.slice(0, 16), 20);
  writer.string('Pixel 6', 21);
  writer.int(1, 22);
  for (let i = 23; i <= 26; i++) writer.int(0, i);
  writer.string('', 27);
  writer.string('', 28);
  writer.string(guid, 29);
}

export function wrapJcePacket(cmd: number, body: Uint8Array, reqid: number, guid: string): Uint8Array<ArrayBuffer> {
  const writer = new JceWriter();
  writer.struct((ww) => {
    ww.int(reqid, 0);
    ww.int(cmd, 1);
    ww.struct((qua) => { writeQua(qua, guid); }, 2);
    ww.string(JCE_APP_ID, 3);
    ww.string(guid, 4);
    ww.emptyList(5);
    ww.struct(() => undefined, 6);
    ww.emptyList(7);
    ww.int(0, 8);
    ww.int(0, 9);
    ww.int(0, 10);
  }, 0);
  writer.bytesField(body, 1);
  const reqcmd = writer.out();

  const inner = concatBytes(
    Uint8Array.of(38),
    i32be(reqcmd.length + 17),
    Uint8Array.of(1),
    new Uint8Array(10),
    reqcmd,
    Uint8Array.of(40),
  );
  const comp = gzipSync(inner);

  const guidBytes = new TextEncoder().encode(guid.slice(0, 32));
  const paddedGuid = new Uint8Array(32);
  paddedGuid.set(guidBytes);

  const out = concatBytes(
    Uint8Array.of(19),
    i32be(0),
    i16be(2),
    i16be(65281),
    i16be(cmd),
    i16be(0),
    i64be(reqid),
    i32be(531),
    i32be(JCE_QMF_APP_ID),
    i64be(JCE_BIZ_ID),
    paddedGuid,
    Uint8Array.of(JCE_QMF_PLATFORM & 0xff),
    i32be(JCE_VER_CODE),
    new Uint8Array(6),
    Uint8Array.of(0),
    i16be(0),
    i16be(0),
    i32be(inner.length),
    comp,
    Uint8Array.of(3),
  );
  new DataView(out.buffer).setInt32(1, out.length);
  return out;
}

export function unwrapJcePacket(data: Uint8Array): Uint8Array {
  if (data[0] !== 0x13 || data.length < 90) throw new IptvResolveError('bad jce response');
  const flags = new DataView(data.buffer, data.byteOffset, data.byteLength).getInt32(21);
  let payload = data.subarray(89, data.length - 1);
  if (flags & 2) payload = gunzipSync(payload);
  if (payload[0] !== 0x26 || payload[payload.length - 1] !== 0x28) throw new IptvResolveError('bad jce payload');
  const fields = new JceReader(payload.subarray(16, payload.length - 1)).struct();
  const body = fields.get(1);
  return body instanceof Uint8Array ? body : new Uint8Array();
}

function sessionGuid(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

let cachedGuid: string | undefined;
function packetGuid(): string {
  cachedGuid ??= sessionGuid();
  return cachedGuid;
}

export function buildTimeshiftBody(pid: string, sid: string, start: number, end: number, stream: string): Uint8Array {
  const body = new JceWriter();
  body.string(pid, 0);
  body.string(sid, 1);
  body.long(start, 2);
  body.long(end, 3);
  body.string(stream, 4);
  return body.out();
}

export async function jceTimeshiftUrl(pid: string, sid: string, start: number, end: number, stream: string): Promise<string> {
  const reqid = Date.now() & 0x7fffffff;
  const packet = wrapJcePacket(JCE_CMD_TIMESHIFT, buildTimeshiftBody(pid, sid, start, end, stream), reqid, packetGuid());
  let response: Response;
  try {
    response = await desktopAwareFetch(JCE_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/octet-stream' },
      body: packet,
    });
  } catch (error) {
    throw new IptvResolveError('timeshift request failed', error);
  }
  if (!response.ok) throw new IptvResolveError(`timeshift request failed: HTTP ${String(response.status)}`);
  const raw = new Uint8Array(await response.arrayBuffer());
  const respBody = unwrapJcePacket(raw);
  if (respBody.length === 0) throw new IptvResolveError('bad response');
  const fields = new JceReader(respBody).struct();
  const err = fields.get(0) ?? 0;
  if (err !== 0) {
    const message = fields.get(1);
    const code = typeof err === 'number' || typeof err === 'bigint' ? String(err) : 'unknown';
    throw new IptvResolveError(`errCode=${code}${typeof message === 'string' ? ` ${message}` : ''}`);
  }
  const url = fields.get(2);
  if (typeof url !== 'string' || !url) throw new IptvResolveError('empty m3u8');
  if (url.includes(JCE_DEAD_HOST)) throw new JceDeadHostError('dead cdn host');
  return url;
}

export { JCE_WINDOW_SECONDS, JCE_MIN_INTERVAL_MS };
