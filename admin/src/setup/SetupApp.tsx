/* eslint-disable react-hooks/set-state-in-effect */
import {
  Alert,
  Button,
  Description,
  Input,
  Label,
  ListBox,
  ProgressBar,
  Radio,
  RadioGroup,
  Select,
  Spinner,
  TextField,
} from '@heroui/react';
import { CheckCircle2, CircleAlert, Database, MonitorCog, RotateCcw, ShieldCheck, Upload } from 'lucide-react';
import { useEffect, useReducer, useRef, useState } from 'react';
import { FileTrigger } from 'react-aria-components';

import { useClientTheme } from '../client/layout/clientTheme';
import { completeSetup, getSetupStatus, recoverSetup, subscribeSetupProgress, testSetupDatabase, uploadSetupBranding, validateSetupNetwork } from './setupApi';
import { SetupCinematicIntro } from './SetupCinematicIntro';
import type { CinematicSceneFactory } from './cinematicScene';
import { initialSetupState, setupReducer } from './setupMachine';
import type { CompleteSetupDraft, DatabaseDraft, DatabaseTestResult, NetworkValidationResult, SetupProgressStage } from './setupTypes';
import { SetupLayout } from './SetupLayout';

export function SetupApp({ createCinematicScene }: { createCinematicScene?: CinematicSceneFactory } = {}) {
  const [state, dispatch] = useReducer(setupReducer, undefined, initialSetupState);
  const [locale, setLocale] = useState<'zh-CN' | 'en-US'>(() => window.localStorage.getItem('tjxy-system-locale') === 'en-US' ? 'en-US' : 'zh-CN');
  const [csrf, setCsrf] = useState('');
  const [installationId, setInstallationId] = useState('');
  const [status, setStatus] = useState<'loading' | 'ready' | 'failed'>('loading');
  const [environmentBlockers, setEnvironmentBlockers] = useState<string[]>([]);
  const [setupVersion, setSetupVersion] = useState('');
  const [siteTitle, setSiteTitle] = useState('TJXY');
  const [siteSubtitle, setSiteSubtitle] = useState('Your media library');
  const [logoUrl, setLogoUrl] = useState('/brand/tjxy-mark.webp');
  const [iconUrl, setIconUrl] = useState('/brand/favicon.svg');
  const [logoPreview, setLogoPreview] = useState('/brand/tjxy-mark.webp');
  const [iconPreview, setIconPreview] = useState('/brand/favicon.svg');
  const [brandingPending, setBrandingPending] = useState<'logo' | 'icon' | null>(null);
  const [brandingError, setBrandingError] = useState(false);
  const [listenHost, setListenHost] = useState('127.0.0.1');
  const [port, setPort] = useState(8096);
  const [publicUrl, setPublicUrl] = useState('');
  const [username, setUsername] = useState('admin');
  const [password, setPassword] = useState('');
  const [passwordConfirmation, setPasswordConfirmation] = useState('');
  const [databasePending, setDatabasePending] = useState(false);
  const [databaseResult, setDatabaseResult] = useState<DatabaseTestResult | null>(null);
  const [networkPending, setNetworkPending] = useState(false);
  const [networkResult, setNetworkResult] = useState<NetworkValidationResult | null>(null);
  const [networkTestFingerprint, setNetworkTestFingerprint] = useState('');
  const [networkError, setNetworkError] = useState(false);
  const [operationError, setOperationError] = useState(false);
  const [submissionAttempt, setSubmissionAttempt] = useState(0);
  const [recoveryPending, setRecoveryPending] = useState(false);
  const [progressStages, setProgressStages] = useState<SetupProgressStage[]>([]);
  const [progressUnavailable, setProgressUnavailable] = useState(false);
  const submitted = useRef(false);
  const { theme, toggleTheme } = useClientTheme();
  const tr = (english: string, chinese: string) => locale === 'en-US' ? english : chinese;
  const setupLayoutProps = { locale, onLocaleChange: setLocale };

  useEffect(() => {
    document.documentElement.lang = locale;
    window.localStorage.setItem('tjxy-system-locale', locale);
  }, [locale]);

  useEffect(() => () => {
    if (logoPreview.startsWith('blob:')) URL.revokeObjectURL(logoPreview);
  }, [logoPreview]);
  useEffect(() => () => {
    if (iconPreview.startsWith('blob:')) URL.revokeObjectURL(iconPreview);
  }, [iconPreview]);

  const loadStatus = () => {
    setStatus('loading'); setOperationError(false);
    void getSetupStatus().then((value) => {
      setCsrf(value.csrfToken);
      setInstallationId(value.installationId);
      setSetupVersion(value.version);
      if (value.deploymentMode === 'container') setListenHost('0.0.0.0');
      const blockers = [
        ...(!value.configurationWritable ? ['configuration'] : []),
        ...(!value.sourceEligible ? ['source'] : []),
        ...value.blockingOverrides,
      ];
      setEnvironmentBlockers(blockers);
      if (value.state === 'pending') dispatch({ type: 'recover' });
      setStatus('ready');
    }).catch(() => { setStatus('failed'); });
  };
  useEffect(loadStatus, []);

  useEffect(() => {
    if ((state.screen !== 'progress' && state.screen !== 'recovery') || installationId.length === 0) return;
    setProgressStages([]); setProgressUnavailable(false);
    return subscribeSetupProgress(
      installationId,
      (event) => { setProgressStages((stages) => stages.includes(event.stage) ? stages : [...stages, event.stage]); },
      () => { setProgressUnavailable(true); },
    );
  }, [installationId, state.screen]);

  useEffect(() => {
    if (state.screen !== 'progress' || submitted.current) return;
    submitted.current = true;
    const draft: CompleteSetupDraft = {
      siteTitle, siteSubtitle, locale, logoUrl, iconUrl,
      database: selectedDraft(state),
      network: { listenHost, port, publicUrl: publicUrl.trim() || null },
      administratorUsername: username,
      administratorPassword: password,
    };
    void completeSetup(csrf, draft)
      .then((destinationUrl) => { setPassword(''); setPasswordConfirmation(''); dispatch({ type: 'completed', destinationUrl }); })
      .catch(() => { submitted.current = false; setOperationError(true); });
  }, [csrf, iconUrl, listenHost, locale, logoUrl, password, port, publicUrl, siteSubtitle, siteTitle, state, submissionAttempt, username]);

  const currentDraft = selectedDraft(state);
  const tested = state.databaseTests[state.selectedDatabase] === fingerprint(currentDraft);
  const networkDraft = { listenHost, port, publicUrl: publicUrl.trim() || null };
  const currentNetworkFingerprint = fingerprint(networkDraft);
  const networkTested = networkTestFingerprint === currentNetworkFingerprint;
  const testDatabase = () => {
    if (databasePending || csrf.length === 0) return;
    setDatabasePending(true); setOperationError(false); setDatabaseResult(null);
    void testSetupDatabase(csrf, currentDraft)
      .then((result) => { setDatabaseResult(result); dispatch({ type: 'database-tested', backend: state.selectedDatabase, fingerprint: fingerprint(currentDraft) }); })
      .catch(() => { setOperationError(true); })
      .finally(() => { setDatabasePending(false); });
  };

  const testNetwork = () => {
    if (networkPending || csrf.length === 0) return;
    setNetworkPending(true); setNetworkError(false); setNetworkResult(null);
    void validateSetupNetwork(csrf, networkDraft)
      .then((result) => {
        setNetworkResult(result);
        setNetworkTestFingerprint(currentNetworkFingerprint);
      })
      .catch(() => { setNetworkError(true); setNetworkTestFingerprint(''); })
      .finally(() => { setNetworkPending(false); });
  };

  const uploadBranding = (kind: 'logo' | 'icon', files: FileList | null) => {
    const file = files?.item(0);
    if (!file || brandingPending !== null || csrf.length === 0) return;
    setBrandingPending(kind); setBrandingError(false);
    void uploadSetupBranding(csrf, kind, file)
      .then((assetUrl) => {
        const preview = URL.createObjectURL(file);
        if (kind === 'logo') { setLogoUrl(assetUrl); setLogoPreview(preview); }
        else { setIconUrl(assetUrl); setIconPreview(preview); }
      })
      .catch(() => { setBrandingError(true); })
      .finally(() => { setBrandingPending(null); });
  };

  const recover = () => {
    if (recoveryPending || csrf.length === 0 || username.trim().length === 0 || password.length === 0) return;
    setRecoveryPending(true); setOperationError(false);
    void recoverSetup(csrf, username.trim(), password)
      .then((destinationUrl) => { setPassword(''); dispatch({ type: 'completed', destinationUrl }); })
      .catch(() => { setOperationError(true); })
      .finally(() => { setRecoveryPending(false); });
  };

  if (state.screen === 'intro' && status === 'loading') return <section aria-label={tr('Preparing TJXY setup', '正在准备 TJXY 安装')} className="fixed inset-0 z-[100] flex min-h-dvh items-center justify-center bg-black text-white"><Spinner aria-label={tr('Checking setup state', '正在检查安装状态')} /></section>;

  if (state.screen === 'recovery') return <SetupLayout {...setupLayoutProps} description={tr('Verify the original administrator to continue the interrupted installation.', '验证原管理员身份以继续中断的安装。')} step={0} theme={theme} title={tr('Resume setup', '恢复安装')} toggleTheme={toggleTheme}><div className="space-y-5"><Alert status="warning"><Alert.Content><Alert.Title>{tr('An unfinished installation was found', '发现未完成的安装')}</Alert.Title><Alert.Description>{tr('TJXY will continue the same database operation and will not reset the administrator password.', 'TJXY 将继续同一数据库操作，不会重置管理员密码。')}</Alert.Description></Alert.Content></Alert><TextField fullWidth isRequired><Label>{tr('Administrator username', '管理员用户名')}</Label><Input autoComplete="username" value={username} onChange={(event) => { setUsername(event.currentTarget.value); }} /></TextField><TextField fullWidth isRequired isInvalid={operationError}><Label>{tr('Recovery password', '恢复密码')}</Label><Input autoComplete="current-password" type="password" value={password} onChange={(event) => { setPassword(event.currentTarget.value); }} /></TextField>{operationError && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('Unable to resume setup', '无法恢复安装')}</Alert.Title><Alert.Description>{tr('Verify the administrator credentials and database availability.', '请检查管理员凭据和数据库可用性。')}</Alert.Description></Alert.Content></Alert>}<Button isDisabled={username.trim().length === 0 || password.length === 0} isPending={recoveryPending} onPress={recover}>{tr('Resume installation', '继续安装')}</Button></div></SetupLayout>;

  if (state.screen === 'intro') return <SetupCinematicIntro createScene={createCinematicScene} locale={locale} onComplete={() => { dispatch({ type: 'advance' }); }} />;

  if (state.screen === 'welcome') return <SetupLayout {...setupLayoutProps} description={tr('Check this server before writing configuration.', '写入配置前，先检查当前服务器环境。')} footer={<><span /><Button isDisabled={status !== 'ready' || environmentBlockers.length > 0} onPress={() => { dispatch({ type: 'advance' }); }}>{tr('Continue', '继续')}</Button></>} step={0} theme={theme} title={tr('Before we begin', '开始之前')} toggleTheme={toggleTheme}><div className="space-y-4">{status === 'loading' && <div className="flex min-h-32 items-center justify-center"><Spinner aria-label={tr('Checking environment', '正在检查环境')} /></div>}{status === 'ready' && environmentBlockers.length === 0 && <Alert status="success"><Alert.Indicator><CheckCircle2 className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{tr('Setup is available', '可以开始安装')}</Alert.Title><Alert.Description>TJXY {setupVersion} · {tr('private network', '私有网络')}</Alert.Description></Alert.Content></Alert>}{status === 'ready' && environmentBlockers.length > 0 && <Alert role="alert" status="danger"><Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{tr('Resolve these blockers', '请先处理以下阻塞项')}</Alert.Title><Alert.Description>{environmentBlockers.map((blocker) => blocker === 'configuration' ? tr('Configuration directory is not writable.', '配置目录不可写。') : blocker === 'source' ? tr('This network source is not eligible for setup.', '当前网络来源不能执行安装。') : `${blocker} ${tr('would override setup.', '会覆盖安装结果。')}`).join(' ')}</Alert.Description></Alert.Content></Alert>}{status === 'failed' && <Alert role="alert" status="danger"><Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{tr('Environment check failed', '环境检查失败')}</Alert.Title><Alert.Description>{tr('Check the server and retry.', '请检查服务器后重试。')}</Alert.Description></Alert.Content><Button onPress={loadStatus} size="sm" variant="secondary"><RotateCcw className="size-4" />{tr('Retry', '重试')}</Button></Alert>}</div></SetupLayout>;

  const footer = (continueDisabled = false) => <><Button onPress={() => { dispatch({ type: 'back' }); }} variant="tertiary">{tr('Back', '上一步')}</Button><Button isDisabled={continueDisabled} onPress={() => { dispatch({ type: 'advance' }); }}>{tr('Continue', '继续')}</Button></>;

  if (state.screen === 'branding') return <SetupLayout {...setupLayoutProps} description={tr('Choose the identity shown to users.', '设置用户看到的系统名称与图标。')} footer={footer(siteTitle.trim().length === 0 || brandingPending !== null)} step={1} theme={theme} title={tr('Basic information', '基础信息')} toggleTheme={toggleTheme}><div className="space-y-5"><div className="grid gap-5 sm:grid-cols-2"><TextField className="sm:col-span-2" fullWidth isRequired><Label>{tr('System name', '系统名称')}</Label><Input maxLength={120} value={siteTitle} onChange={(event) => { setSiteTitle(event.currentTarget.value); }} /></TextField><TextField className="sm:col-span-2" fullWidth><Label>{tr('Subtitle', '系统简介')}</Label><Input maxLength={240} value={siteSubtitle} onChange={(event) => { setSiteSubtitle(event.currentTarget.value); }} /></TextField><BrandingPicker acceptedFileTypes={['image/png', 'image/jpeg', 'image/webp']} buttonLabel={tr('Choose logo', '选择 Logo')} imageAlt={tr('Logo preview', 'Logo 预览')} isPending={brandingPending === 'logo'} onSelect={(files) => { uploadBranding('logo', files); }} preview={logoPreview} /><BrandingPicker acceptedFileTypes={['image/png', 'image/jpeg', 'image/webp', 'image/x-icon', 'image/vnd.microsoft.icon']} buttonLabel={tr('Choose icon', '选择图标')} imageAlt={tr('Icon preview', '图标预览')} isPending={brandingPending === 'icon'} onSelect={(files) => { uploadBranding('icon', files); }} preview={iconPreview} /></div>{brandingError && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('Image upload failed', '图片上传失败')}</Alert.Title><Alert.Description>{tr('Choose a supported image smaller than 2 MiB.', '请选择小于 2 MiB 的受支持图片。')}</Alert.Description></Alert.Content></Alert>}<Button onPress={() => { setLogoUrl('/brand/tjxy-mark.webp'); setIconUrl('/brand/favicon.svg'); setLogoPreview('/brand/tjxy-mark.webp'); setIconPreview('/brand/favicon.svg'); setBrandingError(false); }} size="sm" variant="tertiary">{tr('Restore defaults', '恢复默认')}</Button></div></SetupLayout>;

  if (state.screen === 'database') return <SetupLayout {...setupLayoutProps} description={tr('Select a supported database and verify it before continuing.', '选择数据库并通过连接测试后继续。')} footer={footer(!tested)} step={2} theme={theme} title={tr('Connect database', '连接数据库')} toggleTheme={toggleTheme}><div className="space-y-5"><RadioGroup aria-label={tr('Database type', '数据库类型')} name="database-backend" onChange={(value) => { if (isBackend(value)) { dispatch({ type: 'select-database', backend: value }); setDatabaseResult(null); setOperationError(false); } }} orientation="horizontal" value={state.selectedDatabase}>{(['sqlite', 'postgresql', 'mysql'] as const).map((backend) => <Radio aria-label={labelForBackend(backend)} key={backend} value={backend}><Radio.Content><Radio.Control><Radio.Indicator /></Radio.Control><span className="font-medium">{labelForBackend(backend)}</span></Radio.Content></Radio>)}</RadioGroup><DatabaseFields draft={currentDraft} onChange={(draft) => { dispatch({ type: 'update-database-draft', backend: draft.Backend, draft }); setDatabaseResult(null); }} tr={tr} />{operationError && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('Connection failed', '连接失败')}</Alert.Title><Alert.Description>{tr('Check the database settings and retry.', '请检查数据库设置后重试。')}</Alert.Description></Alert.Content></Alert>}{databaseResult && tested && <Alert status="success"><Alert.Indicator><CheckCircle2 className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{labelForBackend(databaseResult.backend)} {databaseResult.version} · {databaseResult.elapsedMilliseconds} ms</Alert.Title></Alert.Content></Alert>}<Button isPending={databasePending} onPress={testDatabase} variant="secondary"><Database className="size-4" />{tr('Test connection', '测试连接')}</Button></div></SetupLayout>;

  if (state.screen === 'network') return <SetupLayout {...setupLayoutProps} description={tr('These values take effect only after installation completes.', '这些设置只会在安装完成后生效。')} footer={footer(!networkTested)} step={3} theme={theme} title={tr('Network', '网络')} toggleTheme={toggleTheme}><div className="space-y-5"><div className="grid gap-5 sm:grid-cols-2"><TextField fullWidth isRequired><Label>{tr('Listen address', '监听地址')}</Label><Input value={listenHost} onChange={(event) => { setListenHost(event.currentTarget.value); setNetworkResult(null); setNetworkError(false); }} /><Description>127.0.0.1 / 0.0.0.0 / ::</Description></TextField><TextField fullWidth isRequired><Label>{tr('Port', '端口')}</Label><Input inputMode="numeric" max={65535} min={1} type="number" value={String(port)} onChange={(event) => { setPort(Number(event.currentTarget.value)); setNetworkResult(null); setNetworkError(false); }} /></TextField><TextField className="sm:col-span-2" fullWidth><Label>{tr('Public URL', '公开地址')}</Label><Input placeholder="https://media.example.com" type="url" value={publicUrl} onChange={(event) => { setPublicUrl(event.currentTarget.value); setNetworkResult(null); setNetworkError(false); }} /></TextField></div>{networkError && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('Network validation failed', '网络校验失败')}</Alert.Title><Alert.Description>{tr('Use a literal listen IP, a valid port, and an HTTP or HTTPS public URL.', '请使用有效的监听 IP、端口以及 HTTP 或 HTTPS 公开地址。')}</Alert.Description></Alert.Content></Alert>}{networkResult && networkTested && <Alert status="success"><Alert.Indicator><CheckCircle2 className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{tr('Network settings are valid', '网络设置有效')}</Alert.Title><Alert.Description>{networkResult.destinationUrl}</Alert.Description></Alert.Content></Alert>}<Button isDisabled={port < 1 || port > 65535 || listenHost.trim().length === 0} isPending={networkPending} onPress={testNetwork} variant="secondary">{tr('Validate network', '校验网络')}</Button></div></SetupLayout>;

  if (state.screen === 'administrator') { const invalid = username.trim().length === 0 || password.length < 8 || password !== passwordConfirmation; return <SetupLayout {...setupLayoutProps} description={tr('Create the first enabled administrator.', '创建第一个启用的管理员账户。')} footer={footer(invalid)} step={4} theme={theme} title={tr('Administrator account', '管理员账户')} toggleTheme={toggleTheme}><div className="space-y-5"><TextField fullWidth isRequired><Label>{tr('Username', '用户名')}</Label><Input autoComplete="username" value={username} onChange={(event) => { setUsername(event.currentTarget.value); }} /></TextField><TextField fullWidth isRequired><Label>{tr('Password', '密码')}</Label><Input autoComplete="new-password" type="password" value={password} onChange={(event) => { setPassword(event.currentTarget.value); }} /><Description>{tr('Use at least 8 characters.', '至少使用 8 个字符。')}</Description></TextField><TextField fullWidth isRequired isInvalid={passwordConfirmation.length > 0 && password !== passwordConfirmation}><Label>{tr('Confirm password', '确认密码')}</Label><Input autoComplete="new-password" type="password" value={passwordConfirmation} onChange={(event) => { setPasswordConfirmation(event.currentTarget.value); }} /></TextField></div></SetupLayout>; }

  if (state.screen === 'review') return <SetupLayout {...setupLayoutProps} description={tr('Review the non-secret configuration before installation.', '安装前确认不含敏感信息的配置摘要。')} footer={<><Button onPress={() => { dispatch({ type: 'back' }); }} variant="tertiary">{tr('Back', '上一步')}</Button><Button onPress={() => { dispatch({ type: 'install' }); }}><ShieldCheck className="size-4" />{tr('Install TJXY', '安装 TJXY')}</Button></>} step={4} theme={theme} title={tr('Review and install', '确认并安装')} toggleTheme={toggleTheme}><dl className="divide-y divide-border border-y border-border text-sm"><Summary label={tr('System', '系统')} value={siteTitle} /><Summary label={tr('Database', '数据库')} value={labelForBackend(state.selectedDatabase)} /><Summary label={tr('Listen address', '监听地址')} value={`${listenHost}:${String(port)}`} /><Summary label={tr('Administrator', '管理员')} value={username} /></dl></SetupLayout>;

  if (state.screen === 'progress') return <SetupLayout {...setupLayoutProps} description={tr('Keep this page open while TJXY prepares the database and account.', 'TJXY 正在准备数据库和管理员账户，请保持页面打开。')} step={0} theme={theme} title={tr('Installing TJXY', '正在安装 TJXY')} toggleTheme={toggleTheme}>{operationError ? <Alert role="alert" status="danger"><Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{tr('Installation stopped', '安装已停止')}</Alert.Title><Alert.Description>{tr('The completed state was not activated. Retry this installation.', '系统未进入已安装状态，请重试。')}</Alert.Description></Alert.Content><Button onPress={() => { setOperationError(false); submitted.current = false; setSubmissionAttempt((attempt) => attempt + 1); }} variant="secondary">{tr('Retry', '重试')}</Button></Alert> : <SetupProgressPanel progressStages={progressStages} progressUnavailable={progressUnavailable} tr={tr} />}</SetupLayout>;

  return <SetupLayout {...setupLayoutProps} description={tr('The server is configured and ready to restart.', '服务器配置已经完成，可以进入系统。')} step={0} theme={theme} title={tr('Setup complete', '安装完成')} toggleTheme={toggleTheme}><div className="flex min-h-64 flex-col items-center justify-center border border-border text-center"><CheckCircle2 className="size-12 text-success" /><p className="mt-4 text-sm text-muted">{state.destinationUrl}</p><Button className="mt-6" onPress={() => { if (state.destinationUrl) window.location.assign(state.destinationUrl); }}><MonitorCog className="size-4" />{tr('Open administrator login', '进入管理员登录')}</Button></div></SetupLayout>;
}

