/* eslint-disable @typescript-eslint/no-floating-promises */
import { Alert, Button, Checkbox, Input, Label, ListBox, Spinner, Tabs, TextField, Tooltip } from '@heroui/react';
import { InlineSelect } from '@heroui-pro/react/inline-select';
import { CircleAlert, Eye, EyeOff } from 'lucide-react';
import { useState, type SyntheticEvent } from 'react';
import { Navigate, useLocation, useNavigate } from 'react-router-dom';
import { useSystemLocale } from '../../settings/SystemLocaleProvider';
import { useTranslate } from '../../settings/i18n';
import { useActiveClientTheme } from '../themes/ThemeRuntime';
import { useClientAuth } from './ClientAuthContext';
import { safeClientDestination } from './clientDestination';
import { getStoredApiBaseUrl, isDesktopShell, probeServer, setApiBaseUrl } from '../api/apiBase';
import { loadSavedCredentials, persistCredentialsPreference } from './savedCredentials';
import { ServerAddressField } from '../ui/ServerAddressField';
import { QrLoginPanel } from './QrLoginPanel';

export function ClientLoginPage() {
  const { adoptAuthentication, isLoading, signIn, signInWithPasskey, user } = useClientAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const saved = loadSavedCredentials();
  const [username, setUsername] = useState(saved.username);
  const [password, setPassword] = useState('');
  const [remember, setRemember] = useState(saved.remember);
  const [visible, setVisible] = useState(false);
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState(false);
  const [server, setServer] = useState(getStoredApiBaseUrl() ?? 'http://127.0.0.1:8096');
  const [serverPending, setServerPending] = useState(false);
  const [serverError, setServerError] = useState<string>();
  const [serverOk, setServerOk] = useState(false);
  const [mode, setMode] = useState<'password' | 'qr'>('password');
  const [passkeyPending, setPasskeyPending] = useState(false);
  const { locale, setLocale, siteTitle, siteSubtitle, logoUrl, passkeyEnabled } = useSystemLocale();
  const { definition, options } = useActiveClientTheme();
  const tr = useTranslate();
  const destination = safeClientDestination(new URLSearchParams(location.search).get('redirect'));

  async function connectServer(): Promise<string> {
    setServerPending(true);
    setServerError(undefined);
    try {
      const origin = await probeServer(server);
      setApiBaseUrl(origin);
      setServer(origin);
      setServerOk(true);
      return origin;
    } catch {
      setServerOk(false);
      setServerError(tr('Could not reach that server.', '无法连接到该服务器。'));
      throw new Error('unreachable');
    } finally {
      setServerPending(false);
    }
  }

  async function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    setPending(true);
    setFailed(false);
    try {
      if (isDesktopShell()) await connectServer();
      await signIn(username, password);
      persistCredentialsPreference(remember, username);
      navigate(destination, { replace: true });
    } catch {
      setFailed(true);
    } finally {
      setPending(false);
    }
  }

  async function adoptQrAuthentication(authentication: Parameters<typeof adoptAuthentication>[0]) {
    await adoptAuthentication(authentication);
    navigate(destination, { replace: true });
  }

  if (isLoading) return <div className="flex min-h-screen items-center justify-center bg-background"><Spinner aria-label={tr('Loading account', '正在加载账户')} /></div>;
  if (user !== null) return <Navigate replace to={destination} />;
  const LoginFrame = definition.LoginFrame;
  return (
    <LoginFrame
      actions={(
        <InlineSelect aria-label={tr('Interface language', '界面语言')} value={locale} onChange={(value) => { if (value === 'zh-CN' || value === 'en-US') setLocale(value); }}>
          <InlineSelect.Trigger><InlineSelect.Value /><InlineSelect.Indicator /></InlineSelect.Trigger>
          <InlineSelect.Popover><ListBox><ListBox.Item id="zh-CN" textValue="中文">中文<ListBox.ItemIndicator /></ListBox.Item><ListBox.Item id="en-US" textValue="English">English<ListBox.ItemIndicator /></ListBox.Item></ListBox></InlineSelect.Popover>
        </InlineSelect>
      )}
      logoUrl={logoUrl}
      options={options}
      siteSubtitle={siteSubtitle}
      siteTitle={siteTitle}
    >
      <h1 className="text-2xl font-semibold text-foreground">{tr('Welcome back', '欢迎回来')}</h1>
      <p className="mt-1 text-sm text-muted">{tr('Sign in to continue watching.', '登录后继续观看。')}</p>
      {isDesktopShell() && (
        <div className="mt-5">
          <ServerAddressField
            error={serverError}
            ok={serverOk}
            pending={serverPending}
            required
            value={server}
            onChange={(next) => { setServer(next); setServerOk(false); setServerError(undefined); }}
            onSave={() => { void connectServer(); }}
          />
        </div>
      )}
      {failed && !serverError && (
        <Alert className="mt-5" role="alert" status="danger">
          <Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator>
          <Alert.Content>
            <Alert.Title>{tr('Sign in failed', '登录失败')}</Alert.Title>
            <Alert.Description>{tr('Check your username and password.', '请检查用户名和密码。')}</Alert.Description>
          </Alert.Content>
        </Alert>
      )}
      <Tabs className="mt-6 w-full" selectedKey={mode} onSelectionChange={(key) => {
        const next = key === 'qr' ? 'qr' : 'password';
        if (next === 'qr' && isDesktopShell() && !serverOk) void connectServer().then(() => { setMode(next); }).catch(() => { /* keep password mode */ });
        else setMode(next);
      }}>
        <Tabs.ListContainer><Tabs.List aria-label={tr('Sign-in method', '登录方式')}><Tabs.Tab id="password">{tr('Password', '密码')}<Tabs.Indicator /></Tabs.Tab><Tabs.Tab id="qr">{tr('QR code', '二维码')}<Tabs.Indicator /></Tabs.Tab></Tabs.List></Tabs.ListContainer>
        <Tabs.Panel className="pt-4" id="password">
        <form className="space-y-4" onSubmit={(event) => { void submit(event); }}>
        <TextField fullWidth isRequired name="username"><Label>{tr('Username', '用户名')}</Label><Input autoComplete="username" fullWidth value={username} onChange={(event) => { setUsername(event.currentTarget.value); }} /></TextField>
        <TextField fullWidth isRequired name="password"><Label>{tr('Password', '密码')}</Label><div className="relative"><Input autoComplete="current-password" className="pr-11" fullWidth type={visible ? 'text' : 'password'} value={password} onChange={(event) => { setPassword(event.currentTarget.value); }} /><Tooltip><Button aria-label={visible ? tr('Hide password', '隐藏密码') : tr('Show password', '显示密码')} className="absolute right-1 top-1/2 -translate-y-1/2" isIconOnly onPress={() => { setVisible((value) => !value); }} size="sm" type="button" variant="ghost">{visible ? <EyeOff className="size-4" /> : <Eye className="size-4" />}</Button><Tooltip.Content>{visible ? tr('Hide password', '隐藏密码') : tr('Show password', '显示密码')}</Tooltip.Content></Tooltip></div></TextField>
        <Checkbox isSelected={remember} onChange={setRemember}>
          <Checkbox.Content>
            <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
            {tr('Remember username', '保存用户名')}
          </Checkbox.Content>
        </Checkbox>
        <Button fullWidth isDisabled={pending || serverPending} type="submit">{pending ? tr('Signing in…', '登录中…') : tr('Sign in', '登录')}</Button>
        {passkeyEnabled ? <Button fullWidth isDisabled={pending || passkeyPending || serverPending} isPending={passkeyPending} onPress={() => { setPasskeyPending(true); void signInWithPasskey().then(() => { navigate(destination, { replace: true }); }).catch(() => { setFailed(true); }).finally(() => { setPasskeyPending(false); }); }} type="button" variant="secondary">{tr('Sign in with Passkey', 'Passkey 登录')}</Button> : null}
        </form>
        </Tabs.Panel>
        <Tabs.Panel className="pt-4" id="qr"><QrLoginPanel onAuthenticated={adoptQrAuthentication} /></Tabs.Panel>
      </Tabs>
    </LoginFrame>
  );
}
