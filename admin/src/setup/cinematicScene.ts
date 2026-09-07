import { createEvolutionScene } from './cinematic/scene';
import { DURATION } from './cinematicTimeline';

export interface CinematicSceneOptions {
  onComplete: () => void;
  onFailure: () => void;
  onFrame?: (seconds: number) => void;
  reducedMotion: boolean;
}

export interface CinematicSceneController {
  dispose: () => void;
  start: () => void;
}

export type CinematicSceneFactory = (
  canvas: HTMLCanvasElement,
  options: CinematicSceneOptions,
) => CinematicSceneController;

export const createCinematicScene: CinematicSceneFactory = (canvas, options) => {
  let started = false;
  let disposed = false;
  let finished = false;
  const unavailable = () => disposed || finished;
  const scene = createEvolutionScene(canvas, {
    reducedMotion: options.reducedMotion,
    onFrame: (seconds) => { if (!disposed) options.onFrame?.(seconds); },
    onFailure: () => {
      if (disposed || finished) return;
      finished = true;
      options.onFailure();
    },
    onComplete: () => {
      if (!started || disposed || finished) return;
      finished = true;
      options.onComplete();
    },
  });

  return {
    start: () => {
      if (started || disposed || finished) return;
      started = true;
      // Keep a static final composition for 1.5 seconds when motion is reduced.
      if (options.reducedMotion) scene.seek(DURATION - 1.5);
      if (!unavailable()) scene.play();
    },
    dispose: () => {
      if (disposed) return;
      disposed = true;
      scene.dispose();
    },
  };
};
