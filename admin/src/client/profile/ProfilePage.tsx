import {
  Avatar,
  Button,
  Card,
  Input,
  Label,
  Modal,
  TextArea,
  TextField,
} from '@heroui/react';
import { BarChart } from '@heroui-pro/react/bar-chart';
import { AreaChart } from '@heroui-pro/react/area-chart';
import { KPI } from '@heroui-pro/react/kpi';
import { KPIGroup } from '@heroui-pro/react/kpi-group';
import { PieChart } from '@heroui-pro/react/pie-chart';
import { Timeline } from '@heroui-pro/react/timeline';
import { CheckCircle2, Clock3, Film, Pencil, Play, Tags, Tv } from 'lucide-react';
import { useEffect, useState, type SyntheticEvent } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import {
  PolarAngleAxis,
  PolarGrid,
  PolarRadiusAxis,
  Radar,
  RadarChart,
  ResponsiveContainer,
  Tooltip as ChartTooltip,
} from 'recharts';
import {
  getProfile,
  getUserInsights,
  listPersonalSessions,
  revokePersonalSession,
  updateProfile,
  type InsightRange,
  type InsightTimelineEvent,
  type UserInsights,
  type UserProfile,
  type PersonalSession,
} from '../api/portalApi';
import { useClientAuth } from '../auth/ClientAuthContext';
import { getStoredApiBaseUrl, isDesktopShell, probeServer, setApiBaseUrl } from '../api/apiBase';
import { ServerAddressField } from '../ui/ServerAddressField';
import { useTranslate } from '../../settings/i18n';
import { useSystemLocale } from '../../settings/SystemLocaleProvider';

const ranges: { key: InsightRange; label: string }[] = [
  { key: 'today', label: 'Today' },
  { key: '7d', label: '7 days' },
  { key: '30d', label: '30 days' },
  { key: 'all', label: 'All time' },
];

