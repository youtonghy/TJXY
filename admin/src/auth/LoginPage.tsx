import {
  Alert,
  Button,
  Input,
  Label,
  ListBox,
  TextField,
  Tooltip,
} from '@heroui/react';
import { CircleAlert, Eye, EyeOff } from 'lucide-react';
import { InlineSelect } from '@heroui-pro/react/inline-select';
import { useLogin } from 'ra-core';
import {
  useEffect,
  useRef,
  useState,
  type SyntheticEvent,
} from 'react';
import { useLocation } from 'react-router-dom';

import { checkServerReadiness } from '../api/readiness';
import { BrandMark } from '../ui/BrandMark';
import { loginDestination } from './loginDestination';
import { saveSystemLanguage } from '../settings/systemLanguageApi';
import { useSystemLocale } from '../settings/SystemLocaleProvider';
import { useTranslate } from '../settings/i18n';

type ReadinessState = 'checking' | 'ready' | 'unavailable';

export function LoginPage() {
  const login = useLogin();
  const location = useLocation();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [isPasswordVisible, setIsPasswordVisible] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submissionError, setSubmissionError] = useState(false);
  const [readiness, setReadiness] = useState<ReadinessState>('checking');
  const { locale, setLocale } = useSystemLocale();
  const tr = useTranslate();
  const mountedRef = useRef(true);

  useEffect(() => {
    document.title = locale === 'zh-CN' ? '登录 | TJXY 管理后台' : 'Sign in | TJXY Admin';
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, [locale]);

  useEffect(() => {
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      void checkServerReadiness(controller.signal).then((isReady) => {
        if (!controller.signal.aborted) setReadiness(isReady ? 'ready' : 'unavailable');
      });
    }, 0);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, []);

  const handleSubmit = async (event: SyntheticEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (isSubmitting) return;
    setIsSubmitting(true);
    setSubmissionError(false);
    try {
      await login(
        { username, password },
        loginDestination(location.state, window.location.origin),
      );
      if (mountedRef.current) setPassword('');
    } catch {
      if (mountedRef.current) setSubmissionError(true);
    } finally {
      if (mountedRef.current) setIsSubmitting(false);
    }
  };

  const readinessLabel = readiness === 'checking'
    ? tr('Checking server', '正在检查服务器')
    : readiness === 'ready'
      ? tr('Server ready', '服务器已就绪')
      : tr('Server unavailable', '服务器不可用');

  return (
    <div className="min-h-screen bg-background p-4 sm:p-6 lg:p-8">
      <header
        className="mb-8 flex items-center gap-3 lg:fixed lg:left-8 lg:top-8 lg:mb-0"
        data-testid="login-brand"
      >
        <BrandMark className="size-10" priority />
        <span>
          <span className="block text-sm font-semibold text-foreground">{tr('TJXY Admin', 'TJXY 管理后台')}</span>
          <span className="block text-xs text-muted">{tr('Administrator workspace', '管理员工作区')}</span>
        </span>
      </header>

      <main className="flex min-h-[calc(100vh-2rem)] items-center justify-center sm:min-h-[calc(100vh-3rem)] lg:min-h-[calc(100vh-4rem)]">
        <section className="relative w-full max-w-[380px] rounded-lg border border-border bg-surface p-6 shadow-sm sm:p-7">
          <InlineSelect aria-label={tr('Interface language', '界面语言')} className="absolute right-5 top-4" value={locale} onChange={(value) => { if (value !== 'zh-CN' && value !== 'en-US') return; setLocale(value); void saveSystemLanguage(value, null, true).catch(() => undefined); }}><InlineSelect.Trigger><InlineSelect.Value /><InlineSelect.Indicator /></InlineSelect.Trigger><InlineSelect.Popover><ListBox><ListBox.Item id="zh-CN" textValue="中文">中文<ListBox.ItemIndicator /></ListBox.Item><ListBox.Item id="en-US" textValue="English">English<ListBox.ItemIndicator /></ListBox.Item></ListBox></InlineSelect.Popover></InlineSelect>
          <div className="mb-6">
            <h1 className="text-2xl font-semibold text-foreground">{tr('Administrator sign in', '管理员登录')}</h1>
            <p className="mt-1 text-sm leading-6 text-muted">
              {tr('Use an enabled administrator account.', '请使用已启用的管理员账户。')}
            </p>
          </div>


          <form
            aria-describedby={submissionError ? 'login-error' : undefined}
            aria-label={tr('Administrator sign in', '管理员登录')}
            className="space-y-4"
            onSubmit={(event) => { void handleSubmit(event); }}
          >
            {submissionError && (
              <Alert id="login-error" role="alert" status="danger">
                <Alert.Indicator>
                  <CircleAlert aria-hidden="true" className="size-4" />
                </Alert.Indicator>
                <Alert.Content>
                  <Alert.Title>{tr('Sign in failed', '登录失败')}</Alert.Title>
                  <Alert.Description>{tr('Check your credentials and try again.', '请检查登录信息后重试。')}</Alert.Description>
                </Alert.Content>
              </Alert>
            )}

            <TextField fullWidth isRequired name="username">
              <Label>{tr('Username', '用户名')}</Label>
              <Input
                autoComplete="username"
                fullWidth
                onChange={(event) => { setUsername(event.currentTarget.value); }}
                value={username}
              />
            </TextField>

            <TextField fullWidth isRequired name="password">
              <Label>{tr('Password', '密码')}</Label>
              <div className="relative">
                <Input
                  autoComplete="current-password"
                  className="pr-11"
                  fullWidth
                  onChange={(event) => { setPassword(event.currentTarget.value); }}
                  type={isPasswordVisible ? 'text' : 'password'}
                  value={password}
                />
                <Tooltip>
                  <Button
                    aria-label={isPasswordVisible ? tr('Hide password', '隐藏密码') : tr('Show password', '显示密码')}
                    className="absolute right-1 top-1/2 -translate-y-1/2"
                    isIconOnly
                    onPress={() => { setIsPasswordVisible((visible) => !visible); }}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    {isPasswordVisible
                      ? <EyeOff aria-hidden="true" className="size-4" />
                      : <Eye aria-hidden="true" className="size-4" />}
                  </Button>
                  <Tooltip.Content>
                    {isPasswordVisible ? tr('Hide password', '隐藏密码') : tr('Show password', '显示密码')}
                  </Tooltip.Content>
                </Tooltip>
              </div>
            </TextField>

            <Button
              aria-busy={isSubmitting}
              fullWidth
              isDisabled={isSubmitting}
              type="submit"
            >
              <span className="inline-flex min-h-5 items-center">{isSubmitting ? tr('Signing in…', '登录中…') : tr('Sign in', '登录')}</span>
            </Button>
          </form>

          <div className="mt-5 flex items-center gap-2 text-xs text-muted" role="status">
            <span
              aria-hidden="true"
              className={[
                'size-2 rounded-full',
                readiness === 'ready'
                  ? 'bg-success'
                  : readiness === 'unavailable'
                    ? 'bg-danger'
                    : 'bg-warning',
              ].join(' ')}
            />
            {readinessLabel}
          </div>
        </section>
      </main>
    </div>
  );
}