function DatabaseFields({ draft, onChange, tr }: { draft: DatabaseDraft; onChange: (draft: DatabaseDraft) => void; tr: (english: string, chinese: string) => string }) {
  if (draft.Backend === 'sqlite') return <TextField fullWidth isRequired><Label>{tr('Database file', '数据库文件')}</Label><Input value={draft.Path} onChange={(event) => { onChange({ ...draft, Path: event.currentTarget.value }); }} /></TextField>;
  return <div className="grid gap-4 sm:grid-cols-2"><TextField fullWidth isRequired><Label>{tr('Host', '主机')}</Label><Input value={draft.Host} onChange={(event) => { onChange({ ...draft, Host: event.currentTarget.value }); }} /></TextField><TextField fullWidth isRequired><Label>{tr('Port', '端口')}</Label><Input inputMode="numeric" type="number" value={String(draft.Port)} onChange={(event) => { onChange({ ...draft, Port: Number(event.currentTarget.value) }); }} /></TextField><TextField fullWidth isRequired><Label>{tr('Database', '数据库')}</Label><Input value={draft.Database} onChange={(event) => { onChange({ ...draft, Database: event.currentTarget.value }); }} /></TextField><TextField fullWidth isRequired><Label>{tr('Username', '用户名')}</Label><Input autoComplete="username" value={draft.Username} onChange={(event) => { onChange({ ...draft, Username: event.currentTarget.value }); }} /></TextField><TextField className="sm:col-span-2" fullWidth isRequired><Label>{tr('Password', '密码')}</Label><Input autoComplete="new-password" type="password" value={draft.Password} onChange={(event) => { onChange({ ...draft, Password: event.currentTarget.value }); }} /></TextField><Select className="sm:col-span-2" value={draft.Tls} onChange={(value) => { if (value === 'disable' || value === 'prefer' || value === 'require') onChange({ ...draft, Tls: value }); }}><Label>TLS</Label><Select.Trigger><Select.Value /><Select.Indicator /></Select.Trigger><Select.Popover><ListBox><ListBox.Item id="disable">{tr('Disabled', '关闭')}<ListBox.ItemIndicator /></ListBox.Item><ListBox.Item id="prefer">{tr('Preferred', '优先加密')}<ListBox.ItemIndicator /></ListBox.Item><ListBox.Item id="require">{tr('Required', '必须加密')}<ListBox.ItemIndicator /></ListBox.Item></ListBox></Select.Popover></Select></div>;
}

