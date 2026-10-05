import { makeIptvCkey } from './iptvCrypto';

// Deterministic vectors produced by the reference `_ckey` implementation in
// ysp-live.py with os.urandom patched to emit 0x40, 0x41, ... and a fixed
// timestamp. Any deviation in TEA, packet framing, checksums, XOR masks or the
// swapped base64url alphabet fails these assertions.
function sequentialRandom(start: number): (length: number) => Uint8Array {
  let value = start;
  return (length) => {
    const out = new Uint8Array(length);
    for (let index = 0; index < length; index += 1) {
      out[index] = value & 0xff;
      value = (value + 1) & 0xff;
    }
    return out;
  };
}

const FIXED_NOW = 1_791_200_000_000;

it('reproduces the reference cKey for channel 2024078201', () => {
  const token = makeIptvCkey('2024078201', { now: FIXED_NOW, randomBytes: sequentialRandom(0x40) });
  expect(token.guid).toBe('404142434445464748494a4b4c4d4e4f');
  expect(token.ts).toBe(1_791_200_000);
  expect(token.flowId.endsWith('_4330403')).toBe(true);
  expect(token.cKey).toBe('--01hJM-_s0rqZtOuR_mC2tI8RNjz-SE1tiUCAhXXUVFyvDCYkXbRres0zK-wbucpNbv-F_ld_tnc3W5Q9zJWq1Abnfe7Gi9JVcxtuJvtN2OEeM6KUdX3MJkWvPjs3T13kA7F8U9Bs9rH7JvWK0zikqdBqnfvnR5y1W-FIXV-1cG8fwt_PyiYlv0rIX7eZpdNFQfSw_wbVrHvsRlldX34bwYVupNiys9iHZC0XXAq4V4uQWTYQWEzXVLvs2ojb3jWMIJZbfNkHfrKhYY19XuMRyOo3ott2tT53nEiLg1J4_yutpwdAUmWQLY1leD6C59NthghT7I575Vc8CHOZLgORQuA45loVqLsGa_aZRGNkDjioyWk6bmJ7WpXxDQmQ9hy9qRI1yOGwcurICbnl_fdGFNYo5OUJTjT__HLtP-vKrCbYC5p0wW9yoeCt2A0-mVo_RDOpVZl3HM3SMfE5bz4IHQdvcNqH4');
});

it('reproduces the reference cKey for a second channel', () => {
  const token = makeIptvCkey('2024052703', { now: FIXED_NOW, randomBytes: sequentialRandom(0x40) });
  expect(token.cKey).toBe('--01hJM-_s0rqZtOuR_mC2tI8RNjz-SE1tiUMURP_Otr1nrlV_iNNWt3NNRgqywq6BGq1nylhXNkdV2PVOepaVgv8asiw-w8S9DRZDigu5qr5guP1bgiGpt-g6VHR6vcaz5cSgjagBwwUpA0cLyrbbFp1scSffb_wV6xp7tvdc5kJFpJ2FLSiEjmCtbKMKUoXivHE1WOvCYatmqYpUILVkA2w0ovMfoy5C4TGGldk19aX60KkoVE7Zpgsit3wwF7w1iHPeQWxLlEhEpFsRMnbE9iN5KyArNCIkHsRQB4TdJmu2ErLca6dknjHqn0ARsJLlrmk8wyiMoVGphiGNTYnOdHgbPovxmZ9ILD66JnRMOX-qsj9IDfAjzawFi7Is0jk4Wh-bWLl6c8ZpaEs6Ejq5Kl1zeRJIn1Ui8MBuXZC2oTv_sMoZ9cBGTW7bHTXhyIR2-K7yGRB15Zdg_78QgcQV66-5kg2fQ');
});

it('produces url-safe tokens without padding', () => {
  const token = makeIptvCkey('2024078201');
  expect(token.cKey.startsWith('--01')).toBe(true);
  expect(token.cKey).toMatch(/^[-\w]+$/);
  expect(token.guid).toMatch(/^[0-9a-f]{32}$/);
  expect(token.flowId).toMatch(/^[0-9A-F]{32}_4330403$/);
});
