import { createEvolutionScene } from './cinematic/scene';
import type { EvolutionOptions } from './cinematic/scene';
import { createCinematicScene } from './cinematicScene';

vi.mock('./cinematic/scene', () => ({ createEvolutionScene: vi.fn() }));

function sceneHarness(reducedMotion = false, failDuringCreation = false) {
  const play = vi.fn();
  const pause = vi.fn();
  const seek = vi.fn();
  const dispose = vi.fn();
  const onComplete = vi.fn();
  const onFailure = vi.fn();
  const onFrame = vi.fn();
  let callbacks: EvolutionOptions | undefined;
  vi.mocked(createEvolutionScene).mockImplementation((_canvas, options) => {
    callbacks = options;
    if (failDuringCreation) options.onFailure();
    return { play, pause, seek, dispose };
  });
  const controller = createCinematicScene(document.createElement('canvas'), {
    reducedMotion, onComplete, onFailure, onFrame,
  });
  return { controller, play, seek, dispose, onComplete, onFailure, onFrame, callbacks: () => callbacks };
}

it('starts the approved scene once and completes once at its natural ending', () => {
  const scene = sceneHarness();
  scene.controller.start();
  scene.controller.start();
  expect(scene.play).toHaveBeenCalledOnce();
  expect(scene.seek).not.toHaveBeenCalled();
  scene.callbacks()?.onFrame(13, true);
  expect(scene.onFrame).toHaveBeenCalledWith(13);
  scene.callbacks()?.onComplete?.();
  scene.callbacks()?.onComplete?.();
  expect(scene.onComplete).toHaveBeenCalledOnce();
  expect(scene.onFailure).not.toHaveBeenCalled();
});

it('starts reduced motion on the static final composition with only 1.5 seconds remaining', () => {
  const scene = sceneHarness(true);
  scene.controller.start();
  expect(scene.callbacks()?.reducedMotion).toBe(true);
  expect(scene.seek).toHaveBeenCalledWith(30.5);
  expect(scene.seek.mock.invocationCallOrder[0]).toBeLessThan(scene.play.mock.invocationCallOrder[0] ?? 0);
  expect(scene.onComplete).not.toHaveBeenCalled();
  scene.callbacks()?.onComplete?.();
  expect(scene.onComplete).toHaveBeenCalledOnce();
});

it('ignores late callbacks and releases the renderer once when skipped or unmounted', () => {
  const scene = sceneHarness();
  scene.controller.start();
  scene.controller.dispose();
  scene.controller.dispose();
  scene.controller.start();
  scene.callbacks()?.onComplete?.();
  scene.callbacks()?.onFailure();
  scene.callbacks()?.onFrame(32, false);
  expect(scene.dispose).toHaveBeenCalledOnce();
  expect(scene.play).toHaveBeenCalledOnce();
  expect(scene.onComplete).not.toHaveBeenCalled();
  expect(scene.onFailure).not.toHaveBeenCalled();
  expect(scene.onFrame).not.toHaveBeenCalled();
});

it('reports rendering failure once and never reports successful completion afterward', () => {
  const scene = sceneHarness();
  scene.controller.start();
  scene.callbacks()?.onFailure();
  scene.callbacks()?.onFailure();
  scene.callbacks()?.onComplete?.();
  expect(scene.onFailure).toHaveBeenCalledOnce();
  expect(scene.onComplete).not.toHaveBeenCalled();
});

it('does not play a scene that failed during creation', () => {
  const scene = sceneHarness(false, true);
  scene.controller.start();
  expect(scene.onFailure).toHaveBeenCalledOnce();
  expect(scene.play).not.toHaveBeenCalled();
  scene.controller.dispose();
  expect(scene.dispose).toHaveBeenCalledOnce();
});
