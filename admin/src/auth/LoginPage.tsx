import {
  Alert,
  Button,
  Input,
  Label,
  TextField,
  Tooltip,
} from '@heroui/react';
import { CircleAlert, Eye, EyeOff } from 'lucide-react';
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
  const mountedRef = useRef(true);

  useEffect(() => {
    document.title = 'Sign in | TJXY Admin';
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

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
    ? 'Checking server'
    : readiness === 'ready'
      ? 'Server ready'
      : 'Server unavailable';

  return (
    <div className="min-h-screen bg-background p-4 sm:p-6 lg:p-8">
      <header
        className="mb-8 flex items-center gap-3 lg:fixed lg:left-8 lg:top-8 lg:mb-0"
        data-testid="login-brand"
      >
        <BrandMark className="size-10" priority />
        <span>
          <span className="block text-sm font-semibold text-foreground">TJXY Admin</span>
          <span className="block text-xs text-muted">Administrator workspace</span>
        </span>
      </header>

      <main className="flex min-h-[calc(100vh-2rem)] items-center justify-center sm:min-h-[calc(100vh-3rem)] lg:min-h-[calc(100vh-4rem)]">
        <section className="w-full max-w-[380px] rounded-lg border border-border bg-surface p-6 shadow-sm sm:p-7">
          <div className="mb-6">
            <h1 className="text-2xl font-semibold text-foreground">Administrator sign in</h1>
            <p className="mt-1 text-sm leading-6 text-muted">
              Use an enabled administrator account.
            </p>
          </div>

          <form
            aria-describedby={submissionError ? 'login-error' : undefined}
            aria-label="Administrator sign in"
            className="space-y-4"
            onSubmit={(event) => { void handleSubmit(event); }}
          >
            {submissionError && (
              <Alert id="login-error" role="alert" status="danger">
                <Alert.Indicator>
                  <CircleAlert aria-hidden="true" className="size-4" />
                </Alert.Indicator>
                <Alert.Content>
                  <Alert.Title>Sign in failed</Alert.Title>
                  <Alert.Description>Check your credentials and try again.</Alert.Description>
                </Alert.Content>
              </Alert>
            )}

            <TextField fullWidth isRequired name="username">
              <Label>Username</Label>
              <Input
                autoComplete="username"
                fullWidth
                onChange={(event) => { setUsername(event.currentTarget.value); }}
                value={username}
              />
            </TextField>

            <TextField fullWidth isRequired name="password">
              <Label>Password</Label>
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
                    aria-label={isPasswordVisible ? 'Hide password' : 'Show password'}
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
                    {isPasswordVisible ? 'Hide password' : 'Show password'}
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
              <span className="inline-flex min-h-5 items-center">Sign in</span>
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
