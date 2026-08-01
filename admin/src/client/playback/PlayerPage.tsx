/* eslint-disable react-hooks/set-state-in-effect */
import { Alert, Button, Label, ListBox, Select, Skeleton } from '@heroui/react';
import { ArrowLeft, Captions, CircleAlert, Film, RotateCcw, SlidersHorizontal } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { getItem, togglePlayed, type MediaItem } from '../api/catalogApi';
import {
  getPlaybackInfo,
  getSubtitleBlob,
  issuePlaybackTicket,
  reportPlaybackProgress,
  revokePlaybackTicket,
  startPlayback,
  stopPlayback,
  type PlaybackSource,
  type PlaybackState as PlaybackStatePayload,
  type PlaybackStream,
  type PlaybackTicket,
} from '../api/playbackApi';
import { useClientAuth } from '../auth/ClientAuthContext';
import { browserSources, selectBrowserSource, sourceLabel } from './sourceSelection';

type LoadState = 'loading' | 'ready' | 'no-source' | 'unsupported' | 'error';
interface SubtitleTrack {
  key: string;
  stream: PlaybackStream;
  url: string;
}

const TICKS_PER_SECOND = 10_000_000;
const PROGRESS_INTERVAL_TICKS = 15 * TICKS_PER_SECOND;

