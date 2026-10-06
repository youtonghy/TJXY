// Pure-TS port of the device-protocol crypto from ysp-live.py. The app-shell
// WebViews can run on a plain http origin where `crypto.subtle` is absent, so
// — like the upstream script — the primitives are implemented directly:
// AES-256 block + GCM, RSA-OAEP-SHA256, SHA-1/SHA-256/MD5, plus the
// X-Fingerprint identity derivation and the exact form/JSON encoders the
// endpoints expect.

export const IPTV_DEVICE_AK = '9f5c54c4ed0e50109b800f7e28fec205';

const RSA_PUBLIC_KEY_B64 =
  'MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAkKeLy4ywWLSnBkwRyqYgF3HMIj05V5uuh5HjyEsZOWnu1NHu3jPQv3sr32wwQNYv5qapsNXmNgLUDHtgHZxqPQAYXltjSRc0qhcD286t62wOIHId8zXS3s1Jy4rgU4qjQWzI9rp/1sE0pMsmwTaJa4zuJ5iz8VwF8Av5oJ1k+HxY+/HLnjNlW1hmWLpuDYmkZYuAoTHa1VGeHQh9FEKI8ZcL3GTQphShUoC+Kg3P1hGUVTtCYapmzPS5lkAdwebuzwvTCfGiTErYZCnPBUSeV7BVlgjtLYIi29KvF0a8FHsJMfe/UdHcyW/RihsIYOtDQcRRpFGXyPXbVrzFJse24QIDAQAB';

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export function utf8(value: string): Uint8Array<ArrayBuffer> {
  return encoder.encode(value);
}

