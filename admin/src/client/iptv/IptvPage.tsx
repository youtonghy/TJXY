import { Chip, Tabs } from '@heroui/react';
import { EmptyState } from '@heroui-pro/react';
import { History, Tv } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { Link } from 'react-router-dom';
import { useTranslate } from '../../settings/i18n';
import { PageHeader } from '../../ui/PageHeader';
import { isIptvSupportedShell } from './iptvApi';
import { IPTV_CHANNELS, IPTV_LOGO_BASE, iptvChannelGroup, type IptvChannel } from './iptvChannels';
import { getIptvDeviceEngine } from './iptvDevice';
import { loadIptvGuide, type IptvProgramme } from './iptvEpg';

type IptvGroup = 'cctv' | 'satellite' | 'digital';

const GROUP_LABELS: Record<IptvGroup, [string, string]> = {
  cctv: ['CCTV', '央视频道'],
  satellite: ['Satellite', '卫视频道'],
  digital: ['Digital', '数字频道'],
};

function groupOf(channel: IptvChannel): IptvGroup {
  const group = iptvChannelGroup(channel.slug);
  if (group === '卫视频道') return 'satellite';
  if (group === '数字频道') return 'digital';
  return 'cctv';
}

export function IptvPage() {
  const tr = useTranslate();
  const supported = isIptvSupportedShell();
  const [group, setGroup] = useState<IptvGroup>('cctv');
  const [guide, setGuide] = useState<Map<string, IptvProgramme>>();

  useEffect(() => {
    if (!supported) return;
    let active = true;
    void loadIptvGuide().then((programmes) => {
      if (active) setGuide(programmes);
    });
    // Warm the device-protocol session (~15 paced requests) while the user
    // browses the grid so the first playback starts instantly.
    getIptvDeviceEngine().prewarm();
    return () => { active = false; };
  }, [supported]);

  const channels = useMemo(
    () => IPTV_CHANNELS.filter((channel) => groupOf(channel) === group),
    [group],
  );

  if (!supported) {
    return (
      <div className="space-y-6">
        <PageHeader
          description={tr('CCTV, CGTN and satellite channels resolved on-device.', '在本地解析的央视、CGTN 与卫视直播频道。')}
          title="IPTV"
        />
        <EmptyState className="min-h-64 border border-dashed border-separator">
          <EmptyState.Media variant="icon"><Tv /></EmptyState.Media>
          <EmptyState.Title>{tr('IPTV needs the app', 'IPTV 需要应用环境')}</EmptyState.Title>
          <EmptyState.Description>
            {tr(
              'Channel resolution is unavailable in a plain browser. Open this page in the mobile or desktop app.',
              '浏览器环境无法解析频道地址，请在移动端或桌面端应用中打开此页面。',
            )}
          </EmptyState.Description>
        </EmptyState>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <PageHeader
        description={tr('CCTV, CGTN and satellite channels resolved on-device.', '在本地解析的央视、CGTN 与卫视直播频道。')}
        title="IPTV"
      />
      <Tabs
        onSelectionChange={(key) => { if (key === 'cctv' || key === 'satellite' || key === 'digital') setGroup(key); }}
        selectedKey={group}
      >
        <Tabs.ListContainer className="w-full sm:w-fit">
          <Tabs.List className="grid w-full grid-cols-3 sm:min-w-96">
            {(Object.keys(GROUP_LABELS) as IptvGroup[]).map((id) => (
              <Tabs.Tab className="h-10 gap-2 whitespace-nowrap px-4" id={id} key={id}>
                {tr(...GROUP_LABELS[id])}
                <Tabs.Indicator />
              </Tabs.Tab>
            ))}
          </Tabs.List>
        </Tabs.ListContainer>
      </Tabs>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
        {channels.map((channel) => (
          <ChannelCard
            channel={channel}
            key={channel.slug}
            nowPlaying={channel.tvgId ? guide?.get(channel.tvgId)?.title : undefined}
          />
        ))}
      </div>
    </div>
  );
}

function ChannelCard({ channel, nowPlaying }: { channel: IptvChannel; nowPlaying?: string }) {
  const tr = useTranslate();
  const [logoFailed, setLogoFailed] = useState(false);
  return (
    <Link
      className="group flex flex-col gap-3 rounded-xl border border-default/20 bg-surface p-4 transition-colors hover:border-accent/40 hover:bg-surface-secondary focus-visible:outline-2 focus-visible:outline-accent"
      to={`/app/iptv/${channel.slug}`}
    >
      <div className="flex items-center gap-3">
        <div className="flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-surface-secondary">
          {logoFailed ? (
            <Tv aria-hidden="true" className="size-5 text-muted" />
          ) : (
            <img
              alt=""
              className="size-10 object-contain"
              loading="lazy"
              onError={() => { setLogoFailed(true); }}
              src={`${IPTV_LOGO_BASE}/${channel.slug}.png`}
            />
          )}
        </div>
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-foreground">{channel.name}</p>
          <p className="truncate text-xs text-muted">
            {nowPlaying ? tr('Now:', '正在播放：') + nowPlaying : iptvChannelGroup(channel.slug)}
          </p>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        <Chip size="sm" variant="secondary">{channel.defn.toUpperCase()}</Chip>
        {channel.timeshift && (
          <Chip size="sm" variant="soft">
            <History aria-hidden="true" className="size-3" />
            {tr('7-day replay', '7 天回看')}
          </Chip>
        )}
      </div>
    </Link>
  );
}
