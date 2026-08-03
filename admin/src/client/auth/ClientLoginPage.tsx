/* eslint-disable @typescript-eslint/no-floating-promises */
import { Alert, Button, Input, Label, ListBox, TextField, Tooltip } from '@heroui/react';
import { InlineSelect } from '@heroui-pro/react/inline-select';
import { CircleAlert, Eye, EyeOff } from 'lucide-react';
import { useState, type SyntheticEvent } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { BrandMark } from '../../ui/BrandMark';
import { useSystemLocale } from '../../settings/SystemLocaleProvider';
import { useTranslate } from '../../settings/i18n';
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
  const { locale, setLocale } = useSystemLocale();
  const tr = useTranslate();
  const destination = safeClientDestination(new URLSearchParams(location.search).get('redirect'));
  async function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault(); if (pending) return; setPending(true); setFailed(false);
    try { await signIn(username, password); navigate(destination, { replace: true }); } catch { setFailed(true); } finally { setPending(false); }
  }
  return <div className="min-h-screen bg-background px-4 py-10"><main className="mx-auto flex min-h-[calc(100vh-5rem)] max-w-md items-center justify-center"><section className="relative w-full rounded-2xl border border-border bg-surface p-7 shadow-sm sm:p-9"><InlineSelect aria-label={tr('Interface language', '界面语言')} className="absolute right-5 top-5" value={locale} onChange={(value) => { if (value === 'zh-CN' || value === 'en-US') setLocale(value); }}><InlineSelect.Trigger><InlineSelect.Value /><InlineSelect.Indicator /></InlineSelect.Trigger><InlineSelect.Popover><ListBox><ListBox.Item id="zh-CN" textValue="中文">中文<ListBox.ItemIndicator /></ListBox.Item><ListBox.Item id="en-US" textValue="English">English<ListBox.ItemIndicator /></ListBox.Item></ListBox></InlineSelect.Popover></InlineSelect><div className="mb-8 flex items-center gap-3"><BrandMark className="size-11" priority /><div><p className="text-base font-semibold text-foreground">TJXY</p><p className="text-sm text-muted">{tr('Your media library', '你的媒体库')}</p></div></div><h1 className="text-2xl font-semibold text-foreground">{tr('Welcome back', '欢迎回来')}</h1><p className="mt-1 text-sm text-muted">{tr('Sign in to continue watching.', '登录后继续观看。')}</p>{failed && <Alert className="mt-5" role="alert" status="danger"><Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{tr('Sign in failed', '登录失败')}</Alert.Title><Alert.Description>{tr('Check your username and password.', '请检查用户名和密码。')}</Alert.Description></Alert.Content></Alert>}<form className="mt-6 space-y-4" onSubmit={(event) => { void submit(event); }}><TextField fullWidth isRequired name="username"><Label>{tr('Username', '用户名')}</Label><Input autoComplete="username" fullWidth value={username} onChange={(event) => { setUsername(event.currentTarget.value); }} /></TextField><TextField fullWidth isRequired name="password"><Label>{tr('Password', '密码')}</Label><div className="relative"><Input autoComplete="current-password" className="pr-11" fullWidth type={visible ? 'text' : 'password'} value={password} onChange={(event) => { setPassword(event.currentTarget.value); }} /><Tooltip><Button aria-label={visible ? tr('Hide password', '隐藏密码') : tr('Show password', '显示密码')} className="absolute right-1 top-1/2 -translate-y-1/2" isIconOnly onPress={() => { setVisible((value) => !value); }} size="sm" type="button" variant="ghost">{visible ? <EyeOff className="size-4" /> : <Eye className="size-4" />}</Button><Tooltip.Content>{visible ? tr('Hide password', '隐藏密码') : tr('Show password', '显示密码')}</Tooltip.Content></Tooltip></div></TextField><Button fullWidth isDisabled={pending} type="submit">{pending ? tr('Signing in…', '登录中…') : tr('Sign in', '登录')}</Button></form></section></main></div>;
}