export function PlayerPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const { user } = useClientAuth();
  const videoRef = useRef<HTMLVideoElement>(null);
  const resumeTicksRef = useRef(0);
  const positionTicksRef = useRef(0);
  const lastProgressTicksRef = useRef(0);
  const startedRef = useRef(false);
  const playbackContextRef = useRef<Pick<PlaybackStatePayload, 'itemId' | 'mediaSourceId' | 'playSessionId'> | undefined>(undefined);
  const [item, setItem] = useState<MediaItem>();
  const [sources, setSources] = useState<PlaybackSource[]>([]);
  const [selectedSourceId, setSelectedSourceId] = useState<string>();
  const [playSessionId, setPlaySessionId] = useState<string>();
  const [ticket, setTicket] = useState<PlaybackTicket>();
  const [subtitleTracks, setSubtitleTracks] = useState<SubtitleTrack[]>([]);
  const [selectedSubtitle, setSelectedSubtitle] = useState('off');
  const [state, setState] = useState<LoadState>('loading');
  const [playbackError, setPlaybackError] = useState<string>();
  const selectedSource = useMemo(
    () => sources.find((source) => source.Id === selectedSourceId),
    [selectedSourceId, sources],
  );

  useEffect(() => {
    playbackContextRef.current = id && selectedSource && playSessionId
      ? { itemId: id, mediaSourceId: selectedSource.Id, playSessionId }
      : undefined;
  }, [id, playSessionId, selectedSource]);

  useEffect(() => {
    const stopActivePlayback = () => {
      const context = playbackContextRef.current;
      if (!startedRef.current || !context) return;
      startedRef.current = false;
      void stopPlayback({
        ...context,
        positionTicks: positionTicksRef.current,
      }, { keepalive: true });
    };
    window.addEventListener('pagehide', stopActivePlayback);
    return () => {
      window.removeEventListener('pagehide', stopActivePlayback);
      stopActivePlayback();
    };
  }, []);

  useEffect(() => {
    if (!id) return;
    let active = true;
    void Promise.resolve()
      .then(() => {
        setState('loading');
        setItem(undefined);
        setSources([]);
        setSelectedSourceId(undefined);
        setPlaySessionId(undefined);
        setTicket(undefined);
        setPlaybackError(undefined);
        return getItem(id);
      })
      .then(async (nextItem) => {
        if (!active) return;
        setItem(nextItem);
        resumeTicksRef.current = nextItem.UserData?.PlaybackPositionTicks ?? 0;
        positionTicksRef.current = resumeTicksRef.current;
        if (nextItem.HasMediaSources === false) {
          setState('no-source');
          return;
        }
        const playback = await getPlaybackInfo(id);
        if (!playback.PlaySessionId) throw new Error('missing playback session');
        const compatible = browserSources(playback.MediaSources ?? []);
        const initial = selectBrowserSource(compatible);
        if (!initial) {
          setState('unsupported');
          return;
        }
        setSources(compatible);
        setSelectedSourceId(initial.Id);
        setPlaySessionId(playback.PlaySessionId);
      })
      .catch(() => {
        if (active) setState('error');
      });
    return () => {
      active = false;
    };
  }, [id]);

  useEffect(() => {
    if (!id || !playSessionId || !selectedSource) return;
    let active = true;
    setTicket(undefined);
    setPlaybackError(undefined);
    void issuePlaybackTicket(id, selectedSource.Id, playSessionId)
      .then((issued) => {
        if (!active) {
          void revokePlaybackTicket(issued.Id);
          return;
        }
        setTicket(issued);
        setState('ready');
      })
      .catch(() => {
        if (active) setState('error');
      });
    return () => {
      active = false;
    };
  }, [id, playSessionId, selectedSource]);

  useEffect(() => {
    if (!ticket) return;
    return () => {
      void revokePlaybackTicket(ticket.Id);
    };
  }, [ticket]);

  useEffect(() => {
    const controller = new AbortController();
    const createdUrls: string[] = [];
    const streams = selectedSource?.MediaStreams?.filter(
      (stream) => stream.Type === 'Subtitle' && stream.IsExternal && stream.DeliveryUrl,
    ) ?? [];
    setSubtitleTracks([]);
    setSelectedSubtitle('off');
    if (!streams.length) {
      return () => {
        controller.abort();
      };
    }
    let active = true;
    void Promise.all(streams.map(async (stream) => {
      const deliveryUrl = stream.DeliveryUrl;
      if (!deliveryUrl) throw new Error('missing subtitle delivery URL');
      const blob = await getSubtitleBlob(deliveryUrl, controller.signal);
      const url = URL.createObjectURL(blob);
      createdUrls.push(url);
      return { key: String(stream.Index ?? createdUrls.length - 1), stream, url };
    }))
      .then((tracks) => {
        if (!active) return;
        setSubtitleTracks(tracks);
        const defaultTrack = tracks.find((track) => track.stream.IsDefault);
        setSelectedSubtitle(defaultTrack?.key ?? 'off');
      })
      .catch((error: unknown) => {
        if (active && !(error instanceof DOMException && error.name === 'AbortError')) {
          setPlaybackError('Subtitles could not be loaded.');
        }
      });
    return () => {
      active = false;
      controller.abort();
      createdUrls.forEach((url) => {
        URL.revokeObjectURL(url);
      });
    };
  }, [selectedSource]);

  useEffect(() => {
    const textTracks = videoRef.current?.textTracks;
    if (!textTracks) return;
    const selectedIndex = subtitleTracks.findIndex((track) => track.key === selectedSubtitle);
    for (let index = 0; index < textTracks.length; index += 1) {
      const track = textTracks[index];
      if (track) {
        track.mode = selectedIndex === index ? 'showing' : 'disabled';
      }
    }
  }, [selectedSubtitle, subtitleTracks]);

  if (!id) return <p className="text-muted">This title could not be found.</p>;
  if (state === 'loading') return <Skeleton className="aspect-video w-full rounded-lg" />;
  if (state === 'error') {
    return (
      <PlaybackAlert id={id} status="danger" title="Playback unavailable">
        We could not prepare this title right now.
      </PlaybackAlert>
    );
  }
  if (state === 'no-source') {
    return (
      <div className="space-y-5">
        <BackLink id={id} />
        <Alert status="warning">
          <Alert.Indicator><Film className="size-4" /></Alert.Indicator>
          <Alert.Content>
            <Alert.Title>No video source available</Alert.Title>
            <Alert.Description>
              Add a media file to this title before starting playback.
            </Alert.Description>
          </Alert.Content>
        </Alert>
      </div>
    );
  }
  if (state === 'unsupported') {
    return (
      <PlaybackAlert id={id} status="warning" title="No browser-compatible source">
        Choose an MP4, WebM, MP3, M4A, or Ogg source for browser playback.
      </PlaybackAlert>
    );
  }
  if (!item || !ticket || !selectedSource || !playSessionId) {
    return <Skeleton className="aspect-video w-full rounded-lg" />;
  }

  const playbackState = (ticks = currentPositionTicks(videoRef.current)): PlaybackStatePayload => {
    positionTicksRef.current = ticks;
    return {
      itemId: id,
      mediaSourceId: selectedSource.Id,
      playSessionId,
      positionTicks: ticks,
    };
  };
  const selectSource = (sourceId: string) => {
    if (sourceId === selectedSource.Id) return;
    resumeTicksRef.current = currentPositionTicks(videoRef.current);
    if (startedRef.current) void stopPlayback(playbackState(resumeTicksRef.current));
    startedRef.current = false;
    lastProgressTicksRef.current = resumeTicksRef.current;
    setPlaySessionId(crypto.randomUUID());
    setSelectedSourceId(sourceId);
  };
  const nextSource = sources[(sources.findIndex((source) => source.Id === selectedSource.Id) + 1) % sources.length];

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between gap-4">
        <BackLink id={id} />
        <Button
          variant="tertiary"
          onPress={() => {
            const positionTicks = currentPositionTicks(videoRef.current);
            videoRef.current?.pause();
            if (startedRef.current) {
              startedRef.current = false;
              void stopPlayback(playbackState(positionTicks));
            }
            void navigate(`/app/items/${id}`);
          }}
        >
          Exit
        </Button>
      </div>

      <div className="overflow-hidden rounded-lg bg-black shadow-sm">
        <video
          aria-label={`Playing ${item.Name}`}
          className="aspect-video w-full"
          controls
          onEnded={() => {
            const finalState = playbackState(currentPositionTicks(videoRef.current));
            startedRef.current = false;
            void stopPlayback(finalState);
            if (user) void togglePlayed(user.Id, id, true);
            setItem({ ...item, UserData: { ...item.UserData, Played: true, PlaybackPositionTicks: 0 } });
          }}
          onError={() => {
            setPlaybackError('This source could not be played. Choose another source.');
          }}
          onLoadedMetadata={() => {
            const video = videoRef.current;
            if (video && resumeTicksRef.current > 0) {
              video.currentTime = resumeTicksRef.current / TICKS_PER_SECOND;
            }
          }}
          onPause={() => {
            if (startedRef.current) void reportPlaybackProgress(playbackState());
          }}
          onPlay={() => {
            const nextState = playbackState();
            lastProgressTicksRef.current = nextState.positionTicks;
            startedRef.current = true;
            void startPlayback(nextState);
          }}
          onTimeUpdate={() => {
            if (!startedRef.current) return;
            const positionTicks = currentPositionTicks(videoRef.current);
            positionTicksRef.current = positionTicks;
            if (positionTicks - lastProgressTicksRef.current >= PROGRESS_INTERVAL_TICKS) {
              lastProgressTicksRef.current = positionTicks;
              void reportPlaybackProgress(playbackState(positionTicks));
            }
          }}
          playsInline
          ref={videoRef}
          src={ticket.StreamUrl}
        >
          {subtitleTracks.map((track) => (
            <track
              default={track.key === selectedSubtitle}
              key={track.key}
              kind="subtitles"
              label={languageLabel(track.stream.Language)}
              src={track.url}
              srcLang={track.stream.Language ?? 'und'}
            />
          ))}
        </video>
      </div>

      {playbackError && (
        <Alert status="danger">
          <Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator>
          <Alert.Content>
            <Alert.Title>Source error</Alert.Title>
            <Alert.Description>{playbackError}</Alert.Description>
          </Alert.Content>
          {sources.length > 1 && nextSource && (
            <Button
              className="self-center"
              size="sm"
              variant="secondary"
              onPress={() => {
                setPlaybackError(undefined);
                selectSource(nextSource.Id);
              }}
            >
              <RotateCcw className="size-4" />
              Try next source
            </Button>
          )}
        </Alert>
      )}

      <div className="grid gap-4 border-b border-separator pb-5 sm:grid-cols-2">
        <Select
          aria-label="Video source"
          value={selectedSource.Id}
          variant="secondary"
          onChange={(key) => {
            if (key !== null) selectSource(String(key));
          }}
        >
          <Label className="flex items-center gap-2 text-sm">
            <SlidersHorizontal className="size-4" />
            Video source
          </Label>
          <Select.Trigger>
            <Select.Value />
            <Select.Indicator />
          </Select.Trigger>
          <Select.Popover>
            <ListBox>
              {sources.map((source) => (
                <ListBox.Item id={source.Id} key={source.Id} textValue={sourceLabel(source)}>
                  {sourceLabel(source)}
                  <ListBox.ItemIndicator />
                </ListBox.Item>
              ))}
            </ListBox>
          </Select.Popover>
        </Select>

        <Select
          aria-label="Subtitles"
          isDisabled={!subtitleTracks.length}
          value={selectedSubtitle}
          variant="secondary"
          onChange={(key) => {
            if (key !== null) setSelectedSubtitle(String(key));
          }}
        >
          <Label className="flex items-center gap-2 text-sm">
            <Captions className="size-4" />
            Subtitles
          </Label>
          <Select.Trigger>
            <Select.Value />
            <Select.Indicator />
          </Select.Trigger>
          <Select.Popover>
            <ListBox>
              <ListBox.Item id="off" textValue="Off">
                Off
                <ListBox.ItemIndicator />
              </ListBox.Item>
              {subtitleTracks.map((track) => (
                <ListBox.Item
                  id={track.key}
                  key={track.key}
                  textValue={languageLabel(track.stream.Language)}
                >
                  {languageLabel(track.stream.Language)}
                  <ListBox.ItemIndicator />
                </ListBox.Item>
              ))}
            </ListBox>
          </Select.Popover>
        </Select>
      </div>

      <div>
        <h1 className="text-xl font-semibold text-foreground">{item.Name}</h1>
        <p className="mt-1 text-sm text-muted">{sourceLabel(selectedSource)}</p>
      </div>
    </div>
  );
}

