// Port of the bkliveinfo `cKey` signing scheme from ysp-live.py (QQ video TEA
// packet crypto). Produces the `cKey`/`guid`/`fntick`/`flowid` credentials the
// bkliveinfo endpoint expects; no server round-trip is involved.

const CK_PLATFORM = 4330403;
const CK_APPVER = 'V8.22.1035.3031';
const CK_TEA = fromHex('59b2f7cf725ef43c34fdd7c123411ed3');
const CK_GTEA = fromHex('110DBEC10C23E7D2E56A1CAD6914EF1B');
const CK_XOR = Uint8Array.from([132, 46, 237, 8, 240, 102, 230, 234, 72, 180, 202, 169, 145, 237, 111, 243]);
const CK_GXOR = Uint8Array.from([179, 201, 83, 160, 105, 19, 173, 77]);

const textEncoder = new TextEncoder();

export interface IptvCkeyToken {
  cKey: string;
  guid: string;
  ts: number;
  flowId: string;
}

export interface IptvCkeyOptions {
  now?: number;
  randomBytes?: (length: number) => Uint8Array;
}

export function makeIptvCkey(channelId: string, options: IptvCkeyOptions = {}): IptvCkeyToken {
  const randomBytes = options.randomBytes ?? defaultRandom;
  const ts = Math.floor((options.now ?? Date.now()) / 1000);
  const guid = toHex(randomBytes(16));
  const guard = ckGuard(ts, guid, randomBytes);
  const uid = toHex(randomBytes(4)).toUpperCase();

  const body = concatBytes(
    fromHex('0000004200000004000004d2'),
    be32(CK_PLATFORM),
    be32(0),
    be32(ts),
    lp('dcgh'),
    lp('_zj1A5Gh6QYcxWjIUGos2w=='),
    lp(CK_APPVER),
    lp(channelId),
    lp(guid),
    be32(1),
    be32(1),
    lp(uid),
    lp('nil'),
    lp('57eab0c4-2c58-44c6-8ae9-dd2757525dc5'),
    lp('nil'),
    lp('v0.1.000'),
    lp('com.cctv.yangshipin.app.iphone'),
    lp(String(CK_PLATFORM)),
    lp('ex_json_bus'),
    lp('ex_json_vs'),
    lp(guard),
  );

  const packet = concatBytes(be16(body.length), body);
  // The checksum occupies bytes 18..21 of the packet (zeroed at this point).
  packet.set(be32(cksum(packet)), 18);
  const tail = be32(cksum(packet));
  const encrypted = teaPacket(packet, CK_TEA, randomBytes);
  const signed = concatBytes(encrypted, tail);
  for (const [index, byte] of signed.entries()) {
    signed[index] = byte ^ (CK_XOR[index & 15] ?? 0);
  }
  return {
    cKey: `--01${base64UrlEncode(signed)}`,
    guid,
    ts,
    flowId: `${randomUuidHex()}_${String(CK_PLATFORM)}`,
  };
}

function ckGuard(ts: number, guid: string, randomBytes: (length: number) => Uint8Array): string {
  const tail = (value: string) => (value.length >= 5 ? value.slice(-5) : '');
  const body = concatBytes(be32(ts), lp(tail(guid)), lp(tail('null')), lp(tail('null')), lp('-1'));
  const plain = lp(body);
  const encrypted = concatBytes(teaPacket(plain, CK_GTEA, randomBytes), be32(cksum(plain)));
  for (const [index, byte] of encrypted.entries()) {
    encrypted[index] = byte ^ (CK_GXOR[index & 7] ?? 0);
  }
  return toHex(encrypted).toUpperCase();
}

function teaPacket(data: Uint8Array, key: Uint8Array, randomBytes: (length: number) => Uint8Array = defaultRandom): Uint8Array {
  const pad = (8 - ((data.length + 10) % 8)) % 8;
  const head = randomBytes(1 + pad + 2);
  head[0] = ((head[0] ?? 0) & 248) | pad;
  const plain = concatBytes(head, data, new Uint8Array(7));
  const out = new Uint8Array(plain.length);
  let previousPlain: Uint8Array = new Uint8Array(8);
  let previousCipher: Uint8Array = new Uint8Array(8);
  for (let offset = 0; offset < plain.length; offset += 8) {
    const mixed = xor(plain.subarray(offset, offset + 8), previousCipher);
    const cipher = xor(teaBlock(mixed, key), previousPlain);
    out.set(cipher, offset);
    previousPlain = mixed;
    previousCipher = cipher;
  }
  return out;
}

function teaBlock(block: Uint8Array, key: Uint8Array): Uint8Array {
  const input = new DataView(block.buffer, block.byteOffset, block.byteLength);
  const keyView = new DataView(key.buffer, key.byteOffset, key.byteLength);
  let y = input.getUint32(0);
  let z = input.getUint32(4);
  const k0 = keyView.getUint32(0);
  const k1 = keyView.getUint32(4);
  const k2 = keyView.getUint32(8);
  const k3 = keyView.getUint32(12);
  let sum = 0;
  for (let round = 0; round < 16; round += 1) {
    sum = (sum + 2654435769) >>> 0;
    y = (y + (((((z << 4) >>> 0) + k0) >>> 0 ^ (z + sum) >>> 0 ^ (((z >>> 5) + k1) >>> 0)) >>> 0)) >>> 0;
    z = (z + (((((y << 4) >>> 0) + k2) >>> 0 ^ (y + sum) >>> 0 ^ (((y >>> 5) + k3) >>> 0)) >>> 0)) >>> 0;
  }
  const out = new Uint8Array(8);
  const view = new DataView(out.buffer);
  view.setUint32(0, y);
  view.setUint32(4, z);
  return out;
}

function cksum(buffer: Uint8Array): number {
  let value = 0;
  for (const byte of buffer) value = (131 * value + byte) & 2147483647;
  return value;
}

function lp(value: string | Uint8Array): Uint8Array {
  const data = typeof value === 'string' ? textEncoder.encode(value) : value;
  return concatBytes(be16(data.length), data);
}

function be16(value: number): Uint8Array {
  const out = new Uint8Array(2);
  new DataView(out.buffer).setUint16(0, value);
  return out;
}

function be32(value: number): Uint8Array {
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, value >>> 0);
  return out;
}

function concatBytes(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((size, part) => size + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function xor(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length);
  for (const [index, byte] of a.entries()) out[index] = byte ^ (b[index] ?? 0);
  return out;
}

function fromHex(value: string): Uint8Array {
  const out = new Uint8Array(value.length / 2);
  for (let index = 0; index < out.length; index += 1) {
    out[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
  }
  return out;
}

function toHex(bytes: Uint8Array): string {
  return [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function base64UrlEncode(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll('+', '_').replaceAll('/', '-').replace(/=+$/, '');
}

function randomUuidHex(): string {
  // crypto.randomUUID is secure-context only; the embedded WebView pages run on
  // a plain http origin where it is undefined.
  return toHex(defaultRandom(16)).toUpperCase();
}

function defaultRandom(length: number): Uint8Array {
  const out = new Uint8Array(length);
  crypto.getRandomValues(out);
  return out;
}