export function ProfilePage() {
  const navigate = useNavigate();
  const { signOut } = useClientAuth();
  const [profile, setProfile] = useState<UserProfile>();
  const [insights, setInsights] = useState<UserInsights>();
  const [sessions, setSessions] = useState<PersonalSession[]>([]);
  const [sessionsLoading, setSessionsLoading] = useState(true);
  const [sessionBusy, setSessionBusy] = useState<string | null>(null);
  const [range, setRange] = useState<InsightRange>('today');
  const [editing, setEditing] = useState(false);
  const [server, setServer] = useState(getStoredApiBaseUrl() ?? 'http://127.0.0.1:8096');
  const [serverPending, setServerPending] = useState(false);
  const [serverError, setServerError] = useState<string>();
  const [serverOk, setServerOk] = useState(false);
  const tr = useTranslate();
  const { locale } = useSystemLocale();

  useEffect(() => { void getProfile().then(setProfile); }, []);
  useEffect(() => {
    const controller = new AbortController();
    void listPersonalSessions(controller.signal).then(setSessions).catch(() => undefined).finally(() => {
      if (!controller.signal.aborted) setSessionsLoading(false);
    });
    return () => { controller.abort(); };
  }, []);
  useEffect(() => { void getUserInsights(range).then(setInsights); }, [range]);

  if (!profile) return <div aria-label={tr('Loading profile', '正在加载个人资料')} className="h-52 animate-pulse rounded-2xl bg-default" role="status" />;
  return (
    <div className="space-y-8">
      <Card className="overflow-hidden p-0">
        <Card.Content className="flex flex-col gap-5 p-6 sm:flex-row sm:items-center sm:p-8">
          <Avatar className="size-20 text-2xl"><Avatar.Fallback>{profile.Username.slice(0, 1).toUpperCase()}</Avatar.Fallback></Avatar>
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-accent">{tr('Your account', '你的账户')}</p>
            <h1 className="mt-1 text-3xl font-semibold">{profile.Username}</h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-muted">{profile.Bio || tr('Add a short introduction about yourself.', '添加一段简短的自我介绍。')}</p>
          </div>
          <div className="flex flex-wrap gap-2"><Button onPress={() => { void navigate('/app/profile/authorize'); }} variant="secondary">{tr('Authorize device', '授权设备')}</Button><Button onPress={() => { setEditing(true); }} variant="secondary"><Pencil className="size-4" />{tr('Edit profile', '编辑个人资料')}</Button></div>
        </Card.Content>
      </Card>

      {isDesktopShell() && (
        <Card>
          <Card.Header>
            <Card.Title>{tr('Server', '服务器')}</Card.Title>
            <Card.Description>{tr('Changing the server address signs you out.', '更改服务器地址会退出当前登录。')}</Card.Description>
          </Card.Header>
          <Card.Content>
            <ServerAddressField
              error={serverError}
              ok={serverOk}
              pending={serverPending}
              value={server}
              onChange={(next) => { setServer(next); setServerOk(false); setServerError(undefined); }}
              onSave={() => {
                setServerPending(true);
                setServerError(undefined);
                void probeServer(server)
                  .then(async (origin) => {
                    setApiBaseUrl(origin);
                    setServer(origin);
                    setServerOk(true);
                    await signOut();
                    void navigate('/app/login', { replace: true });
                  })
                  .catch(() => {
                    setServerError(tr('Could not reach that server.', '无法连接到该服务器。'));
                  })
                  .finally(() => { setServerPending(false); });
              }}
            />
          </Card.Content>
        </Card>
      )}

      <SessionManagement
        loading={sessionsLoading}
        sessions={sessions}
        busy={sessionBusy}
        onRevoke={async (session) => {
          if (sessionBusy !== null) return;
          setSessionBusy(session.Id);
          try {
            await revokePersonalSession(session.Id);
            if (session.IsCurrent) {
              await signOut();
              void navigate('/app/login', { replace: true });
              return;
            }
            setSessions((current) => current.filter((item) => item.Id !== session.Id));
          } finally {
            setSessionBusy(null);
          }
        }}
      />

      <section className="space-y-5" aria-labelledby="statistics-heading">
        <div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end">
          <div><h2 className="text-2xl font-semibold" id="statistics-heading">{tr('Viewing statistics', '观看统计')}</h2><p className="mt-1 text-sm text-muted">{tr('Your activity across the selected period.', '所选时间范围内的观看活动。')}</p></div>
          <div aria-label={tr('Statistics period', '统计时间范围')} className="flex flex-wrap gap-2" role="group">
            {ranges.map((item) => <Button key={item.key} onPress={() => { setRange(item.key); }} size="sm" variant={range === item.key ? 'primary' : 'secondary'}>{tr(item.label, ({ Today: '今天', '7 days': '7 天', '30 days': '30 天', 'All time': '全部时间' } as Record<string, string>)[item.label] ?? item.label)}</Button>)}
          </div>
        </div>
        <div aria-label={tr('Viewing KPIs', '观看指标')} className="grid gap-3 lg:grid-cols-2" role="group">
          <KPIGroup aria-label={tr('Viewing totals', '观看总量')}>
            <InsightKpi icon={Clock3} label={tr('Watch time', '观看时长')} value={formatTicks(insights?.WatchedTicks, locale)} />
            <KPIGroup.Separator />
            <InsightKpi icon={Play} label={tr('Playback starts', '播放次数')} value={String(insights?.PlayCount ?? 0)} />
          </KPIGroup>
          <KPIGroup aria-label={tr('Viewing variety', '观看内容分布')}>
            <InsightKpi icon={Film} label={tr('Unique titles', '观看内容数')} value={String(insights?.UniqueTitles ?? 0)} />
            <KPIGroup.Separator />
            <InsightKpi icon={Tags} label={tr('Top genre', '最常看类型')} value={insights?.Genres[0]?.Name ?? tr('No activity', '暂无活动')} />
          </KPIGroup>
        </div>
        <div className="grid gap-4 lg:grid-cols-2">
          <Card><Card.Header><Card.Title>{tr('Daily watch time', '每日观看时长')}</Card.Title><Card.Description>{tr('Minutes watched by day.', '每天观看的分钟数。')}</Card.Description></Card.Header><Card.Content><DailyWatchChart insights={insights} /></Card.Content></Card>
          <Card><Card.Header><Card.Title>{tr('Genre mix', '类型分布')}</Card.Title><Card.Description>{tr('Genres receiving the most watch time.', '观看时间最多的内容类型。')}</Card.Description></Card.Header><Card.Content><GenreRadar insights={insights} /></Card.Content></Card>
          <Card>
            <Card.Header><Card.Title>{tr('Movies and series', '电影与剧集')}</Card.Title><Card.Description>{tr('Playback starts grouped by media type.', '按媒体类型统计播放次数。')}</Card.Description></Card.Header>
            <Card.Content><MediaTypePieChart insights={insights} /></Card.Content>
          </Card>
          <Card><Card.Header><Card.Title>{tr('Cumulative watch time', '累计观看时长')}</Card.Title><Card.Description>{tr('Watch time accumulated across the selected period.', '所选时间范围内累计的观看时长。')}</Card.Description></Card.Header><Card.Content><CumulativeWatchChart insights={insights} /></Card.Content></Card>
        </div>
        <ViewingTimeline events={insights?.Timeline ?? []} />
      </section>
      {editing ? <ProfileDialog profile={profile} onClose={() => { setEditing(false); }} onSaved={setProfile} onSessionInvalidated={async () => { await signOut(); void navigate('/app/login?redirect=%2Fapp%2Fprofile', { replace: true }); }} /> : null}
    </div>
  );
}

