import { describe, expect, it } from 'vitest';

import { getCinematicSceneDetail } from './cinematicQuality';

describe('cinematic scene detail', () => {
  it('uses reduced detail on a narrow viewport', () => {
    expect(getCinematicSceneDetail({ viewportWidth: 390 })).toBe('reduced');
  });

  it('uses reduced detail on constrained hardware', () => {
    expect(getCinematicSceneDetail({ hardwareConcurrency: 4, viewportWidth: 1_440 })).toBe('reduced');
    expect(getCinematicSceneDetail({ deviceMemory: 4, viewportWidth: 1_440 })).toBe('reduced');
  });

  it('keeps full detail when no constrained signal is present', () => {
    expect(getCinematicSceneDetail({
      deviceMemory: 8,
      hardwareConcurrency: 8,
      viewportWidth: 1_440,
    })).toBe('full');
  });
});
