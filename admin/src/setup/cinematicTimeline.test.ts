import { describe, expect, it } from 'vitest';

import { CINEMATIC_DURATION_MS, getCinematicCaption, getCinematicTimelineFrame } from './cinematicTimeline';

describe('cinematic setup intro timeline', () => {
  it.each([
    { elapsedMilliseconds: -100, phase: 'projector', progress: 0 },
    { elapsedMilliseconds: 2_500, phase: 'projector', progress: 0.5 },
    { elapsedMilliseconds: 7_000, phase: 'monochrome-film', progress: 0.5 },
    { elapsedMilliseconds: 11_000, phase: 'colour-cinema', progress: 0.5 },
    { elapsedMilliseconds: 16_000, phase: 'crt', progress: 0.5 },
    { elapsedMilliseconds: 21_500, phase: 'lcd', progress: 0.5 },
    { elapsedMilliseconds: 26_750, phase: 'devices', progress: 0.5 },
    { elapsedMilliseconds: 30_750, phase: 'brand-handoff', progress: 0.5 },
    { elapsedMilliseconds: 99_000, phase: 'brand-handoff', progress: 1 },
  ] as const)('maps $elapsedMilliseconds ms to $phase', ({ elapsedMilliseconds, phase, progress }) => {
    expect(getCinematicTimelineFrame(elapsedMilliseconds)).toEqual({
      elapsedMilliseconds: Math.min(Math.max(elapsedMilliseconds, 0), CINEMATIC_DURATION_MS),
      phase,
      phaseProgress: progress,
      isComplete: elapsedMilliseconds >= CINEMATIC_DURATION_MS,
    });
  });

  it('uses an thirty-two second full sequence', () => {
    expect(CINEMATIC_DURATION_MS).toBe(32_000);
  });

  it.each([
    [0, 'projector'],
    [4_999, 'projector'],
    [5_000, 'monochrome-film'],
    [8_999, 'monochrome-film'],
    [9_000, 'colour-cinema'],
    [12_999, 'colour-cinema'],
    [13_000, 'crt'],
    [18_999, 'crt'],
    [19_000, 'lcd'],
    [23_999, 'lcd'],
    [24_000, 'devices'],
    [29_499, 'devices'],
    [29_500, 'brand-handoff'],
    [32_000, 'brand-handoff'],
  ] as const)('keeps the %i ms boundary in %s', (elapsedMilliseconds, phase) => {
    expect(getCinematicTimelineFrame(elapsedMilliseconds).phase).toBe(phase);
  });

  it('treats an invalid elapsed time as the first frame', () => {
    expect(getCinematicTimelineFrame(Number.NaN)).toEqual({
      elapsedMilliseconds: 0,
      phase: 'projector',
      phaseProgress: 0,
      isComplete: false,
    });
  });
});

it('uses the selected setup language for the current chapter and closing frame', () => {
  expect(getCinematicCaption('crt', 'zh-CN').title).toBe('把整个世界，带回家。');
  expect(getCinematicCaption('crt', 'en-US').title).toBe('The world came home.');
  expect(getCinematicCaption('brand-handoff', 'en-US').title).toBe('Every screen is a new beginning.');
});
