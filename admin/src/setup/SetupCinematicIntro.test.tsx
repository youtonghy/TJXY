import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SetupCinematicIntro } from './SetupCinematicIntro';
import type { CinematicSceneFactory, CinematicSceneOptions } from './cinematicScene';

function createSceneFactory() {
  let options: CinematicSceneOptions | null = null;
  const dispose = vi.fn();
  const start = vi.fn();
  const factory = vi.fn<CinematicSceneFactory>((_canvas, nextOptions) => {
    options = nextOptions;
    return { dispose, start };
  });
  return { dispose, factory, getOptions: () => options, start };
}

it('renders a full-screen canvas and completes only once when skipped', async () => {
  const user = userEvent.setup();
  const scene = createSceneFactory();
  const onComplete = vi.fn();
  const { unmount } = render(
    <SetupCinematicIntro createScene={scene.factory} locale="en-US" onComplete={onComplete} />,
  );

  const canvas = screen.getByTestId('setup-cinematic-canvas');
  expect(canvas).toHaveAttribute('aria-hidden', 'true');
  expect(canvas.closest('section')).toHaveClass('fixed', 'inset-0');
  expect(scene.start).toHaveBeenCalledOnce();

  await user.click(screen.getByRole('button', { name: 'Skip intro' }));
  act(() => { scene.getOptions()?.onComplete(); });

  expect(onComplete).toHaveBeenCalledOnce();
  unmount();
  expect(scene.dispose).toHaveBeenCalledOnce();
});

it('uses the shortened scene when reduced motion is requested', () => {
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: vi.fn(() => ({
      matches: true,
      media: '(prefers-reduced-motion: reduce)',
      onchange: null,
      addEventListener: vi.fn(),
      addListener: vi.fn(),
      dispatchEvent: vi.fn(() => false),
      removeEventListener: vi.fn(),
      removeListener: vi.fn(),
    })),
  });
  const scene = createSceneFactory();

  render(<SetupCinematicIntro createScene={scene.factory} locale="zh-CN" onComplete={vi.fn()} />);

  expect(scene.factory).toHaveBeenCalledOnce();
  expect(scene.getOptions()?.reducedMotion).toBe(true);
  expect(screen.getByRole('button', { name: '跳过动画' })).toBeVisible();
});

it('completes and disposes once when the scene finishes naturally', () => {
  const scene = createSceneFactory();
  const onComplete = vi.fn();

  render(<SetupCinematicIntro createScene={scene.factory} locale="en-US" onComplete={onComplete} />);
  act(() => {
    scene.getOptions()?.onComplete();
    scene.getOptions()?.onComplete();
  });

  expect(scene.dispose).toHaveBeenCalledOnce();
  expect(onComplete).toHaveBeenCalledOnce();
});

it('shows the static terminal frame and advances when WebGL is unavailable', () => {
  vi.useFakeTimers();
  const onComplete = vi.fn();
  const createScene = vi.fn<CinematicSceneFactory>(() => {
    throw new Error('WebGL unavailable');
  });

  render(<SetupCinematicIntro createScene={createScene} locale="en-US" onComplete={onComplete} />);

  expect(screen.getByTestId('setup-cinematic-fallback')).toBeVisible();
  expect(screen.getByText('TJXY')).toBeVisible();
  act(() => { vi.advanceTimersByTime(999); });
  expect(onComplete).not.toHaveBeenCalled();
  act(() => { vi.advanceTimersByTime(1); });
  expect(onComplete).toHaveBeenCalledOnce();
  vi.useRealTimers();
});

it('falls back when a running scene loses its WebGL context', () => {
  vi.useFakeTimers();
  const scene = createSceneFactory();
  const onComplete = vi.fn();

  render(<SetupCinematicIntro createScene={scene.factory} locale="en-US" onComplete={onComplete} />);
  act(() => { scene.getOptions()?.onFailure(); });

  expect(scene.dispose).toHaveBeenCalledOnce();
  expect(screen.getByTestId('setup-cinematic-fallback')).toBeVisible();
  act(() => { vi.advanceTimersByTime(1_000); });
  act(() => { scene.getOptions()?.onComplete(); });
  expect(onComplete).toHaveBeenCalledOnce();
  vi.useRealTimers();
});
