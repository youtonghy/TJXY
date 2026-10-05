import { Alert, Button, Chip, Spinner } from '@heroui/react';
import { ArrowLeft, History, RotateCcw, Tv } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useTranslate } from '../../settings/i18n';
import { attachHlsSource } from '../playback/hlsPlayback';
import { WebPlayerSurface } from '../playback/WebPlayerSurface';
import { isIptvSupportedShell, resolveIptvChannelCached } from './iptvApi';
import { getIptvChannel, IPTV_LOGO_BASE, iptvChannelGroup, type IptvChannel } from './iptvChannels';
import { loadIptvGuide } from './iptvEpg';
import { attachIptvLive, isIptvJceChannel } from './iptvLive';

type PlayerState = 'loading' | 'ready' | 'failed' | 'unsupported';
type FailureKind = 'resolve' | 'playback';

export function IptvPlayerPage() {
  const tr = useTranslate();
  const { slug = '' } = useParams();
  const channel = getIptvChannel(slug);
  const supported = isIptvSupportedShell();
  const videoRef = useRef<HTMLVideoElement>(null);
  const [urls, setUrls] = useState<string[]>();
  const [failure, setFailure] = useState<FailureKind>('resolve');
  const [reloadKey, setReloadKey] = useState(0);
  const [nowPlaying, setNowPlaying] = useState<string>();
  // JCE-capable channels play through the rolling-window session first; the
  // raw bkliveinfo URL chain is only a fallback for engines without MSE or
  // when the session fatally fails.
  const jceEligible = channel !== undefined && isIptvJceChannel(channel);
  const [jceExhaustedFor, setJceExhaustedFor] = useState<string>();
  const jceExhausted = jceExhaustedFor === channel?.slug;
  const [state, setState] = useState<PlayerState>(() =>
    channel && supported && isIptvJceChannel(channel) ? 'ready' : 'loading',
  );

  useEffect(() => {
    if (!channel || !supported) return;
    // The live session resolves streams lazily inside the playlist loader, so
    // JCE channels are ready as soon as the video element mounts.
    if (jceEligible && !jceExhausted) return;
    let active = true;
    void resolveIptvChannelCached(channel)
      .then((resolved) => {
        if (!active) return;
        setUrls(resolved);
        setState('ready');
      })
      .catch(() => {
        if (!active) return;
        setFailure('resolve');
        setState('failed');
      });
    return () => { active = false; };
  }, [channel, supported, reloadKey, jceEligible, jceExhausted]);

  useEffect(() => {
    if (!channel?.tvgId || !supported) return;
    let active = true;
    void loadIptvGuide().then((guide) => {
      if (active) setNowPlaying(guide.get(channel.tvgId ?? '')?.title);
    });
    return () => { active = false; };
  }, [channel, supported]);

  // Attach sources in order; a fatal failure advances to the next url.
  useEffect(() => {
    const video = videoRef.current;
    if (!video || !channel || state !== 'ready') return;
    let disposed = false;
    let index = 0;
    let detach: (() => void) | undefined;
    const attach = () => {
      const url = urls?.[index];
      if (disposed) return;
      if (!url) {
        setFailure('playback');
        setState('failed');
        return;
      }
      const advance = () => {
        if (disposed) return;
        index += 1;
        detach?.();
        detach = undefined;
        attach();
      };
      video.addEventListener('error', advance, { once: true });
      void attachHlsSource(video, url, advance).then((cleanup) => {
        if (disposed) {
          cleanup();
          return;
        }
        detach = () => {
          video.removeEventListener('error', advance);
          cleanup();
        };
      });
    };
    if (jceEligible && !jceExhausted) {
      // `undefined` means MSE is unavailable; drop to the raw URL chain so a
      // native-HLS engine still gets a source. A fatal session error falls
      // back the same way.
      void attachIptvLive(video, channel, () => {
        if (!disposed) setJceExhaustedFor(channel.slug);
      }).then((cleanup) => {
        if (disposed) {
          cleanup?.();
          return;
        }
        if (cleanup) {
          detach = cleanup;
        } else {
          setJceExhaustedFor(channel.slug);
        }
      });
    } else {
      attach();
    }
    return () => {
      disposed = true;
      detach?.();
    };
  }, [channel, urls, state, jceEligible, jceExhausted]);

  if (!channel) {
    return (
      <Alert role="alert" status="danger">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Title>{tr('Unknown channel', '未知频道')}</Alert.Title>
          <Alert.Description>
            <Link className="text-accent hover:underline" to="/app/iptv">
              {tr('Back to the channel list.', '返回频道列表。')}
            </Link>
          </Alert.Description>
        </Alert.Content>
      </Alert>
    );
  }

  if (!supported) {
    return <UnsupportedNotice channel={channel} />;
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <Link
          aria-label={tr('Back to channels', '返回频道列表')}
          className="inline-flex size-9 items-center justify-center rounded-lg text-muted transition-colors hover:bg-surface-secondary hover:text-foreground"
          to="/app/iptv"
        >
          <ArrowLeft className="size-4" />
        </Link>
        <ChannelHeading channel={channel} />
      </div>

      <div className="overflow-hidden rounded-xl border border-default/20 bg-black">
        {state === 'ready' ? (
          <WebPlayerSurface
            onCanPlay={() => undefined}
            onEnded={() => undefined}
            onError={() => undefined}
            onLoadedMetadata={() => undefined}
            onPause={() => undefined}
            onPlay={() => undefined}
            onTimeUpdate={() => undefined}
            subtitles={null}
            title={channel.name}
            videoRef={videoRef}
          />
        ) : (
          <div className="flex aspect-video w-full items-center justify-center">
            {state === 'loading' ? (
              <Spinner aria-label={tr('Resolving stream', '正在解析直播地址')} color="accent" />
            ) : (
              <div className="flex flex-col items-center gap-3 p-6 text-center">
                <Tv aria-hidden="true" className="size-8 text-muted" />
                <p className="text-sm text-muted">
                  {tr('This channel is not available right now.', '该频道暂时无法播放。')}
                </p>
                <Button
                  onPress={() => {
                    setUrls(undefined);
                    setJceExhaustedFor(undefined);
                    setState(jceEligible ? 'ready' : 'loading');
                    setReloadKey((key) => key + 1);
                  }}
                  size="sm"
                  variant="secondary"
                >
                  <RotateCcw aria-hidden="true" className="size-4" />
                  {tr('Retry', '重试')}
                </Button>
              </div>
            )}
          </div>
        )}
      </div>

      {state === 'failed' && (
        <Alert role="alert" status="danger">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>
              {failure === 'resolve'
                ? tr('Unable to resolve the live stream', '无法解析直播地址')
                : tr('The resolved streams could not be played', '直播地址解析成功，但所有流均无法播放')}
            </Alert.Title>
            <Alert.Description>
              {failure === 'resolve'
                ? tr('The upstream source may be temporarily unavailable. Try again later.', '上游直播源可能暂时不可用，请稍后重试。')
                : tr('The stream CDN may be unreachable from this network. Try another channel or network.', '当前网络可能无法访问该直播 CDN，请更换频道或网络后重试。')}
            </Alert.Description>
          </Alert.Content>
        </Alert>
      )}

      <ChannelMeta channel={channel} nowPlaying={nowPlaying} />
    </div>
  );
}

