import { describe, expect, it, vi } from 'vitest';
import { randomUuid } from './uuid';

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

describe('randomUuid', () => {
  it('produces an RFC 4122 v4 uuid', () => {
    expect(randomUuid()).toMatch(UUID_V4);
  });

  // The embedded shells serve the client from a plain http origin, which is not
  // a secure context: crypto.randomUUID is undefined there.
  it('falls back to getRandomValues when crypto.randomUUID is unavailable', () => {
    const getRandomValues = crypto.getRandomValues.bind(crypto);
    vi.stubGlobal('crypto', { getRandomValues });
    const id = randomUuid();
    vi.unstubAllGlobals();
    expect(id).toMatch(UUID_V4);
  });
});
