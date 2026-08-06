import { Button } from '@heroui/react';
import { FastForward } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';

import type { CinematicSceneController, CinematicSceneFactory } from './cinematicScene';

type SetupLocale = 'zh-CN' | 'en-US';

export function SetupCinematicIntro({
  createScene,
  locale,
  onComplete,
}: {
  createScene?: CinematicSceneFactory;
  locale: SetupLocale;
  onComplete: () => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const completeRef = useRef(onComplete);
  const completedRef = useRef(false);
  const controllerRef = useRef<CinematicSceneController | undefined>(undefined);
  const fallbackTimerRef = useRef<ReturnType<typeof window.setTimeout> | undefined>(undefined);
  const [fallback, setFallback] = useState(false);
  const tr = (english: string, chinese: string) => locale === 'en-US' ? english : chinese;

  useEffect(() => { completeRef.current = onComplete; }, [onComplete]);

  const complete = useCallback(() => {
    if (completedRef.current) return;
    completedRef.current = true;
    if (fallbackTimerRef.current !== undefined) window.clearTimeout(fallbackTimerRef.current);
    controllerRef.current?.dispose();
    controllerRef.current = undefined;
    completeRef.current();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    let cancelled = false;
    let failureHandled = false;
    const fail = () => {
      if (cancelled || failureHandled || completedRef.current) return;
      failureHandled = true;
      controllerRef.current?.dispose();
      controllerRef.current = undefined;
      setFallback(true);
      fallbackTimerRef.current = window.setTimeout(complete, 1_000);
    };
    const startScene = (factory: CinematicSceneFactory) => {
      if (cancelled || completedRef.current) return;
      try {
        controllerRef.current = factory(canvas, {
          onComplete: complete,
          onFailure: fail,
          reducedMotion: window.matchMedia('(prefers-reduced-motion: reduce)').matches,
        });
        controllerRef.current.start();
      } catch {
        fail();
      }
    };

    if (createScene) startScene(createScene);
    else void import('./cinematicScene')
      .then(({ createCinematicScene: factory }) => { startScene(factory); })
      .catch(() => { fail(); });

    return () => {
      cancelled = true;
      if (fallbackTimerRef.current !== undefined) window.clearTimeout(fallbackTimerRef.current);
      controllerRef.current?.dispose();
      controllerRef.current = undefined;
    };
  }, [complete, createScene]);

  return (
    <section
      aria-label={tr('TJXY setup introduction', 'TJXY 安装开场动画')}
      className="fixed inset-0 z-[100] min-h-dvh overflow-hidden bg-black text-white"
    >
      <canvas
        aria-hidden="true"
        className="absolute inset-0 size-full touch-none"
        data-testid="setup-cinematic-canvas"
        ref={canvasRef}
      />
      {fallback && (
        <div
          className="absolute inset-0 flex flex-col items-center justify-center bg-black"
          data-testid="setup-cinematic-fallback"
        >
          <img alt="" aria-hidden="true" className="size-20 object-contain" src="/brand/tjxy-mark.webp" />
          <p className="mt-4 text-xl font-semibold tracking-normal">TJXY</p>
        </div>
      )}
      <p aria-live="polite" className="sr-only">
        {tr('TJXY setup introduction is playing.', '正在播放 TJXY 安装开场动画。')}
      </p>
      <Button
        className="absolute right-[max(1rem,env(safe-area-inset-right))] top-[max(1rem,env(safe-area-inset-top))] z-10 border-white/20 bg-black/45 text-white backdrop-blur-sm"
        onPress={complete}
        variant="tertiary"
      >
        <FastForward aria-hidden="true" className="size-4" />
        {tr('Skip intro', '跳过动画')}
      </Button>
    </section>
  );
}
