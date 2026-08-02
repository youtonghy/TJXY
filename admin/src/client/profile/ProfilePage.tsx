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
import { KPI } from '@heroui-pro/react/kpi';
import { KPIGroup } from '@heroui-pro/react/kpi-group';
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
  updateProfile,
  type InsightRange,
  type InsightTimelineEvent,
  type UserInsights,
  type UserProfile,
} from '../api/portalApi';
import { useClientAuth } from '../auth/ClientAuthContext';

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
  const [range, setRange] = useState<InsightRange>('today');
  const [editing, setEditing] = useState(false);

  useEffect(() => { void getProfile().then(setProfile); }, []);
  useEffect(() => { void getUserInsights(range).then(setInsights); }, [range]);

  if (!profile) return <div aria-label="Loading profile" className="h-52 animate-pulse rounded-2xl bg-default" role="status" />;
  return (
    <div className="space-y-8">
      <Card className="overflow-hidden p-0">
        <Card.Content className="flex flex-col gap-5 p-6 sm:flex-row sm:items-center sm:p-8">
          <Avatar className="size-20 text-2xl"><Avatar.Fallback>{profile.Username.slice(0, 1).toUpperCase()}</Avatar.Fallback></Avatar>
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-accent">Your account</p>
            <h1 className="mt-1 text-3xl font-semibold">{profile.Username}</h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-muted">{profile.Bio || 'Add a short introduction about yourself.'}</p>
          </div>
          <Button onPress={() => { setEditing(true); }} variant="secondary"><Pencil className="size-4" />Edit profile</Button>
        </Card.Content>
      </Card>

      <section className="space-y-5" aria-labelledby="statistics-heading">
        <div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end">
          <div><h2 className="text-2xl font-semibold" id="statistics-heading">Viewing statistics</h2><p className="mt-1 text-sm text-muted">Your activity across the selected period.</p></div>
          <div aria-label="Statistics period" className="flex flex-wrap gap-2" role="group">
            {ranges.map((item) => <Button key={item.key} onPress={() => { setRange(item.key); }} size="sm" variant={range === item.key ? 'primary' : 'secondary'}>{item.label}</Button>)}
          </div>
        </div>
        <div aria-label="Viewing KPIs" className="grid gap-3 lg:grid-cols-2" role="group">
          <KPIGroup aria-label="Viewing totals">
            <InsightKpi icon={Clock3} label="Watch time" value={formatTicks(insights?.WatchedTicks)} />
            <KPIGroup.Separator />
            <InsightKpi icon={Play} label="Playback starts" value={String(insights?.PlayCount ?? 0)} />
          </KPIGroup>
          <KPIGroup aria-label="Viewing variety">
            <InsightKpi icon={Film} label="Unique titles" value={String(insights?.UniqueTitles ?? 0)} />
            <KPIGroup.Separator />
            <InsightKpi icon={Tags} label="Top genre" value={insights?.Genres[0]?.Name ?? 'No activity'} />
          </KPIGroup>
        </div>
        <div className="grid gap-4 lg:grid-cols-2">
          <Card><Card.Header><Card.Title>Daily watch time</Card.Title><Card.Description>Minutes watched by day.</Card.Description></Card.Header><Card.Content><DailyWatchChart insights={insights} /></Card.Content></Card>
          <Card><Card.Header><Card.Title>Genre mix</Card.Title><Card.Description>Genres receiving the most watch time.</Card.Description></Card.Header><Card.Content><GenreRadar insights={insights} /></Card.Content></Card>
          <Card>
            <Card.Header><Card.Title>Movies and series</Card.Title><Card.Description>Playback starts grouped by media type.</Card.Description></Card.Header>
            <Card.Content className="grid grid-cols-2 gap-3">
              <MediaCount icon={Film} label="movies" value={insights?.Media.Movies ?? 0} />
              <MediaCount icon={Tv} label="series" value={insights?.Media.Series ?? 0} />
            </Card.Content>
          </Card>
        </div>
        <ViewingTimeline events={insights?.Timeline ?? []} />
      </section>
      {editing ? <ProfileDialog profile={profile} onClose={() => { setEditing(false); }} onSaved={setProfile} onSessionInvalidated={async () => { await signOut(); void navigate('/app/login?redirect=%2Fapp%2Fprofile', { replace: true }); }} /> : null}
    </div>
  );
}

