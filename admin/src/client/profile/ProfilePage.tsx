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
import { Clock3, Film, Pencil, Play, Tags, Tv } from 'lucide-react';
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
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
          <MetricCard icon={Clock3} label="Watch time" value={formatTicks(insights?.WatchedTicks)} />
          <MetricCard icon={Play} label="Playback starts" value={String(insights?.PlayCount ?? 0)} />
          <MetricCard icon={Film} label="Unique titles" value={String(insights?.UniqueTitles ?? 0)} />
          <MetricCard icon={Tags} label="Genre mix" value={insights?.Genres[0]?.Name ?? 'No activity'} />
        </div>
        <div className="grid gap-4 lg:grid-cols-2">
          <Card><Card.Header><Card.Title>Daily watch time</Card.Title><Card.Description>Minutes watched by day.</Card.Description></Card.Header><Card.Content><DailyBars insights={insights} /></Card.Content></Card>
          <Card><Card.Header><Card.Title>Genre mix</Card.Title><Card.Description>Genres receiving the most watch time.</Card.Description></Card.Header><Card.Content><GenreRadar insights={insights} /></Card.Content></Card>
          <Card>
            <Card.Header><Card.Title>Movies and series</Card.Title><Card.Description>Playback starts grouped by media type.</Card.Description></Card.Header>
            <Card.Content className="grid grid-cols-2 gap-3">
              <MediaCount icon={Film} label="movies" value={insights?.Media.Movies ?? 0} />
              <MediaCount icon={Tv} label="series" value={insights?.Media.Series ?? 0} />
            </Card.Content>
          </Card>
          <Card>
            <Card.Header><Card.Title>Recent activity</Card.Title><Card.Description>Your latest titles in this period.</Card.Description></Card.Header>
            <Card.Content className="space-y-2">
              {insights?.Recent.length
                ? insights.Recent.slice(0, 6).map((item) => (
                    <Link className="flex items-center justify-between gap-4 rounded-lg px-2 py-2 hover:bg-default" key={item.Id} to={`/app/items/${item.Id}`}>
                      <span className="min-w-0"><span className="block truncate font-medium">{item.Name}</span><span className="text-xs text-muted">{[item.Type, item.ProductionYear].filter(Boolean).join(' · ')}</span></span>
                      <Play aria-hidden="true" className="size-4 shrink-0 text-accent" />
                    </Link>
                  ))
                : <p className="text-sm text-muted">No viewing history in this period.</p>}
            </Card.Content>
          </Card>
        </div>
      </section>
      {editing ? <ProfileDialog profile={profile} onClose={() => { setEditing(false); }} onSaved={setProfile} onSessionInvalidated={async () => { await signOut(); void navigate('/app/login?redirect=%2Fapp%2Fprofile', { replace: true }); }} /> : null}
    </div>
  );
}

function MetricCard({ icon: Icon, label, value }: { icon: typeof Clock3; label: string; value: string }) {
  return <Card><Card.Content className="flex items-start justify-between gap-4"><div><p className="text-sm text-muted">{label}</p><p className="mt-2 text-2xl font-semibold">{value}</p></div><span className="grid size-10 place-items-center rounded-lg bg-accent/12 text-accent"><Icon className="size-5" /></span></Card.Content></Card>;
}

function MediaCount({ icon: Icon, label, value }: { icon: typeof Film; label: string; value: number }) {
  return <div aria-label={`${String(value)} ${label}`} className="rounded-xl bg-default p-4"><Icon aria-hidden="true" className="size-5 text-accent" /><p className="mt-3 text-2xl font-semibold">{value} <span className="text-sm font-normal text-muted">{label}</span></p></div>;
}

function DailyBars({ insights }: { insights?: UserInsights }) {
  const maximum = Math.max(1, ...(insights?.Daily.map((point) => point.WatchedTicks) ?? [1]));
  return <div className="flex h-36 items-end gap-2">{insights?.Daily.length ? insights.Daily.map((point) => <div className="flex min-w-0 flex-1 flex-col items-center gap-2" key={point.Date}><div aria-label={`${point.Date}: ${formatTicks(point.WatchedTicks)}`} className="w-full rounded-t bg-accent" style={{ height: `${String(Math.max(8, (point.WatchedTicks / maximum) * 112))}px` }} /><span className="truncate text-[10px] text-muted">{point.Date.slice(5)}</span></div>) : <p className="self-center text-sm text-muted">No activity in this period.</p>}</div>;
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
