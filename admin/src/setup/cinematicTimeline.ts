export const CINEMATIC_DURATION_MS = 18_000;

export type CinematicPhase =
  | 'cinema-wake'
  | 'monochrome-film'
  | 'colour-cinema'
  | 'television'
  | 'tablet'
  | 'phone'
  | 'brand-handoff';

export const CINEMATIC_PHASE_START_MS = {
  'cinema-wake': 0,
  'monochrome-film': 3_000,
  'colour-cinema': 7_000,
  television: 11_000,
  tablet: 14_000,
  phone: 16_000,
  'brand-handoff': 17_000,
} as const satisfies Record<CinematicPhase, number>;

export interface CinematicTimelineFrame {
  elapsedMilliseconds: number;
  phase: CinematicPhase;
  phaseProgress: number;
  isComplete: boolean;
}

const phases = [
  { phase: 'cinema-wake', start: CINEMATIC_PHASE_START_MS['cinema-wake'], end: CINEMATIC_PHASE_START_MS['monochrome-film'] },
  { phase: 'monochrome-film', start: CINEMATIC_PHASE_START_MS['monochrome-film'], end: CINEMATIC_PHASE_START_MS['colour-cinema'] },
  { phase: 'colour-cinema', start: CINEMATIC_PHASE_START_MS['colour-cinema'], end: CINEMATIC_PHASE_START_MS.television },
  { phase: 'television', start: CINEMATIC_PHASE_START_MS.television, end: CINEMATIC_PHASE_START_MS.tablet },
  { phase: 'tablet', start: CINEMATIC_PHASE_START_MS.tablet, end: CINEMATIC_PHASE_START_MS.phone },
  { phase: 'phone', start: CINEMATIC_PHASE_START_MS.phone, end: CINEMATIC_PHASE_START_MS['brand-handoff'] },
  { phase: 'brand-handoff', start: CINEMATIC_PHASE_START_MS['brand-handoff'], end: CINEMATIC_DURATION_MS },
] as const satisfies readonly { phase: CinematicPhase; start: number; end: number }[];

export function getCinematicPhaseStartSeconds(phase: CinematicPhase): number {
  return CINEMATIC_PHASE_START_MS[phase] / 1_000;
}

export function getCinematicTimelineFrame(elapsedMilliseconds: number): CinematicTimelineFrame {
  const safeElapsed = Number.isFinite(elapsedMilliseconds) ? elapsedMilliseconds : 0;
  const elapsed = Math.min(Math.max(safeElapsed, 0), CINEMATIC_DURATION_MS);
  const phase = phases.find((candidate) => elapsed < candidate.end) ?? phases.at(-1);

  if (!phase) throw new Error('The cinematic timeline requires at least one phase.');

  return {
    elapsedMilliseconds: elapsed,
    phase: phase.phase,
    phaseProgress: Math.min(Math.max((elapsed - phase.start) / (phase.end - phase.start), 0), 1),
    isComplete: elapsedMilliseconds >= CINEMATIC_DURATION_MS,
  };
}
