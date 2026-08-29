import type { ReactNode, RefObject } from 'react';

interface WebPlayerSurfaceProps {
  videoRef: RefObject<HTMLVideoElement | null>;
  src?: string;
  title: string;
  subtitles: ReactNode;
  onCanPlay: () => void;
  onEnded: () => void;
  onError: () => void;
  onLoadedMetadata: () => void;
  onPause: () => void;
  onPlay: () => void;
  onTimeUpdate: () => void;
}

export function WebPlayerSurface({
  videoRef,
  src,
  title,
  subtitles,
  onCanPlay,
  onEnded,
  onError,
  onLoadedMetadata,
  onPause,
  onPlay,
  onTimeUpdate,
}: WebPlayerSurfaceProps) {
  return (
    <video
      aria-label={title}
      autoPlay
      className="aspect-video w-full object-contain"
      controls
      onCanPlay={onCanPlay}
      onEnded={onEnded}
      onError={onError}
      onLoadedMetadata={onLoadedMetadata}
      onPause={onPause}
      onPlay={onPlay}
      onTimeUpdate={onTimeUpdate}
      playsInline
      ref={videoRef}
      src={src}
    >
      {subtitles}
    </video>
  );
}