function InsightKpi({ icon: Icon, label, value }: { icon: typeof Clock3; label: string; value: string }) {
  return <KPI><KPI.Header><KPI.Icon className="bg-accent/10 text-accent"><Icon aria-hidden="true" className="size-4" /></KPI.Icon><KPI.Title>{label}</KPI.Title></KPI.Header><KPI.Content><p className="truncate text-2xl font-semibold tabular-nums text-foreground">{value}</p></KPI.Content></KPI>;
}

function MediaCount({ icon: Icon, label, value }: { icon: typeof Film; label: string; value: number }) {
  return <div aria-label={`${String(value)} ${label}`} className="rounded-xl bg-default p-4"><Icon aria-hidden="true" className="size-5 text-accent" /><p className="mt-3 text-2xl font-semibold">{value} <span className="text-sm font-normal text-muted">{label}</span></p></div>;
}

function DailyWatchChart({ insights }: { insights?: UserInsights }) {
  const data = insights?.Daily.map((point) => ({ date: point.Date.slice(5), minutes: Math.round(point.WatchedTicks / 600_000_000) })) ?? [];
  if (!data.length) return <p className="py-20 text-center text-sm text-muted">No activity in this period.</p>;
  return (
    <figure aria-label="Daily watch time bar chart" role="img">
      <BarChart data={data} height={220}>
        <BarChart.Grid vertical={false} />
        <BarChart.XAxis dataKey="date" tickMargin={8} />
        <BarChart.YAxis tickMargin={4} width={36} />
        <BarChart.Bar barSize={18} dataKey="minutes" fill="var(--color-accent)" name="Minutes watched" radius={[4, 4, 0, 0]} />
        <BarChart.Tooltip content={<BarChart.TooltipContent valueFormatter={(value) => `${String(value)} min`} />} />
      </BarChart>
      <figcaption className="sr-only">{data.map((point) => `${point.date}: ${String(point.minutes)} minutes`).join('; ')}</figcaption>
    </figure>
  );
}

function ViewingTimeline({ events }: { events: InsightTimelineEvent[] }) {
  return (
    <Card>
      <Card.Header><Card.Title>Viewing timeline</Card.Title><Card.Description>Milestones from your playback history.</Card.Description></Card.Header>
      <Card.Content>
        {events.length ? (
          <Timeline density="compact" size="sm">
            {events.map((event) => {
              const presentation = timelinePresentation(event);
              const Icon = presentation.icon;
              return (
                <Timeline.Item key={`${event.Kind}-${event.ItemId}-${event.At}`} status={presentation.status}>
                  <Timeline.Marker aria-hidden="true"><Icon /></Timeline.Marker>
                  <Timeline.Content>
                    <div className="flex min-w-0 flex-col gap-1 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
                      <p className="min-w-0 text-sm"><span className="text-muted">{presentation.prefix} </span><Link className="font-medium text-foreground hover:text-accent" to={`/app/items/${event.ItemId}`}>{event.Name}</Link></p>
                      <time className="shrink-0 text-xs text-muted" dateTime={event.At}>{formatTimelineDate(event.At)}</time>
                    </div>
                  </Timeline.Content>
                </Timeline.Item>
              );
            })}
          </Timeline>
        ) : <p className="py-8 text-center text-sm text-muted">No viewing milestones in this period.</p>}
      </Card.Content>
    </Card>
  );
}

function timelinePresentation(event: InsightTimelineEvent) {
  if (event.Kind === 'SeriesCompleted') return { icon: CheckCircle2, prefix: 'Finished', status: 'success' as const };
  if (event.Kind === 'SeriesStarted') return { icon: Tv, prefix: 'Started', status: 'current' as const };
  return { icon: Film, prefix: 'Watched', status: 'default' as const };
}

function formatTimelineDate(value: string): string {
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value));
}