function SessionManagement({
  loading,
  sessions,
  busy,
  onRevoke,
}: {
  loading: boolean;
  sessions: PersonalSession[];
  busy: string | null;
  onRevoke: (session: PersonalSession) => Promise<void>;
}) {
  const tr = useTranslate();
  return (
    <Card>
      <Card.Header>
        <Card.Title>{tr('Signed-in devices', '已登录设备')}</Card.Title>
        <Card.Description>{tr('Review active sessions and sign out older logins.', '查看当前会话并注销较早的登录。')}</Card.Description>
      </Card.Header>
      <Card.Content>
        {loading ? <div aria-label={tr('Loading signed-in devices', '正在加载已登录设备')} className="h-24 animate-pulse rounded-xl bg-default" role="status" /> : sessions.length === 0 ? (
          <p className="py-6 text-sm text-muted">{tr('No active sessions.', '暂无活跃会话。')}</p>
        ) : (
          <ul aria-label={tr('Signed-in devices', '已登录设备')} className="divide-y divide-border">
            {sessions.map((session) => (
              <li className="flex flex-col gap-3 py-4 sm:flex-row sm:items-center sm:justify-between" key={session.Id}>
                <div className="min-w-0">
                  <p className="font-medium text-foreground">{session.DeviceName} {session.IsCurrent ? <span className="text-xs text-accent">({tr('This device', '此设备')})</span> : null}</p>
                  <p className="text-sm text-muted">{session.ClientName} · {session.ApplicationVersion}</p>
                  <p className="text-xs text-muted">{tr('Last active', '最近活动')}：{formatTimelineDate(session.LastActivityDate, localeForDate())}</p>
                </div>
                <Button
                  isDisabled={busy !== null}
                  isPending={busy === session.Id}
                  onPress={() => { void onRevoke(session); }}
                  size="sm"
                  variant={session.IsCurrent ? 'tertiary' : 'danger-soft'}
                >{session.IsCurrent ? tr('Sign out', '退出登录') : tr('Revoke', '注销')}</Button>
              </li>
            ))}
          </ul>
        )}
      </Card.Content>
    </Card>
  );
}

function localeForDate(): string {
  return document.documentElement.lang || 'en-US';
}

function InsightKpi({ icon: Icon, label, value }: { icon: typeof Clock3; label: string; value: string }) {
  return <KPI><KPI.Header><KPI.Icon className="bg-accent/10 text-accent"><Icon aria-hidden="true" className="size-4" /></KPI.Icon><KPI.Title>{label}</KPI.Title></KPI.Header><KPI.Content><p className="truncate text-2xl font-semibold tabular-nums text-foreground">{value}</p></KPI.Content></KPI>;
}

