import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useEffect, useRef, useState } from 'react';
import {
  closeDesktopPlayer,
  openDesktopPlayer,
  updateDesktopPlayerViewport,
  type DesktopPlayerEvent,
  type DesktopPlayerOpenRequest,
  type DesktopPlayerSnapshot,
} from './desktopPlayer';

interface DesktopPlayerSurfaceProps {
  request: DesktopPlayerOpenRequest;
  title: string;
  onEnded: (positionTicks: number) => void;
  onError: (message: string) => void;
  onPhase: (snapshot: DesktopPlayerSnapshot, previousPhase?: string) => void;
}

export function DesktopPlayerSurface({ request, title, onEnded, onError, onPhase }: DesktopPlayerSurfaceProps) {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const phaseRef = useRef<string | undefined>(undefined);
  const callbacksRef = useRef({ onEnded, onError, onPhase });
  const [, setSnapshot] = useState<DesktopPlayerSnapshot>({
    loadId: request.loadId,
    phase: 'loading',
    positionTicks: request.startPositionTicks,
    seekable: false,
    volume: 100,
    muted: false,
    playbackRate: 1,
  });

  useEffect(() => {
    callbacksRef.current = { onEnded, onError, onPhase };
  }, [onEnded, onError, onPhase]);

  useEffect(() => {
    const surface = surfaceRef.current;
    if (!surface) return;
    let active = true;
    let unlisten: UnlistenFn | undefined;

    const viewport = () => {
      const rect = surface.getBoundingClientRect();
      return {
        x: rect.left,
        y: rect.top,
        width: rect.width,
        height: rect.height,
        visible: active && rect.width > 0 && rect.height > 0,
      };
    };
    const syncViewport = () => {
      void updateDesktopPlayerViewport(request.loadId, viewport()).catch(() => undefined);
    };
    const observer = new ResizeObserver(syncViewport);
    observer.observe(surface);
    window.addEventListener('resize', syncViewport);
    window.addEventListener('scroll', syncViewport, true);

    void listen<DesktopPlayerEvent>('tjxy-desktop-player-event', ({ payload }) => {
      const eventLoadId = payload.kind === 'state' ? payload.snapshot.loadId : payload.loadId;
      if (!active || eventLoadId !== request.loadId) return;
      if (payload.kind === 'error') {
        callbacksRef.current.onError(payload.message);
      } else if (payload.kind === 'ended') {
        callbacksRef.current.onEnded(payload.positionTicks);
      } else {
        const previousPhase = phaseRef.current;
        phaseRef.current = payload.snapshot.phase;
        setSnapshot(payload.snapshot);
        callbacksRef.current.onPhase(payload.snapshot, previousPhase);
      }
    }).then((cleanup) => {
      if (!active) cleanup();
      else unlisten = cleanup;
      return openDesktopPlayer(request, viewport());
    }).then((initial) => {
      if (active) setSnapshot(initial);
    }).catch((error: unknown) => {
      if (!active) return;
      const message = error instanceof Error
        ? error.message
        : typeof error === 'string'
          ? error
          : 'The embedded player could not start.';
      callbacksRef.current.onError(message);
    });

    return () => {
      active = false;
      observer.disconnect();
      window.removeEventListener('resize', syncViewport);
      window.removeEventListener('scroll', syncViewport, true);
      unlisten?.();
      void closeDesktopPlayer(request.loadId).catch(() => undefined);
    };
  }, [request]);

  return (
    <div className="bg-black text-white">
      <div
        aria-label={`Playing ${title}`}
        className="aspect-video w-full bg-black"
        ref={surfaceRef}
        role="img"
      />
    </div>
  );
}