function GenreRadar({ insights }: { insights?: UserInsights }) {
  const genres = insights?.Genres.slice(0, 6) ?? [];
  const maximum = Math.max(0, ...genres.map((genre) => genre.WatchedTicks));
  if (!genres.length) return <p className="py-16 text-center text-sm text-muted">No genre activity in this period.</p>;
  const scale = Math.max(1, maximum);
  const data = genres.map((genre) => ({
    genre: genre.Name,
    score: Math.round((genre.WatchedTicks / scale) * 100),
    watched: formatTicks(genre.WatchedTicks),
  }));
  return (
    <figure aria-label="Genre watch time radar chart" className="min-w-0" role="img">
      <div aria-hidden="true" className="h-64 min-w-0">
        <ResponsiveContainer height="100%" minHeight={256} minWidth={240} width="100%">
          <RadarChart data={data} margin={{ bottom: 12, left: 24, right: 24, top: 12 }}>
            <PolarGrid stroke="var(--color-border)" />
            <PolarAngleAxis dataKey="genre" tick={{ fill: 'var(--color-muted)', fontSize: 12 }} />
            <PolarRadiusAxis angle={90} domain={[0, 100]} tick={false} />
            <Radar dataKey="score" dot={{ fill: 'var(--color-accent)', r: 3 }} fill="var(--color-accent)" fillOpacity={0.22} name="Relative watch time" stroke="var(--color-accent)" strokeWidth={2} />
            <ChartTooltip formatter={(value) => [`${String(value)}%`, 'Relative watch time']} />
          </RadarChart>
        </ResponsiveContainer>
      </div>
      <figcaption className="sr-only">{data.map((genre) => `${genre.genre}: ${genre.watched}`).join('; ')}</figcaption>
    </figure>
  );
}

function ProfileDialog({ profile, onClose, onSaved, onSessionInvalidated }: { profile: UserProfile; onClose: () => void; onSaved: (profile: UserProfile) => void; onSessionInvalidated: () => Promise<void> }) {
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
      setError(reason instanceof Error ? reason.message : 'Unable to update this account.');
    } finally { setPending(false); }
  };
  return <Modal isOpen onOpenChange={(open) => { if (!open) onClose(); }}><Modal.Backdrop isDismissable={!pending}><Modal.Container placement="center" size="md"><Modal.Dialog><Modal.CloseTrigger aria-label="Close" isDisabled={pending} /><Modal.Header><Modal.Heading>Edit account</Modal.Heading></Modal.Header><Modal.Body><form className="space-y-5" id="profile-form" onSubmit={(event) => { void submit(event); }}><TextField fullWidth isRequired><Label>Username</Label><Input autoComplete="username" value={username} onChange={(event) => { setUsername(event.currentTarget.value); }} /></TextField><TextField fullWidth><Label>Biography</Label><TextArea maxLength={500} value={bio} onChange={(event) => { setBio(event.currentTarget.value); }} /></TextField><div className="border-t border-border pt-5"><p className="mb-4 text-sm font-medium">Security confirmation</p><div className="space-y-4"><TextField fullWidth isRequired><Label>Current password</Label><Input autoComplete="current-password" type="password" value={currentPassword} onChange={(event) => { setCurrentPassword(event.currentTarget.value); }} /></TextField><TextField fullWidth><Label>New password</Label><Input autoComplete="new-password" type="password" value={newPassword} onChange={(event) => { setNewPassword(event.currentTarget.value); }} /></TextField><TextField fullWidth isInvalid={Boolean(confirmPassword && confirmPassword !== newPassword)}><Label>Confirm new password</Label><Input autoComplete="new-password" type="password" value={confirmPassword} onChange={(event) => { setConfirmPassword(event.currentTarget.value); }} /></TextField></div></div>{error ? <p className="text-sm text-danger" role="alert">{error}</p> : null}</form></Modal.Body><Modal.Footer><Button onPress={onClose} variant="tertiary">Cancel</Button><Button form="profile-form" isPending={pending} type="submit">Save changes</Button></Modal.Footer></Modal.Dialog></Modal.Container></Modal.Backdrop></Modal>;
}

function formatTicks(value = 0): string {
  const minutes = Math.round(value / 600_000_000);
  return minutes >= 60 ? `${String(Math.floor(minutes / 60))}h ${String(minutes % 60)}m` : `${String(minutes)}m`;
}