function ChannelHeading({ channel }: { channel: IptvChannel }) {
  const [logoFailed, setLogoFailed] = useState(false);
  return (
    <div className="flex min-w-0 items-center gap-3">
      <div className="flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-surface-secondary">
        {logoFailed ? (
          <Tv aria-hidden="true" className="size-5 text-muted" />
        ) : (
          <img
            alt=""
            className="size-10 object-contain"
            onError={() => { setLogoFailed(true); }}
            src={`${IPTV_LOGO_BASE}/${channel.slug}.png`}
          />
        )}
      </div>
      <h1 className="truncate text-xl font-semibold text-foreground">{channel.name}</h1>
    </div>
  );
}

function ChannelMeta({ channel, nowPlaying }: { channel: IptvChannel; nowPlaying?: string }) {
  const tr = useTranslate();
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-1.5">
        <Chip size="sm" variant="secondary">{iptvChannelGroup(channel.slug)}</Chip>
        <Chip size="sm" variant="secondary">{channel.defn.toUpperCase()}</Chip>
        {channel.timeshift && (
          <Chip size="sm" variant="soft">
            <History aria-hidden="true" className="size-3" />
            {tr('7-day replay', '7 天回看')}
          </Chip>
        )}
      </div>
      {nowPlaying && (
        <p className="text-sm text-muted">
          {tr('Now playing:', '正在播放：')}{nowPlaying}
        </p>
      )}
    </div>
  );
}

function UnsupportedNotice({ channel }: { channel: IptvChannel }) {
  const tr = useTranslate();
  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <Link
          aria-label={tr('Back to channels', '返回频道列表')}
          className="inline-flex size-9 items-center justify-center rounded-lg text-muted transition-colors hover:bg-surface-secondary hover:text-foreground"
          to="/app/iptv"
        >
          <ArrowLeft className="size-4" />
        </Link>
        <ChannelHeading channel={channel} />
      </div>
      <Alert role="alert" status="warning">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Title>{tr('IPTV needs the app', 'IPTV 需要应用环境')}</Alert.Title>
          <Alert.Description>
            {tr(
              'Channel resolution is unavailable in a plain browser. Open this page in the mobile or desktop app.',
              '浏览器环境无法解析频道地址，请在移动端或桌面端应用中打开此页面。',
            )}
          </Alert.Description>
        </Alert.Content>
      </Alert>
    </div>
  );
}