function BrandingPicker({ acceptedFileTypes, buttonLabel, imageAlt, isPending, onSelect, preview }: {
  acceptedFileTypes: string[];
  buttonLabel: string;
  imageAlt: string;
  isPending: boolean;
  onSelect: (files: FileList | null) => void;
  preview: string;
}) {
  return <div className="flex min-h-32 items-center gap-4 border border-border p-4"><img alt={imageAlt} className="size-16 shrink-0 object-contain" src={preview} /><div className="min-w-0"><FileTrigger acceptedFileTypes={acceptedFileTypes} onSelect={onSelect}><Button isPending={isPending} size="sm" variant="secondary"><Upload aria-hidden="true" className="size-4" />{buttonLabel}</Button></FileTrigger><p className="mt-2 text-xs text-muted">PNG, JPEG, WebP · 2 MiB</p></div></div>;
}

function selectedDraft(state: ReturnType<typeof initialSetupState>): DatabaseDraft { return state.databaseDrafts[state.selectedDatabase]; }
function fingerprint(draft: unknown): string { return JSON.stringify(draft); }
function isBackend(value: string): value is 'sqlite' | 'postgresql' | 'mysql' { return value === 'sqlite' || value === 'postgresql' || value === 'mysql'; }
function labelForBackend(backend: 'sqlite' | 'postgresql' | 'mysql'): string { return backend === 'sqlite' ? 'SQLite' : backend === 'postgresql' ? 'PostgreSQL' : 'MySQL'; }
function Summary({ label, value }: { label: string; value: string }) { return <div className="grid grid-cols-[140px_1fr] gap-4 py-4"><dt className="text-muted">{label}</dt><dd className="min-w-0 break-words font-medium">{value}</dd></div>; }