function DailyWatchChart({ insights }: { insights?: UserInsights }) {
  const tr = useTranslate();
  const data = insights?.Daily.map((point) => ({ date: point.Date.slice(5), minutes: Math.round(point.WatchedTicks / 600_000_000) })) ?? [];
  if (!data.length) return <p className="py-20 text-center text-sm text-muted">{tr('No activity in this period.', '此时间范围内暂无活动。')}</p>;
  return (
    <figure aria-label={tr('Daily watch time bar chart', '每日观看时长柱状图')} role="img">
      <BarChart data={data} height={220}>
        <BarChart.Grid vertical={false} />
        <BarChart.XAxis dataKey="date" tickMargin={8} />
        <BarChart.YAxis tickMargin={4} width={36} />
        <BarChart.Bar barSize={18} dataKey="minutes" fill="var(--color-accent)" name={tr('Minutes watched', '观看分钟数')} radius={[4, 4, 0, 0]} />
        <BarChart.Tooltip content={<BarChart.TooltipContent valueFormatter={(value) => tr(`${String(value)} min`, `${String(value)} 分钟`)} />} />
      </BarChart>
      <figcaption className="sr-only">{data.map((point) => tr(`${point.date}: ${String(point.minutes)} minutes`, `${point.date}：${String(point.minutes)} 分钟`)).join('; ')}</figcaption>
    </figure>
  );
}

function MediaTypePieChart({ insights }: { insights?: UserInsights }) {
  const tr = useTranslate();
  const data = [
    { color: 'var(--color-accent)', label: tr('movies', '电影'), value: insights?.Media.Movies ?? 0 },
    { color: 'var(--color-warning)', label: tr('series', '剧集'), value: insights?.Media.Series ?? 0 },
  ];
  const total = data.reduce((sum, item) => sum + item.value, 0);
  return (
    <figure aria-label={tr('Movies and series pie chart', '电影与剧集饼图')} className="min-w-0" role="img">
      {total > 0 ? (
        <PieChart height={220}>
          <PieChart.Pie data={data} dataKey="value" innerRadius={54} nameKey="label" outerRadius={82} paddingAngle={3} stroke="var(--color-surface)" strokeWidth={3}>
            {data.map((item) => <PieChart.Cell fill={item.color} key={item.label} />)}
          </PieChart.Pie>
          <PieChart.Tooltip content={<PieChart.TooltipContent valueFormatter={(value) => tr(`${String(value)} plays`, `${String(value)} 次播放`)} />} />
        </PieChart>
      ) : <p className="py-16 text-center text-sm text-muted">{tr('No media activity in this period.', '此时间范围内暂无媒体数据。')}</p>}
      <figcaption className="mt-1 grid grid-cols-2 gap-3">
        {data.map((item) => (
          <div aria-label={`${String(item.value)} ${item.label}`} className="flex items-center gap-2 rounded-xl bg-default px-3 py-2" key={item.label}>
            <span aria-hidden="true" className="size-2.5 rounded-full" style={{ backgroundColor: item.color }} />
            <span className="text-sm text-muted">{item.label}</span>
            <strong className="ml-auto tabular-nums text-foreground">{item.value}</strong>
          </div>
        ))}
      </figcaption>
    </figure>
  );
}

function CumulativeWatchChart({ insights }: { insights?: UserInsights }) {
  const tr = useTranslate();
  const { locale } = useSystemLocale();
  const axisFormatter = new Intl.NumberFormat(locale, { maximumFractionDigits: 1, notation: 'compact' });
  const data = [...(insights?.Daily ?? [])].sort((left, right) => left.Date.localeCompare(right.Date)).reduce<{ date: string; minutes: number; ticks: number }[]>((points, point) => {
    const cumulativeTicks = (points.at(-1)?.ticks ?? 0) + point.WatchedTicks;
    points.push({ date: point.Date.slice(5), minutes: Math.round(cumulativeTicks / 600_000_000), ticks: cumulativeTicks });
    return points;
  }, []);
  if (!data.length) return <p className="py-20 text-center text-sm text-muted">{tr('No activity in this period.', '此时间范围内暂无活动。')}</p>;
  return (
    <figure aria-label={tr('Cumulative watch time area chart', '累计观看时长面积图')} role="img">
      <AreaChart data={data} height={220}>
        <AreaChart.Grid vertical={false} />
        <AreaChart.XAxis dataKey="date" tickMargin={8} />
        <AreaChart.YAxis tickFormatter={(value) => axisFormatter.format(Number(value))} tickMargin={4} width={42} />
        <AreaChart.Area dataKey="minutes" fill="var(--color-accent)" fillOpacity={0.18} name={tr('Cumulative minutes', '累计分钟数')} stroke="var(--color-accent)" strokeWidth={2} type="monotone" />
        <AreaChart.Tooltip content={<AreaChart.TooltipContent valueFormatter={(value) => tr(`${String(value)} min`, `${String(value)} 分钟`)} />} />
      </AreaChart>
      <figcaption className="sr-only">{data.map((point) => `${point.date}: ${formatTicks(point.ticks, locale)}`).join('; ')}</figcaption>
    </figure>
  );
}

