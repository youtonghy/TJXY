/* eslint-disable react-hooks/set-state-in-effect */
import { Alert, Button, Label, ListBox, Select, Skeleton } from '@heroui/react';
import { ArrowLeft, Captions, CircleAlert, Film, RotateCcw, SlidersHorizontal } from 'lucide-react';
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
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
import { getApiBaseUrl, isDesktopShell, resolveApiUrl } from '../api/apiBase';
import { browserSources, isBrowserPlayableSource, nativeSources, selectBrowserSource, selectNativeSource, sourceLabel } from './sourceSelection';
import { useTranslate } from '../../settings/i18n';
import { attachHlsSource, isHlsSource } from './hlsPlayback';

type LoadState = 'loading' | 'ready' | 'no-source' | 'unsupported' | 'error';
interface SubtitleTrack {
  key: string;
  stream: PlaybackStream;
  url: string;
}

const TICKS_PER_SECOND = 10_000_000;
const PROGRESS_INTERVAL_TICKS = 15 * TICKS_PER_SECOND;

export function PlayerPage() {
  const tr = useTranslate();
  const { id } = useParams();
  const navigate = useNavigate();
  const { user } = useClientAuth();
  const videoRef = useRef<HTMLVideoElement>(null);
  const resumeTicksRef = useRef(0);
  const positionTicksRef = useRef(0);
  const playbackOffsetTicksRef = useRef(0);
  const lastProgressTicksRef = useRef(0);
  const autoplayAttemptedRef = useRef(false);
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
  const [autoplayBlocked, setAutoplayBlocked] = useState(false);
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
        const desktop = isDesktopShell();
        const compatible = desktop
          ? nativeSources(playback.MediaSources ?? [])
          : browserSources(playback.MediaSources ?? []);
        const initial = desktop ? selectNativeSource(compatible) : selectBrowserSource(compatible);
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

  const [embedSrc, setEmbedSrc] = useState<string>();
  const desktop = isDesktopShell();

  useEffect(() => {
    if (!desktop || !ticket || !selectedSource) {
      setEmbedSrc(undefined);
      return;
    }
    let cancelled = false;
    const streamUrl = ticket.StreamUrl.startsWith('http')
      ? ticket.StreamUrl
      : resolveApiUrl(ticket.StreamUrl, getApiBaseUrl());
    if (isBrowserPlayableSource(selectedSource)) {
      playbackOffsetTicksRef.current = 0;
      setEmbedSrc(streamUrl);
      return;
    }
    playbackOffsetTicksRef.current = resumeTicksRef.current;
    setEmbedSrc(undefined);
    void import('./desktopPlayer')
      .then(({ startDesktopStream }) => startDesktopStream(
        streamUrl,
        getApiBaseUrl(),
        resumeTicksRef.current / TICKS_PER_SECOND,
      ))
      .then((src) => {
        if (!cancelled) setEmbedSrc(src);
        else void import('./desktopPlayer').then(({ stopDesktopStream }) => stopDesktopStream()).catch(() => undefined);
      })
      .catch((error: unknown) => {
        if (!cancelled) setPlaybackError(error instanceof Error ? error.message : 'Playback proxy failed.');
      });
    return () => {
      cancelled = true;
      void import('./desktopPlayer').then(({ stopDesktopStream }) => stopDesktopStream()).catch(() => undefined);
    };
  }, [desktop, selectedSource, ticket]);

  const playerSrc = desktop ? embedSrc : ticket?.StreamUrl;
  const usesHlsAdapter = isHlsSource(playerSrc);

  useEffect(() => {
    autoplayAttemptedRef.current = false;
    setAutoplayBlocked(false);
    if (!usesHlsAdapter || !playerSrc || !videoRef.current) return;
    let active = true;
    let detach: (() => void) | undefined;
    void attachHlsSource(videoRef.current, playerSrc, () => {
      if (active) setPlaybackError('This HLS source could not be played. Choose another source.');
    }).then((cleanup) => {
      if (active) detach = cleanup;
      else cleanup();
    }).catch(() => {
      if (active) setPlaybackError('This HLS source could not be played. Choose another source.');
    });
    return () => {
      active = false;
      detach?.();
    };
  }, [playerSrc, usesHlsAdapter]);

  if (!id) return <p className="text-muted">{tr('This title could not be found.', '找不到此影片。')}</p>;
  if (state === 'loading') return <Skeleton className="aspect-video w-full rounded-lg" />;
  if (state === 'error') {
    return (
      <PlaybackAlert id={id} status="danger" title={tr('Playback unavailable', '无法播放')}>
        {tr('We could not prepare this title right now.', '目前无法准备此影片进行播放。')}
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
            <Alert.Title>{tr('No video source available', '没有可用的视频源')}</Alert.Title>
            <Alert.Description>
              {tr('Add a media file to this title before starting playback.', '开始播放前，请先为此影片添加媒体文件。')}
            </Alert.Description>
          </Alert.Content>
        </Alert>
      </div>
    );
  }
  if (state === 'unsupported') {
    return (
      <PlaybackAlert id={id} status="warning" title={desktop ? tr('No playable source', '没有可播放的视频源') : tr('No browser-compatible source', '没有浏览器兼容的视频源')}>
        {desktop
          ? tr('This title has no direct-play source.', '此影片没有可直接播放的视频源。')
          : tr('Choose an MP4, WebM, MP3, M4A, or Ogg source for browser playback.', '请选择 MP4、WebM、MP3、M4A 或 Ogg 格式以在浏览器中播放。')}
      </PlaybackAlert>
    );
  }
  if (!item || !ticket || !selectedSource || !playSessionId) {
    return <Skeleton className="aspect-video w-full rounded-lg" />;
  }

  const playbackState = (
    ticks = playbackOffsetTicksRef.current + currentPositionTicks(videoRef.current)
  ): PlaybackStatePayload => {
    positionTicksRef.current = ticks;
    return {
      itemId: id,
      mediaSourceId: selectedSource.Id,
      playSessionId,
      positionTicks: ticks,
    };
  };
  const currentPlaybackTicks = () => (
    playbackOffsetTicksRef.current + currentPositionTicks(videoRef.current)
  );
  const selectSource = (sourceId: string) => {
    if (sourceId === selectedSource.Id) return;
    resumeTicksRef.current = currentPlaybackTicks();
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
            const positionTicks = currentPlaybackTicks();
            videoRef.current?.pause();
            if (startedRef.current) {
              startedRef.current = false;
              void stopPlayback(playbackState(positionTicks));
            }
            void navigate(`/app/items/${id}`);
          }}
        >
          {tr('Exit', '退出播放')}
        </Button>
      </div>

      <div className="overflow-hidden rounded-lg bg-black shadow-sm">
        {desktop && !embedSrc ? (
          <Skeleton className="aspect-video w-full rounded-lg" />
        ) : (
        <video
          aria-label={tr(`Playing ${item.Name}`, `正在播放 ${item.Name}`)}
          autoPlay
          className="aspect-video w-full"
          controls
          onCanPlay={() => {
            const video = videoRef.current;
            if (!video || autoplayAttemptedRef.current || !video.paused) return;
            autoplayAttemptedRef.current = true;
            void video.play().catch(() => { setAutoplayBlocked(true); });
          }}
          onEnded={() => {
            const finalState = playbackState(currentPlaybackTicks());
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
            if (video && playbackOffsetTicksRef.current === 0 && resumeTicksRef.current > 0) {
              video.currentTime = resumeTicksRef.current / TICKS_PER_SECOND;
            }
          }}
          onPause={() => {
            if (startedRef.current) void reportPlaybackProgress(playbackState());
          }}
          onPlay={() => {
            setAutoplayBlocked(false);
            const nextState = playbackState();
            lastProgressTicksRef.current = nextState.positionTicks;
            startedRef.current = true;
            void startPlayback(nextState);
          }}
          onTimeUpdate={() => {
            if (!startedRef.current) return;
            const positionTicks = currentPlaybackTicks();
            positionTicksRef.current = positionTicks;
            if (positionTicks - lastProgressTicksRef.current >= PROGRESS_INTERVAL_TICKS) {
              lastProgressTicksRef.current = positionTicks;
              void reportPlaybackProgress(playbackState(positionTicks));
            }
          }}
          playsInline
          ref={videoRef}
          src={usesHlsAdapter ? undefined : playerSrc}
        >
          {subtitleTracks.map((track) => (
            <track
              default={track.key === selectedSubtitle}
              key={track.key}
              kind="subtitles"
              label={languageLabel(track.stream.Language, tr)}
              src={track.url}
              srcLang={track.stream.Language ?? 'und'}
            />
          ))}
        </video>
        )}
      </div>

      {autoplayBlocked && (
        <Alert status="warning">
          <Alert.Content>
            <Alert.Title>{tr('Playback is paused', '播放已暂停')}</Alert.Title>
            <Alert.Description>{tr('Automatic playback was blocked by the system.', '系统阻止了自动播放。')}</Alert.Description>
          </Alert.Content>
          <Button
            className="self-center"
            size="sm"
            variant="secondary"
            onPress={() => { void videoRef.current?.play().catch(() => undefined); }}
          >
            {tr('Play', '播放')}
          </Button>
        </Alert>
      )}

      {playbackError && (
        <Alert status="danger">
          <Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator>
          <Alert.Content>
            <Alert.Title>{tr('Source error', '视频源错误')}</Alert.Title>
            <Alert.Description>{translatePlaybackError(playbackError, tr)}</Alert.Description>
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
              {tr('Try next source', '尝试下一个视频源')}
            </Button>
          )}
        </Alert>
      )}

      <div className="grid gap-4 border-b border-separator pb-5 sm:grid-cols-2">
        <Select
          aria-label={tr('Video source', '视频源')}
          value={selectedSource.Id}
          variant="secondary"
          onChange={(key) => {
            if (key !== null) selectSource(String(key));
          }}
        >
          <Label className="flex items-center gap-2 text-sm">
            <SlidersHorizontal className="size-4" />
            {tr('Video source', '视频源')}
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
          aria-label={tr('Subtitles', '字幕')}
          isDisabled={!subtitleTracks.length}
          value={selectedSubtitle}
          variant="secondary"
          onChange={(key) => {
            if (key !== null) setSelectedSubtitle(String(key));
          }}
        >
          <Label className="flex items-center gap-2 text-sm">
            <Captions className="size-4" />
            {tr('Subtitles', '字幕')}
          </Label>
          <Select.Trigger>
            <Select.Value />
            <Select.Indicator />
          </Select.Trigger>
          <Select.Popover>
            <ListBox>
              <ListBox.Item id="off" textValue={tr('Off', '关闭')}>
                {tr('Off', '关闭')}
                <ListBox.ItemIndicator />
              </ListBox.Item>
              {subtitleTracks.map((track) => (
                <ListBox.Item
                  id={track.key}
                  key={track.key}
                  textValue={languageLabel(track.stream.Language, tr)}
                >
                  {languageLabel(track.stream.Language, tr)}
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

function languageLabel(language: string | undefined, tr: (english: string, chinese: string) => string): string {
  if (!language) return tr('Unknown', '未知');
  const normalized = language.toLowerCase();
  if (normalized === 'zh-cn' || normalized === 'zho' || normalized === 'chi') return tr('Chinese (Simplified)', '简体中文');
  if (normalized === 'en' || normalized === 'eng') return tr('English', '英语');
  return language;
}

function BackLink({ id }: { id: string }) {
  const tr = useTranslate();
  return (
    <Link
      className="inline-flex items-center gap-2 text-sm text-muted hover:text-foreground"
      to={`/app/items/${id}`}
    >
      <ArrowLeft className="size-4" />
      {tr('Back to details', '返回详细信息')}
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
  children: ReactNode;
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

function translatePlaybackError(message: string, tr: (english: string, chinese: string) => string) {
  if (message === 'Subtitles could not be loaded.') return tr(message, '无法加载字幕。');
  if (message === 'This source could not be played. Choose another source.') return tr(message, '无法播放此视频源，请选择其他视频源。');
  if (message === 'This HLS source could not be played. Choose another source.') return tr(message, '无法播放此 HLS 视频源，请选择其他视频源。');
  if (message === 'Playback proxy failed.') return tr(message, '无法在播放页面中准备视频流。');
  return message;
}