const orderedProgressStages: SetupProgressStage[] = ['connecting_database', 'migrating_database', 'creating_administrator', 'saving_settings', 'completing_installation', 'complete'];

function SetupProgressPanel({ progressStages, progressUnavailable, tr }: { progressStages: SetupProgressStage[]; progressUnavailable: boolean; tr: (english: string, chinese: string) => string }) {
  const completed = orderedProgressStages.filter((stage) => progressStages.includes(stage));
  const progress = Math.min(100, Math.round((completed.length / orderedProgressStages.length) * 100));
  const labels: Record<SetupProgressStage, string> = {
    connecting_database: tr('Connecting to database', '正在连接数据库'),
    migrating_database: tr('Applying database migrations', '正在执行数据库迁移'),
    creating_administrator: tr('Creating administrator', '正在创建管理员'),
    saving_settings: tr('Saving system settings', '正在保存系统设置'),
    completing_installation: tr('Finalizing installation', '正在完成安装'),
    complete: tr('Installation complete', '安装完成'),
    failed: tr('Installation stopped', '安装已停止'),
  };
  return <div className="space-y-5"><ProgressBar aria-label={tr('Installation progress', '安装进度')} value={progress}><ProgressBar.Track><ProgressBar.Fill /></ProgressBar.Track></ProgressBar><div aria-live="polite" className="space-y-2">{orderedProgressStages.slice(0, -1).map((stage) => <div className="flex items-center gap-3 text-sm" key={stage}><CheckCircle2 aria-hidden="true" className={progressStages.includes(stage) ? 'size-4 text-success' : 'size-4 text-muted/40'} /><span className={progressStages.includes(stage) ? 'text-foreground' : 'text-muted'}>{labels[stage]}</span></div>)}</div>{progressUnavailable && <Alert status="warning"><Alert.Content><Alert.Title>{tr('Progress connection interrupted', '进度连接已中断')}</Alert.Title><Alert.Description>{tr('The installation request is still authoritative. Keep this page open while it finishes.', '安装请求仍在继续，请保持页面打开等待完成。')}</Alert.Description></Alert.Content></Alert>}</div>;
}
