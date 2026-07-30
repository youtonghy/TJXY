/* eslint-disable @typescript-eslint/no-floating-promises */
import { Alert, Button, Input, Label, TextField, Tooltip } from '@heroui/react';
import { Eye, EyeOff, CircleAlert, Film } from 'lucide-react';
import { useState, type SyntheticEvent } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { useClientAuth } from './ClientAuthContext';
import { safeClientDestination } from './clientDestination';

export function ClientLoginPage() {
  const { signIn } = useClientAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [visible, setVisible] = useState(false);
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState(false);
  const destination = safeClientDestination(new URLSearchParams(location.search).get('redirect'));
  async function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault(); if (pending) return; setPending(true); setFailed(false);
    try { await signIn(username, password); navigate(destination, { replace: true }); } catch { setFailed(true); } finally { setPending(false); }
  }
  return <div className="min-h-screen bg-background px-4 py-10"><main className="mx-auto flex min-h-[calc(100vh-5rem)] max-w-md items-center justify-center"><section className="w-full rounded-2xl border border-border bg-surface p-7 shadow-sm sm:p-9"><div className="mb-8 flex items-center gap-3"><span className="inline-flex size-11 items-center justify-center rounded-xl bg-accent text-accent-foreground"><Film aria-hidden="true" className="size-5" /></span><div><p className="text-base font-semibold text-foreground">TJXY</p><p className="text-sm text-muted">Your media library</p></div></div><h1 className="text-2xl font-semibold text-foreground">Welcome back</h1><p className="mt-1 text-sm text-muted">Sign in to continue watching.</p>{failed && <Alert className="mt-5" role="alert" status="danger"><Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>Sign in failed</Alert.Title><Alert.Description>Check your username and password.</Alert.Description></Alert.Content></Alert>}<form className="mt-6 space-y-4" onSubmit={(event) => { void submit(event); }}><TextField fullWidth isRequired name="username"><Label>Username</Label><Input autoComplete="username" fullWidth value={username} onChange={(event) => { setUsername(event.currentTarget.value); }} /></TextField><TextField fullWidth isRequired name="password"><Label>Password</Label><div className="relative"><Input autoComplete="current-password" className="pr-11" fullWidth type={visible ? 'text' : 'password'} value={password} onChange={(event) => { setPassword(event.currentTarget.value); }} /><Tooltip><Button aria-label={visible ? 'Hide password' : 'Show password'} className="absolute right-1 top-1/2 -translate-y-1/2" isIconOnly onPress={() => { setVisible((value) => !value); }} size="sm" type="button" variant="ghost">{visible ? <EyeOff className="size-4" /> : <Eye className="size-4" />}</Button><Tooltip.Content>{visible ? 'Hide password' : 'Show password'}</Tooltip.Content></Tooltip></div></TextField><Button fullWidth isDisabled={pending} type="submit">{pending ? 'Signing in…' : 'Sign in'}</Button></form></section></main></div>;
}
