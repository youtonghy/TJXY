import { buildTimeshiftBody, unwrapJcePacket, wrapJcePacket } from './iptvJce';

// Golden vectors produced by the upstream ysp-live.py `W`/`_wrap`
// implementation with GUID 'a1b2c3d4e5f607182930405060708090', reqid 12345678,
// pid '600001800', sid '2024075401', window 1791213382..1791213682, 'fhd'.
const GOLDEN_BODY = '0609363030303031383030160a32303234303735343031226ac3bf46326ac3c0724603666864';
const GOLDEN_PACKET =
  '130000015b0002ff0162e000000000000000bc614e000002130000271c000000000000000061316232633364346535663630373138323933303430353036303730383039300100049bf600000000000000000000000000012c1f8b08000000000002ff53636060d4616480032e26863d897e82490fb4d8b88df58cf4ccf58ccc8c0c8dc4d88c0d8c0ccc0d14592c0c39131c98c3980c8d12180b181b19974c63353400ca2c63d8c6708ce11ac333865ffc3c326a0cdcdf0418be09327c1362f825cc2302325a054498c22ce2fe262290689864946c9c62926a9a6666606e68f14d943d20b3223547c1ec8318e31ff13f127f24ff487d9366f826c3f04d56015db591a5b1818981a901906360616069c06dc66e686460606068ec46506d244f1677254fcf9c35dcb20c0c6a6c9c6606208d160606625c4606462606e6a62606864a5987f7bb19651d3e50e4c69c9691a20100ead82fda2c01000003';

function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

function fromHex(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) bytes[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return bytes;
}

describe('iptvJce', () => {
  it('encodes the timeshift request body byte-identically to upstream', () => {
    const body = buildTimeshiftBody('600001800', '2024075401', 1791213382, 1791213682, 'fhd');
    expect(toHex(body)).toBe(GOLDEN_BODY);
  });

  it('unwraps a packet produced by the upstream implementation', () => {
    const body = unwrapJcePacket(fromHex(GOLDEN_PACKET));
    expect(toHex(body)).toBe(GOLDEN_BODY);
  });

  it('round-trips wrap and unwrap', () => {
    const body = buildTimeshiftBody('600001859', '2024078201', 1791213382, 1791213682, 'fhd');
    const packet = wrapJcePacket(25312, body, 12345678, 'a1b2c3d4e5f607182930405060708090');
    expect(toHex(unwrapJcePacket(packet))).toBe(toHex(body));
  });

  it('rejects malformed packets', () => {
    expect(() => unwrapJcePacket(new Uint8Array(16))).toThrow();
    expect(() => unwrapJcePacket(fromHex(`13${'00'.repeat(120)}`))).toThrow();
  });
});