export function bytesToHex(bytes: Uint8Array): string {
  return [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

export function concatBytes(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((size, part) => size + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function base64ToBytes(value: string): Uint8Array {
  const binary = atob(value);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i);
  return out;
}

// ---------------------------------------------------------------------------
// SHA-256 / SHA-1 / MD5 (bytes -> digest bytes)

function rightRotate(word: number, bits: number): number {
  return ((word >>> bits) | (word << (32 - bits))) >>> 0;
}

const SHA256_K = [
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

// SHA-1/SHA-256 pad big-endian; MD5 pads little-endian (handled separately).
function padMdStyle(data: Uint8Array): Uint8Array {
  const bitLength = data.length * 8;
  const padded = new Uint8Array(data.length + ((56 + 64 - (data.length % 64)) % 64) + 8);
  padded.set(data);
  padded[data.length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 8, Math.floor(bitLength / 2 ** 32));
  view.setUint32(padded.length - 4, bitLength >>> 0);
  return padded;
}

function sha256Bytes(data: Uint8Array): Uint8Array {
  const padded = padMdStyle(data);
  const view = new DataView(padded.buffer);
  const state = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
  const w = new Uint32Array(64);
  for (let offset = 0; offset < padded.length; offset += 64) {
    for (let i = 0; i < 16; i += 1) w[i] = view.getUint32(offset + i * 4);
    for (let i = 16; i < 64; i += 1) {
      const s0 = rightRotate(w[i - 15] ?? 0, 7) ^ rightRotate(w[i - 15] ?? 0, 18) ^ ((w[i - 15] ?? 0) >>> 3);
      const s1 = rightRotate(w[i - 2] ?? 0, 17) ^ rightRotate(w[i - 2] ?? 0, 19) ^ ((w[i - 2] ?? 0) >>> 10);
      w[i] = ((w[i - 16] ?? 0) + s0 + (w[i - 7] ?? 0) + s1) >>> 0;
    }
    let a = state[0] ?? 0;
    let b = state[1] ?? 0;
    let c = state[2] ?? 0;
    let d = state[3] ?? 0;
    let e = state[4] ?? 0;
    let f = state[5] ?? 0;
    let g = state[6] ?? 0;
    let h = state[7] ?? 0;
    for (let i = 0; i < 64; i += 1) {
      const s1 = rightRotate(e, 6) ^ rightRotate(e, 11) ^ rightRotate(e, 25);
      const ch = (e & f) ^ (~e & g);
      const t1 = (h + s1 + ch + (SHA256_K[i] ?? 0) + (w[i] ?? 0)) >>> 0;
      const s0 = rightRotate(a, 2) ^ rightRotate(a, 13) ^ rightRotate(a, 22);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const t2 = (s0 + maj) >>> 0;
      h = g;
      g = f;
      f = e;
      e = (d + t1) >>> 0;
      d = c;
      c = b;
      b = a;
      a = (t1 + t2) >>> 0;
    }
    const deltas = [a, b, c, d, e, f, g, h];
    for (const [i, v] of deltas.entries()) state[i] = ((state[i] ?? 0) + v) >>> 0;
  }
  const out = new Uint8Array(32);
  const outView = new DataView(out.buffer);
  for (const [i, v] of state.entries()) outView.setUint32(i * 4, v);
  return out;
}

function sha1Bytes(data: Uint8Array): Uint8Array {
  const padded = padMdStyle(data);
  const view = new DataView(padded.buffer);
  let h0 = 0x67452301;
  let h1 = 0xefcdab89;
  let h2 = 0x98badcfe;
  let h3 = 0x10325476;
  let h4 = 0xc3d2e1f0;
  const w = new Uint32Array(80);
  for (let offset = 0; offset < padded.length; offset += 64) {
    for (let i = 0; i < 16; i += 1) w[i] = view.getUint32(offset + i * 4);
    for (let i = 16; i < 80; i += 1) {
      const mixed = (w[i - 3] ?? 0) ^ (w[i - 8] ?? 0) ^ (w[i - 14] ?? 0) ^ (w[i - 16] ?? 0);
      w[i] = ((mixed << 1) | (mixed >>> 31)) >>> 0;
    }
    let a = h0;
    let b = h1;
    let c = h2;
    let d = h3;
    let e = h4;
    for (let i = 0; i < 80; i += 1) {
      const round = Math.floor(i / 20);
      const f =
        round === 0 ? (b & c) | (~b & d)
        : round === 1 ? b ^ c ^ d
        : round === 2 ? (b & c) | (b & d) | (c & d)
        : b ^ c ^ d;
      const k = round === 0 ? 0x5a827999 : round === 1 ? 0x6ed9eba1 : round === 2 ? 0x8f1bbcdc : 0xca62c1d6;
      const t = (((a << 5) | (a >>> 27)) >>> 0) + f + e + k + (w[i] ?? 0);
      e = d;
      d = c;
      c = ((b << 30) | (b >>> 2)) >>> 0;
      b = a;
      a = t >>> 0;
    }
    h0 = (h0 + a) >>> 0;
    h1 = (h1 + b) >>> 0;
    h2 = (h2 + c) >>> 0;
    h3 = (h3 + d) >>> 0;
    h4 = (h4 + e) >>> 0;
  }
  const out = new Uint8Array(20);
  const outView = new DataView(out.buffer);
  for (const [i, v] of [h0, h1, h2, h3, h4].entries()) outView.setUint32(i * 4, v);
  return out;
}

const MD5_S = [
  7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
  5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
  4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
  6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];
const MD5_K = Array.from({ length: 64 }, (_, i) => Math.floor(Math.abs(Math.sin(i + 1)) * 2 ** 32) >>> 0);

function md5Bytes(data: Uint8Array): Uint8Array {
  const bitLength = data.length * 8;
  const padded = new Uint8Array(data.length + ((56 + 64 - (data.length % 64)) % 64) + 8);
  padded.set(data);
  padded[data.length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 8, bitLength >>> 0, true);
  view.setUint32(padded.length - 4, Math.floor(bitLength / 2 ** 32), true);
  let a0 = 0x67452301;
  let b0 = 0xefcdab89;
  let c0 = 0x98badcfe;
  let d0 = 0x10325476;
  for (let offset = 0; offset < padded.length; offset += 64) {
    const m: number[] = [];
    for (let i = 0; i < 16; i += 1) m.push(view.getUint32(offset + i * 4, true));
    let a = a0;
    let b = b0;
    let c = c0;
    let d = d0;
    for (let i = 0; i < 64; i += 1) {
      const round = Math.floor(i / 16);
      const f =
        round === 0 ? (b & c) | (~b & d)
        : round === 1 ? (d & b) | (~d & c)
        : round === 2 ? b ^ c ^ d
        : c ^ (b | ~d);
      const g = round === 0 ? i : round === 1 ? (5 * i + 1) % 16 : round === 2 ? (3 * i + 5) % 16 : (7 * i) % 16;
      const t = (f + a + (MD5_K[i] ?? 0) + (m[g] ?? 0)) >>> 0;
      const rot = MD5_S[i] ?? 0;
      a = d;
      d = c;
      c = b;
      b = (b + (((t << rot) | (t >>> (32 - rot))) >>> 0)) >>> 0;
    }
    a0 = (a0 + a) >>> 0;
    b0 = (b0 + b) >>> 0;
    c0 = (c0 + c) >>> 0;
    d0 = (d0 + d) >>> 0;
  }
  const out = new Uint8Array(16);
  const outView = new DataView(out.buffer);
  for (const [i, v] of [a0, b0, c0, d0].entries()) outView.setUint32(i * 4, v, true);
  return out;
}

export function sha256Hex(value: string): string {
  return bytesToHex(sha256Bytes(utf8(value)));
}

export function sha1Upper(value: string): string {
  return bytesToHex(sha1Bytes(utf8(value))).toUpperCase();
}

export function md5Hex(value: string): string {
  return bytesToHex(md5Bytes(utf8(value)));
}

// ---------------------------------------------------------------------------
// AES-256 (encrypt only — GCM never decrypts single blocks)

const AES_SBOX = Uint8Array.from([
  99, 124, 119, 123, 242, 107, 111, 197, 48, 1, 103, 43, 254, 215, 171, 118,
  202, 130, 201, 125, 250, 89, 71, 240, 173, 212, 162, 175, 156, 164, 114, 192,
  183, 253, 147, 38, 54, 63, 247, 204, 52, 165, 229, 241, 113, 216, 49, 21, 4,
  199, 35, 195, 24, 150, 5, 154, 7, 18, 128, 226, 235, 39, 178, 117, 9, 131, 44,
  26, 27, 110, 90, 160, 82, 59, 214, 179, 41, 227, 47, 132, 83, 209, 0, 237, 32,
  252, 177, 91, 106, 203, 190, 57, 74, 76, 88, 207, 208, 239, 170, 251, 67, 77,
  51, 133, 69, 249, 2, 127, 80, 60, 159, 168, 81, 163, 64, 143, 146, 157, 56,
  245, 188, 182, 218, 33, 16, 255, 243, 210, 205, 12, 19, 236, 95, 151, 68, 23,
  196, 167, 126, 61, 100, 93, 25, 115, 96, 129, 79, 220, 34, 42, 144, 136, 70,
  238, 184, 20, 222, 94, 11, 219, 224, 50, 58, 10, 73, 6, 36, 92, 194, 211, 172,
  98, 145, 149, 228, 121, 231, 200, 55, 109, 141, 213, 78, 169, 108, 86, 244,
  234, 101, 122, 174, 8, 186, 120, 37, 46, 28, 166, 180, 198, 232, 221, 116, 31,
  75, 189, 139, 138, 112, 62, 181, 102, 72, 3, 246, 14, 97, 53, 87, 185, 134,
  193, 29, 158, 225, 248, 152, 17, 105, 217, 142, 148, 155, 30, 135, 233, 206,
  85, 40, 223, 140, 161, 137, 13, 191, 230, 66, 104, 65, 153, 45, 15, 176, 84,
  187, 22,
]);
const AES_RCON = [1, 2, 4, 8, 16, 32, 64, 128, 27, 54];

function xtime(a: number): number {
  return a & 128 ? ((a << 1) ^ 283) & 255 : (a << 1) & 255;
}

function aes256KeySchedule(key: Uint8Array): Uint8Array[] {
  const w: number[] = [];
  const view = new DataView(key.buffer, key.byteOffset, key.byteLength);
  for (let i = 0; i < 8; i += 1) w.push(view.getUint32(i * 4));
  for (let i = 8; i < 60; i += 1) {
    let temp = w[i - 1] ?? 0;
    if (i % 8 === 0) {
      temp = ((AES_SBOX[(temp >>> 16) & 255] ?? 0) << 24) | ((AES_SBOX[(temp >>> 8) & 255] ?? 0) << 16)
        | ((AES_SBOX[temp & 255] ?? 0) << 8) | (AES_SBOX[(temp >>> 24) & 255] ?? 0);
      temp ^= (AES_RCON[Math.floor(i / 8) - 1] ?? 0) << 24;
    } else if (i % 8 === 4) {
      temp = ((AES_SBOX[(temp >>> 24) & 255] ?? 0) << 24) | ((AES_SBOX[(temp >>> 16) & 255] ?? 0) << 16)
        | ((AES_SBOX[(temp >>> 8) & 255] ?? 0) << 8) | (AES_SBOX[temp & 255] ?? 0);
    }
    w.push(((w[i - 8] ?? 0) ^ temp) >>> 0);
  }
  const roundKeys: Uint8Array[] = [];
  for (let i = 0; i < 15; i += 1) {
    const rk = new Uint8Array(16);
    const rkView = new DataView(rk.buffer);
    for (let j = 0; j < 4; j += 1) rkView.setUint32(j * 4, w[i * 4 + j] ?? 0);
    roundKeys.push(rk);
  }
  return roundKeys;
}

function aes256EncryptBlock(key: Uint8Array, block: Uint8Array): Uint8Array {
  const rk = aes256KeySchedule(key);
  const s = Uint8Array.from(block);
  const addRoundKey = (roundKey: Uint8Array) => {
    for (let i = 0; i < 16; i += 1) s[i] = (s[i] ?? 0) ^ (roundKey[i] ?? 0);
  };
  const subBytes = () => {
    for (let i = 0; i < 16; i += 1) s[i] = AES_SBOX[s[i] ?? 0] ?? 0;
  };
  const shiftRows = () => {
    [s[1], s[5], s[9], s[13]] = [s[5] ?? 0, s[9] ?? 0, s[13] ?? 0, s[1] ?? 0];
    [s[2], s[6], s[10], s[14]] = [s[10] ?? 0, s[14] ?? 0, s[2] ?? 0, s[6] ?? 0];
    [s[3], s[7], s[11], s[15]] = [s[15] ?? 0, s[3] ?? 0, s[7] ?? 0, s[11] ?? 0];
  };
  const mixColumns = () => {
    for (let c = 0; c < 4; c += 1) {
      const a0 = s[4 * c] ?? 0;
      const a1 = s[4 * c + 1] ?? 0;
      const a2 = s[4 * c + 2] ?? 0;
      const a3 = s[4 * c + 3] ?? 0;
      s[4 * c] = xtime(a0) ^ (xtime(a1) ^ a1) ^ a2 ^ a3;
      s[4 * c + 1] = a0 ^ xtime(a1) ^ (xtime(a2) ^ a2) ^ a3;
      s[4 * c + 2] = a0 ^ a1 ^ xtime(a2) ^ (xtime(a3) ^ a3);
      s[4 * c + 3] = xtime(a0) ^ a0 ^ a1 ^ a2 ^ xtime(a3);
    }
  };
  addRoundKey(rk[0] ?? new Uint8Array(16));
  for (let round = 1; round < 14; round += 1) {
    subBytes();
    shiftRows();
    mixColumns();
    addRoundKey(rk[round] ?? new Uint8Array(16));
  }
  subBytes();
  shiftRows();
  addRoundKey(rk[14] ?? new Uint8Array(16));
  return s;
}

// ---------------------------------------------------------------------------
// AES-GCM over BigInt GF(2^128) — 1:1 with the upstream Python helpers.

const GF_R = 299076299051606071403356588563077529600n;
const GF_MSB = 170141183460469231731687303715884105728n;
const GF_MASK = 340282366920938463463374607431768211455n;

function gfMult(xIn: bigint, yIn: bigint): bigint {
  let x = xIn;
  let v = yIn;
  let z = 0n;
  for (let i = 0; i < 128; i += 1) {
    if (x & GF_MSB) z ^= v;
    v = v & 1n ? (v >> 1n) ^ GF_R : v >> 1n;
    x = (x << 1n) & GF_MASK;
  }
  return z;
}

function bytesToBigInt(bytes: Uint8Array): bigint {
  let value = 0n;
  for (const byte of bytes) value = (value << 8n) | BigInt(byte);
  return value;
}

function bigIntToBytes(value: bigint, length: number): Uint8Array {
  const out = new Uint8Array(length);
  let remaining = value;
  for (let i = length - 1; i >= 0; i -= 1) {
    out[i] = Number(remaining & 255n);
    remaining >>= 8n;
  }
  return out;
}

function ghash(h: bigint, aad: Uint8Array, ct: Uint8Array): bigint {
  let x = 0n;
  const blocks = function* (data: Uint8Array) {
    for (let i = 0; i < data.length; i += 16) {
      const block = new Uint8Array(16);
      block.set(data.slice(i, i + 16));
      yield bytesToBigInt(block);
    }
  };
  for (const b of blocks(aad)) x = gfMult(x ^ b, h);
  for (const b of blocks(ct)) x = gfMult(x ^ b, h);
  const lens = (BigInt(aad.length) * 8n) << 64n | (BigInt(ct.length) * 8n);
  x = gfMult(x ^ lens, h);
  return x;
}

function inc32(block: Uint8Array): Uint8Array {
  const out = Uint8Array.from(block);
  const view = new DataView(out.buffer);
  view.setUint32(12, (view.getUint32(12) + 1) >>> 0);
  return out;
}

function gctr(key: Uint8Array, icb: Uint8Array, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(data.length);
  let cb: Uint8Array = Uint8Array.from(icb);
  for (let i = 0; i < data.length; i += 16) {
    const ks = aes256EncryptBlock(key, cb);
    const chunk = data.slice(i, i + 16);
    for (const [j, byte] of chunk.entries()) out[i + j] = byte ^ (ks[j] ?? 0);
    cb = inc32(cb);
  }
  return out;
}

function aesGcmEncryptRaw(key: Uint8Array, nonce: Uint8Array, plaintext: Uint8Array, aad: Uint8Array = new Uint8Array(0)): Uint8Array {
  const h = bytesToBigInt(aes256EncryptBlock(key, new Uint8Array(16)));
  const j0 = concatBytes(nonce, Uint8Array.from([0, 0, 0, 1]));
  const ct = gctr(key, inc32(j0), plaintext);
  const s = bigIntToBytes(ghash(h, aad, ct), 16);
  const ek = aes256EncryptBlock(key, j0);
  const tag = Uint8Array.from(s.map((byte, i) => byte ^ (ek[i] ?? 0)));
  return concatBytes(ct, tag);
}

function aesGcmDecryptRaw(key: Uint8Array, nonce: Uint8Array, ctAndTag: Uint8Array, aad: Uint8Array = new Uint8Array(0)): Uint8Array {
  if (ctAndTag.length < 16) throw new Error('AES-GCM payload too short');
  const ct = ctAndTag.slice(0, -16);
  const tag = ctAndTag.slice(-16);
  const h = bytesToBigInt(aes256EncryptBlock(key, new Uint8Array(16)));
  const j0 = concatBytes(nonce, Uint8Array.from([0, 0, 0, 1]));
  const s = bigIntToBytes(ghash(h, aad, ct), 16);
  const ek = aes256EncryptBlock(key, j0);
  let mismatch = 0;
  for (const [i, byte] of tag.entries()) mismatch |= byte ^ (ek[i] ?? 0) ^ (s[i] ?? 0);
  if (mismatch !== 0) throw new Error('AES-GCM decrypt failed');
  return gctr(key, inc32(j0), ct);
}

// Keys are utf8 strings zero-padded/truncated to 32 bytes.
function normalizeAesKey(value: string): Uint8Array {
  const raw = utf8(value);
  const out = new Uint8Array(32);
  out.set(raw.slice(0, 32));
  return out;
}

export function aesGcmDecryptB64(value: string, key: string): string {
  const raw = base64ToBytes(value);
  if (raw.length <= 12) throw new Error('AES-GCM payload too short');
  return decoder.decode(aesGcmDecryptRaw(normalizeAesKey(key), raw.slice(0, 12), raw.slice(12)));
}

export function aesGcmEncryptB64(value: string, key: string, randomBytes: (length: number) => Uint8Array): string {
  const nonce = randomBytes(12);
  const encrypted = aesGcmEncryptRaw(normalizeAesKey(key), nonce, utf8(value));
  return bytesToBase64(concatBytes(nonce, encrypted));
}

// ---------------------------------------------------------------------------
// RSA-OAEP-SHA256 (encrypt only) — wraps the device id for cloud register/get.

function mgf1Sha256(seed: Uint8Array, length: number): Uint8Array {
  const out = new Uint8Array(length);
  let counter = 0;
  let written = 0;
  while (written < length) {
    const block = new Uint8Array(seed.length + 4);
    block.set(seed);
    new DataView(block.buffer).setUint32(seed.length, counter);
    const digest = sha256Bytes(block);
    out.set(digest.slice(0, Math.min(32, length - written)), written);
    written += digest.length;
    counter += 1;
  }
  return out;
}

function oaepEncodeSha256(message: Uint8Array, k: number, seed: Uint8Array): Uint8Array {
  const hlen = 32;
  if (message.length > k - 2 * hlen - 2) throw new Error('OAEP message too long');
  const lhash = sha256Bytes(new Uint8Array(0));
  const db = concatBytes(lhash, new Uint8Array(k - message.length - 2 * hlen - 2), Uint8Array.from([1]), message);
  const dbMask = mgf1Sha256(seed, k - hlen - 1);
  const maskedDb = Uint8Array.from(db.map((byte, i) => byte ^ (dbMask[i] ?? 0)));
  const seedMask = mgf1Sha256(maskedDb, hlen);
  const maskedSeed = Uint8Array.from(seed.map((byte, i) => byte ^ (seedMask[i] ?? 0)));
  return concatBytes(new Uint8Array(1), maskedSeed, maskedDb);
}

function derReadLength(der: Uint8Array, pos: number): [number, number] {
  const first = der[pos] ?? 0;
  pos += 1;
  if ((first & 128) === 0) return [first, pos];
  const n = first & 127;
  return [Number(bytesToBigInt(der.slice(pos, pos + n))), pos + n];
}

function derReadSequence(der: Uint8Array, pos: number): [number, Uint8Array][] {
  if (der[pos] !== 48) throw new Error('expected SEQUENCE');
  pos += 1;
  const [length, bodyPos] = derReadLength(der, pos);
  const end = bodyPos + length;
  const items: [number, Uint8Array][] = [];
  let cursor = bodyPos;
  while (cursor < end) {
    const tag = der[cursor] ?? 0;
    cursor += 1;
    const [itemLength, itemPos] = derReadLength(der, cursor);
    items.push([tag, der.slice(itemPos, itemPos + itemLength)]);
    cursor = itemPos + itemLength;
  }
  return items;
}

function parseSpkiRsaPubkey(der: Uint8Array): [bigint, bigint] {
  const outer = derReadSequence(der, 0);
  const bitstring = outer[1]?.[1] ?? new Uint8Array(0);
  const inner = derReadSequence(bitstring.slice(1), 0);
  return [bytesToBigInt(inner[0]?.[1] ?? new Uint8Array(0)), bytesToBigInt(inner[1]?.[1] ?? new Uint8Array(0))];
}

export function rsaOaepSha256Encrypt(der: Uint8Array, message: Uint8Array, seed: Uint8Array): Uint8Array {
  const [n, e] = parseSpkiRsaPubkey(der);
  const k = Math.ceil(n.toString(2).length / 8);
  const em = oaepEncodeSha256(message, k, seed);
  const m = bytesToBigInt(em);
  // m^e mod n by square-and-multiply.
  let result = 1n;
  let base = m % n;
  let exp = e;
  while (exp > 0n) {
    if (exp & 1n) result = (result * base) % n;
    base = (base * base) % n;
    exp >>= 1n;
  }
  return bigIntToBytes(result, k);
}

export function rsaEncryptDeviceId(deviceId: string, randomBytes: (length: number) => Uint8Array): string {
  const der = base64ToBytes(RSA_PUBLIC_KEY_B64);
  const [n] = parseSpkiRsaPubkey(der);
  const k = Math.ceil(n.toString(2).length / 8);
  const chunkSize = k - 2 * 32 - 2;
  const data = utf8(deviceId);
  const parts: Uint8Array[] = [];
  for (let i = 0; i < data.length; i += chunkSize) {
    parts.push(rsaOaepSha256Encrypt(der, data.slice(i, i + chunkSize), randomBytes(32)));
  }
  return bytesToBase64(concatBytes(...parts));
}

// ---------------------------------------------------------------------------
// Identity derivation (X-Uid / X-Fingerprint)

export function javaStringHashcode(value: string): number {
  let h = 0;
  for (const ch of value) h = (h * 31 + (ch.codePointAt(0) ?? 0)) >>> 0;
  return h & 0x80000000 ? h - 0x100000000 : h;
}

export function javaUuidFromHashes(msbHash: number, lsbHash: number): string {
  const msb = BigInt.asUintN(64, BigInt(msbHash));
  const lsb = BigInt.asUintN(64, BigInt(lsbHash));
  return ((msb << 64n) | lsb).toString(16).padStart(32, '0');
}

export function nativeDay0Ms(nowS: number): number {
  return 86400000 * Math.floor((nowS + 28800) / 86400) - 28800000;
}

export function computeFingerprint(xUid: string, nowMs: number): [string, number, number] {
  const day0Ms = nativeDay0Ms(Math.floor(nowMs / 1000));
  const first = sha256Hex(`${IPTV_DEVICE_AK}${xUid}${String(nowMs)}${String(day0Ms)}`);
  return [sha256Hex(first), nowMs, day0Ms];
}

// ---------------------------------------------------------------------------
// Wire encoders — the endpoints are picky about exact bytes.

export function formUrlencodeValue(s: string): string {
  const out: string[] = [];
  for (const byte of utf8(s)) {
    const ch = String.fromCharCode(byte);
    if (/^[\w*.-]$/.test(ch)) out.push(ch);
    else if (byte === 32) out.push('+');
    else out.push(`%${byte.toString(16).padStart(2, '0').toUpperCase()}`);
  }
  return out.join('');
}

export function formEncode(pairs: [string, string][]): Uint8Array<ArrayBuffer> {
  return utf8(pairs.map(([k, v]) => `${formUrlencodeValue(k)}=${formUrlencodeValue(v)}`).join('&'));
}

// Upstream dumps request bodies with sort_keys + compact separators and, for
// app/start, escapes forward slashes. Reproduce the canonical form byte-exact.
export function compactJsonBytes(value: unknown, escapeForwardSlashes = false): Uint8Array<ArrayBuffer> {
  const serialize = (v: unknown): string => {
    if (v === null) return 'null';
    if (typeof v === 'number' || typeof v === 'boolean') return JSON.stringify(v);
    if (typeof v === 'string') return JSON.stringify(v);
    if (Array.isArray(v)) return `[${v.map(serialize).join(',')}]`;
    const record = v as Record<string, unknown>;
    return `{${Object.keys(record).sort().map((key) => `${JSON.stringify(key)}:${serialize(record[key])}`).join(',')}}`;
  };
  let text = serialize(value);
  if (escapeForwardSlashes) text = text.replaceAll('/', '\\/');
  return utf8(text);
}

export function buildVdnAppcommon(version: string): string {
  return `{"adid":"","an":"央视频电视投屏助手","ap":"cctv_app_tv","av":${JSON.stringify(version)}}`;
}

export function generateAppRandomStr(randomBytes: (length: number) => Uint8Array): string {
  return `${bytesToHex(randomBytes(4))}-0000-${bytesToHex(randomBytes(2))}-0000-00000000${bytesToHex(randomBytes(2))}`;
}

// Upstream uses uuid.uuid4() for X-Nonce and the page session id.
export function uuid4(randomBytes: (length: number) => Uint8Array): string {
  const bytes = randomBytes(16);
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const hex = bytesToHex(bytes);
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function computeVdnCode(
  appSecret: string,
  randomStr: string | undefined,
  randomBytes: (length: number) => Uint8Array,
): [string, string] {
  const r = randomStr ?? generateAppRandomStr(randomBytes);
  return [md5Hex(`${IPTV_DEVICE_AK}${appSecret}${r}`), r];
}