function ViewingTimeline({ events }: { events: InsightTimelineEvent[] }) {
  const tr = useTranslate();
  const { locale } = useSystemLocale();
  return (
    <Card>
      <Card.Header><Card.Title>{tr('Viewing timeline', '观影时间线')}</Card.Title><Card.Description>{tr('Milestones from your playback history.', '播放历史中的重要节点。')}</Card.Description></Card.Header>
      <Card.Content>
        {events.length ? (
          <Timeline density="compact" size="sm">
            {events.map((event) => {
              const presentation = timelinePresentation(event, tr);
              const Icon = presentation.icon;
              return (
                <Timeline.Item key={`${event.Kind}-${event.ItemId}-${event.At}`} status={presentation.status}>
                  <Timeline.Marker aria-hidden="true"><Icon /></Timeline.Marker>
                  <Timeline.Content>
                    <div className="flex min-w-0 flex-col gap-1 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
                      <p className="min-w-0 text-sm"><span className="text-muted">{presentation.prefix} </span><Link className="font-medium text-foreground hover:text-accent" to={`/app/items/${event.ItemId}`}>{event.Name}</Link></p>
                      <time className="shrink-0 text-xs text-muted" dateTime={event.At}>{formatTimelineDate(event.At, locale)}</time>
                    </div>
                  </Timeline.Content>
                </Timeline.Item>
              );
            })}
          </Timeline>
        ) : <p className="py-8 text-center text-sm text-muted">{tr('No viewing milestones in this period.', '此时间范围内暂无观影节点。')}</p>}
      </Card.Content>
    </Card>
  );
}

function timelinePresentation(event: InsightTimelineEvent, tr: (english: string, chinese: string) => string) {
  if (event.Kind === 'SeriesCompleted') return { icon: CheckCircle2, prefix: tr('Finished', '看完了'), status: 'success' as const };
  if (event.Kind === 'SeriesStarted') return { icon: Tv, prefix: tr('Started', '开始看'), status: 'current' as const };
  return { icon: Film, prefix: tr('Watched', '看了'), status: 'default' as const };
}

function formatTimelineDate(value: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value));
}

function GenreRadar({ insights }: { insights?: UserInsights }) {
  const tr = useTranslate();
  const { locale } = useSystemLocale();
  const genres = insights?.Genres.slice(0, 6) ?? [];
  const maximum = Math.max(0, ...genres.map((genre) => genre.WatchedTicks));
  if (!genres.length) return <p className="py-16 text-center text-sm text-muted">{tr('No genre activity in this period.', '此时间范围内暂无类型数据。')}</p>;
  const scale = Math.max(1, maximum);
  const data = genres.map((genre) => ({
    genre: genre.Name,
    score: Math.round((genre.WatchedTicks / scale) * 100),
    watched: formatTicks(genre.WatchedTicks, locale),
  }));
  return (
    <figure aria-label={tr('Genre watch time radar chart', '类型观看时长雷达图')} className="min-w-0" role="img">
      <div aria-hidden="true" className="h-64 min-w-0">
        <ResponsiveContainer height="100%" minHeight={256} minWidth={240} width="100%">
          <RadarChart data={data} margin={{ bottom: 12, left: 24, right: 24, top: 12 }}>
            <PolarGrid stroke="var(--color-border)" />
            <PolarAngleAxis dataKey="genre" tick={{ fill: 'var(--color-muted)', fontSize: 12 }} />
            <PolarRadiusAxis angle={90} domain={[0, 100]} tick={false} />
            <Radar dataKey="score" dot={{ fill: 'var(--color-accent)', r: 3 }} fill="var(--color-accent)" fillOpacity={0.22} name={tr('Relative watch time', '相对观看时长')} stroke="var(--color-accent)" strokeWidth={2} />
            <ChartTooltip formatter={(value) => [`${String(value)}%`, tr('Relative watch time', '相对观看时长')]} />
          </RadarChart>
        </ResponsiveContainer>
      </div>
      <figcaption className="sr-only">{data.map((genre) => `${genre.genre}: ${genre.watched}`).join('; ')}</figcaption>
    </figure>
  );
}