function currentPositionTicks(video: HTMLVideoElement | null): number {
  const seconds = video?.currentTime ?? 0;
  return Number.isFinite(seconds) && seconds > 0 ? Math.round(seconds * TICKS_PER_SECOND) : 0;
}

function languageLabel(language?: string): string {
  if (!language) return 'Unknown';
  const normalized = language.toLowerCase();
  if (normalized === 'zh-cn' || normalized === 'zho' || normalized === 'chi') return 'Chinese (Simplified)';
  if (normalized === 'en' || normalized === 'eng') return 'English';
  return language;
}

function BackLink({ id }: { id: string }) {
  return (
    <Link
      className="inline-flex items-center gap-2 text-sm text-muted hover:text-foreground"
      to={`/app/items/${id}`}
    >
      <ArrowLeft className="size-4" />
      Back to details
    </Link>
  );
}

function PlaybackAlert({
  id,
  status,
  title,
  children,
}: {
  id: string;
  status: 'danger' | 'warning';
  title: string;
  children: string;
}) {
  return (
    <div className="space-y-5">
      <BackLink id={id} />
      <Alert status={status}>
        <Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator>
        <Alert.Content>
          <Alert.Title>{title}</Alert.Title>
          <Alert.Description>{children}</Alert.Description>
        </Alert.Content>
      </Alert>
    </div>
  );
}
