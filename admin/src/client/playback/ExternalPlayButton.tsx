import { Button, ButtonGroup, Dropdown, Toast } from '@heroui/react';
import { ChevronDown, Copy, ExternalLink, LoaderCircle, Play, RefreshCw } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslate } from '../../settings/i18n';
import {
  buildExternalPlayerUrl,
  createExternalPlaybackLink,
  detectExternalPlayerPlatform,
  externalPlayersForPlatform,
  ExternalPlaybackUnavailableError,
  openExternalPlayer,
  type ExternalPlaybackLink,
} from './externalPlayback';
import { prefetchPlaybackInfo } from './playbackInfoCache';

type PreparationState = 'idle' | 'loading' | 'ready' | 'no-source' | 'error';
const EXPIRY_BUFFER_MS = 30_000;

export function ExternalPlayButton({
  itemId,
  itemTitle,
  isExternalPlaybackDisabled = false,
  onPlay,
  prefetchOnMount = false,
}: {
  itemId: string;
  itemTitle: string;
  isExternalPlaybackDisabled?: boolean;
  onPlay: () => void;
  /** Start PlaybackInfo immediately, e.g. when the item's sources still need a server-side probe. */
  prefetchOnMount?: boolean;
}) {
  const tr = useTranslate();
  const platform = useMemo(() => detectExternalPlayerPlatform(), []);
  const players = useMemo(() => externalPlayersForPlatform(platform), [platform]);
  const requestIdRef = useRef(0);
  const [playbackLink, setPlaybackLink] = useState<ExternalPlaybackLink>();
  const [preparationState, setPreparationState] = useState<PreparationState>('idle');

  useEffect(() => () => {
    requestIdRef.current += 1;
  }, []);

  useEffect(() => {
    if (!prefetchOnMount || isExternalPlaybackDisabled) return;
    // Unprobed sources make the first PlaybackInfo wait for a probe, so start it
    // on load. A 503 "preparing" answer keeps retrying inside the shared bounded
    // retry. The request is tied to this page and cancelled on unmount, unless
    // user intent (hover/focus/press) pinned it. Failed entries are evicted, so
    // the play path simply retries.
    const controller = new AbortController();
    prefetchPlaybackInfo(itemId, controller.signal);
    return () => {
      controller.abort();
    };
  }, [isExternalPlaybackDisabled, itemId, prefetchOnMount]);

  const prepare = async () => {
    if (isExternalPlaybackDisabled || preparationState === 'loading') return;
    if (playbackLink && Date.parse(playbackLink.expiresAt) - Date.now() > EXPIRY_BUFFER_MS) {
      setPreparationState('ready');
      return;
    }
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    setPreparationState('loading');
    try {
      const nextLink = await createExternalPlaybackLink(itemId);
      if (requestIdRef.current !== requestId) return;
      setPlaybackLink(nextLink);
      setPreparationState('ready');
    } catch (error) {
      if (requestIdRef.current !== requestId) return;
      setPlaybackLink(undefined);
      setPreparationState(error instanceof ExternalPlaybackUnavailableError ? 'no-source' : 'error');
    }
  };

  // Warm PlaybackInfo (never a ticket) once the user shows intent to play.
  const prefetch = () => {
    if (!isExternalPlaybackDisabled) prefetchPlaybackInfo(itemId);
  };

  const handleAction = async (key: React.Key) => {
    if (key === 'retry') {
      await prepare();
      return;
    }
    if (!playbackLink || preparationState !== 'ready') return;
    if (key === 'copy') {
      try {
        await navigator.clipboard.writeText(playbackLink.streamUrl);
        Toast.toast.success(tr('Temporary playback link copied.', '临时播放链接已复制。'));
      } catch {
        Toast.toast.danger(tr('The playback link could not be copied.', '无法复制播放链接。'));
      }
      return;
    }
    const player = players.find((option) => option.id === key);
    if (!player) return;
    try {
      openExternalPlayer(buildExternalPlayerUrl(player.id, playbackLink.streamUrl, itemTitle, platform));
    } catch {
      Toast.toast.danger(tr(`Unable to open ${player.label}.`, `无法打开 ${player.label}。`));
    }
  };

  return (
    <ButtonGroup>
      <Button onFocus={prefetch} onHoverStart={prefetch} onPress={() => { prefetch(); onPlay(); }}>
        <Play aria-hidden="true" className="size-4" />
        {tr('Play', '播放')}
      </Button>
      <Dropdown onOpenChange={(isOpen) => { if (isOpen) void prepare(); }}>
        <Button
          aria-label={tr('More playback options', '更多播放选项')}
          isDisabled={isExternalPlaybackDisabled}
          isIconOnly
          onFocus={prefetch}
          onHoverStart={prefetch}
        >
          <ButtonGroup.Separator />
          <ChevronDown aria-hidden="true" className="size-4" />
        </Button>
        <Dropdown.Popover placement="bottom start">
          <Dropdown.Menu
            aria-label={tr('Playback options', '播放选项')}
            onAction={(key) => { void handleAction(key); }}
          >
            {preparationState === 'loading' && (
              <Dropdown.Item id="loading" isDisabled textValue={tr('Preparing playback link', '正在准备播放链接')}>
                <LoaderCircle aria-hidden="true" className="size-4 animate-spin" />
                {tr('Preparing playback link…', '正在准备播放链接…')}
              </Dropdown.Item>
            )}
            {preparationState === 'no-source' && (
              <Dropdown.Item id="no-source" isDisabled textValue={tr('No compatible media source', '没有兼容的媒体源')}>
                {tr('No compatible media source', '没有兼容的媒体源')}
              </Dropdown.Item>
            )}
            {preparationState === 'error' && (
              <Dropdown.Item id="retry" textValue={tr('Retry preparing link', '重试准备链接')}>
                <RefreshCw aria-hidden="true" className="size-4" />
                {tr('Retry preparing link', '重试准备链接')}
              </Dropdown.Item>
            )}
            {preparationState === 'ready' && (
              <>
                <Dropdown.Item id="copy" textValue={tr('Copy temporary playback link', '复制临时播放直链')}>
                  <Copy aria-hidden="true" className="size-4" />
                  {tr('Copy temporary playback link', '复制临时播放直链')}
                </Dropdown.Item>
                {players.map((player) => (
                  <Dropdown.Item id={player.id} key={player.id} textValue={tr(`Open in ${player.label}`, `使用 ${player.label} 打开`)}>
                    <ExternalLink aria-hidden="true" className="size-4" />
                    {tr(`Open in ${player.label}`, `使用 ${player.label} 打开`)}
                  </Dropdown.Item>
                ))}
              </>
            )}
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
    </ButtonGroup>
  );
}