function ProfileDialog({ profile, onClose, onSaved, onSessionInvalidated }: { profile: UserProfile; onClose: () => void; onSaved: (profile: UserProfile) => void; onSessionInvalidated: () => Promise<void> }) {
  const tr = useTranslate();
  const [username, setUsername] = useState(profile.Username);
  const [bio, setBio] = useState(profile.Bio);
  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [pending, setPending] = useState(false);
  const [error, setError] = useState('');

  const submit = async (event: SyntheticEvent<HTMLFormElement, SubmitEvent>) => {
    event.preventDefault();
    if (pending || (newPassword && newPassword !== confirmPassword)) return;
    setPending(true);
    setError('');
    try {
      const updated = await updateProfile({
        Bio: bio.trim(),
        CurrentPassword: currentPassword,
        NewPassword: newPassword || undefined,
        Username: username.trim(),
      });
      onSaved(updated);
      onClose();
      if (newPassword || username.trim() !== profile.Username) await onSessionInvalidated();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : tr('Unable to update this account.', '无法更新此账户。'));
    } finally { setPending(false); }
  };
  return <Modal isOpen onOpenChange={(open) => { if (!open) onClose(); }}><Modal.Backdrop isDismissable={!pending}><Modal.Container placement="center" size="md"><Modal.Dialog><Modal.CloseTrigger aria-label={tr('Close', '关闭')} isDisabled={pending} /><Modal.Header><Modal.Heading>{tr('Edit account', '编辑账户')}</Modal.Heading></Modal.Header><Modal.Body><form className="space-y-5" id="profile-form" onSubmit={(event) => { void submit(event); }}><TextField fullWidth isRequired><Label>{tr('Username', '用户名')}</Label><Input autoComplete="username" value={username} onChange={(event) => { setUsername(event.currentTarget.value); }} /></TextField><TextField fullWidth><Label>{tr('Biography', '个人简介')}</Label><TextArea maxLength={500} value={bio} onChange={(event) => { setBio(event.currentTarget.value); }} /></TextField><div className="border-t border-border pt-5"><p className="mb-4 text-sm font-medium">{tr('Security confirmation', '安全确认')}</p><div className="space-y-4"><TextField fullWidth isRequired><Label>{tr('Current password', '当前密码')}</Label><Input autoComplete="current-password" type="password" value={currentPassword} onChange={(event) => { setCurrentPassword(event.currentTarget.value); }} /></TextField><TextField fullWidth><Label>{tr('New password', '新密码')}</Label><Input autoComplete="new-password" type="password" value={newPassword} onChange={(event) => { setNewPassword(event.currentTarget.value); }} /></TextField><TextField fullWidth isInvalid={Boolean(confirmPassword && confirmPassword !== newPassword)}><Label>{tr('Confirm new password', '确认新密码')}</Label><Input autoComplete="new-password" type="password" value={confirmPassword} onChange={(event) => { setConfirmPassword(event.currentTarget.value); }} /></TextField></div></div>{error ? <p className="text-sm text-danger" role="alert">{error}</p> : null}</form></Modal.Body><Modal.Footer><Button onPress={onClose} variant="tertiary">{tr('Cancel', '取消')}</Button><Button form="profile-form" isPending={pending} type="submit">{tr('Save changes', '保存更改')}</Button></Modal.Footer></Modal.Dialog></Modal.Container></Modal.Backdrop></Modal>;
}

function formatTicks(value = 0, locale = 'en-US'): string {
  const minutes = Math.round(value / 600_000_000);
  if (locale === 'zh-CN') return minutes >= 60 ? `${String(Math.floor(minutes / 60))} 小时 ${String(minutes % 60)} 分钟` : `${String(minutes)} 分钟`;
  return minutes >= 60 ? `${String(Math.floor(minutes / 60))}h ${String(minutes % 60)}m` : `${String(minutes)}m`;
}
