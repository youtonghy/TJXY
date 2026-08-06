import { describe, expect, it } from 'vitest';

import { CINEMATIC_DURATION_MS, getCinematicTimelineFrame } from './cinematicTimeline';

describe('cinematic setup intro timeline', () => {
  it.each([
    { elapsedMilliseconds: -100, phase: 'cinema-wake', progress: 0 },
    { elapsedMilliseconds: 1_500, phase: 'cinema-wake', progress: 0.5 },
    { elapsedMilliseconds: 5_000, phase: 'monochrome-film', progress: 0.5 },
    { elapsedMilliseconds: 9_000, phase: 'colour-cinema', progress: 0.5 },
    { elapsedMilliseconds: 12_500, phase: 'television', progress: 0.5 },
    { elapsedMilliseconds: 15_000, phase: 'tablet', progress: 0.5 },
    { elapsedMilliseconds: 16_500, phase: 'phone', progress: 0.5 },
    { elapsedMilliseconds: 17_500, phase: 'brand-handoff', progress: 0.5 },
    { elapsedMilliseconds: 99_000, phase: 'brand-handoff', progress: 1 },
  ] as const)('maps $elapsedMilliseconds ms to $phase', ({ elapsedMilliseconds, phase, progress }) => {
    expect(getCinematicTimelineFrame(elapsedMilliseconds)).toEqual({
      elapsedMilliseconds: Math.min(Math.max(elapsedMilliseconds, 0), CINEMATIC_DURATION_MS),
      phase,
      phaseProgress: progress,
      isComplete: elapsedMilliseconds >= CINEMATIC_DURATION_MS,
    });
  });

  it('uses an eighteen second full sequence', () => {
    expect(CINEMATIC_DURATION_MS).toBe(18_000);
  });

  it.each([
    [0, 'cinema-wake'],
    [2_999, 'cinema-wake'],
    [3_000, 'monochrome-film'],
    [6_999, 'monochrome-film'],
    [7_000, 'colour-cinema'],
    [10_999, 'colour-cinema'],
    [11_000, 'television'],
    [13_999, 'television'],
    [14_000, 'tablet'],
    [15_999, 'tablet'],
    [16_000, 'phone'],
    [16_999, 'phone'],
    [17_000, 'brand-handoff'],
    [18_000, 'brand-handoff'],
  ] as const)('keeps the %i ms boundary in %s', (elapsedMilliseconds, phase) => {
    expect(getCinematicTimelineFrame(elapsedMilliseconds).phase).toBe(phase);
  });

  it('treats an invalid elapsed time as the first frame', () => {
    expect(getCinematicTimelineFrame(Number.NaN)).toEqual({
      elapsedMilliseconds: 0,
      phase: 'cinema-wake',
      phaseProgress: 0,
      isComplete: false,
    });
  });
});
