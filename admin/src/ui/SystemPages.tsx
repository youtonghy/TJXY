import { Alert, Button, Spinner } from '@heroui/react';
import { ArrowLeft, LogOut, RefreshCw, TriangleAlert } from 'lucide-react';
import { useState, type ErrorInfo, type ReactNode } from 'react';
import { useLogout } from 'ra-core';
import { Link } from 'react-router-dom';

interface SystemPageProps {
  title: string;
  description: string;
  children: ReactNode;
  role?: 'alert' | 'status';
  headingLevel?: 1 | 2;
}

function SystemPage({
  title,
  description,
  children,
  role,
  headingLevel = 1,
}: SystemPageProps) {
  const Heading = headingLevel === 1 ? 'h1' : 'h2';
  return (
    <section
      className="mx-auto flex min-h-72 w-full max-w-xl flex-col items-start justify-center gap-4 py-10"
      role={role}
    >
      <div className="space-y-2">
        <Heading className="text-2xl font-semibold text-foreground">{title}</Heading>
        <p className="max-w-lg text-sm leading-6 text-muted">{description}</p>
      </div>
      {children}
    </section>
  );
}

function UsersLink() {
  return (
    <Link
      className="inline-flex min-h-9 items-center gap-2 text-sm font-medium text-accent hover:underline"
      to="/admin/users"
    >
      <ArrowLeft aria-hidden="true" className="size-4" />
      Back to Users
    </Link>
  );
}

export function LoadingPage() {
  return (
    <SystemPage
      description="Core resources and permissions are being prepared."
      role="status"
      title="Preparing the admin workspace"
    >
      <Spinner aria-label="Loading admin workspace" color="accent" />
      <UsersLink />
    </SystemPage>
  );
}

export interface PageErrorProps {
  error: unknown;
  onRetry: () => void;
  headingLevel?: 1 | 2;
}

export function PageError({ onRetry, headingLevel = 1 }: PageErrorProps) {
  return (
    <SystemPage
      description="The request did not complete. No existing data has been replaced."
      headingLevel={headingLevel}
      role="alert"
      title="Unable to load this content"
    >
      <Button onPress={onRetry} variant="secondary">
        <RefreshCw aria-hidden="true" className="size-4" />
        Retry
      </Button>
    </SystemPage>
  );
}

export function AccessDeniedPage() {
  const logout = useLogout();
  const [isPending, setIsPending] = useState(false);
  const [hasError, setHasError] = useState(false);

  const handleSignOut = async () => {
    setIsPending(true);
    setHasError(false);
    try {
      await logout({}, undefined, false);
    } catch {
      setHasError(true);
      setIsPending(false);
    }
  };

  return (
    <SystemPage
      description="Your session is valid, but this account cannot open the requested admin area."
      title="Access denied"
    >
      {hasError && (
        <Alert role="alert" status="danger">
          <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
          <Alert.Content>
            <Alert.Title>Sign out failed</Alert.Title>
            <Alert.Description>Try again before leaving this page.</Alert.Description>
          </Alert.Content>
        </Alert>
      )}
      <Button
        aria-busy={isPending}
        isDisabled={isPending}
        onPress={() => { void handleSignOut(); }}
        variant="secondary"
      >
        <LogOut aria-hidden="true" className="size-4" />
        Sign out
      </Button>
    </SystemPage>
  );
}

export function AuthenticationErrorPage() {
  return (
    <SystemPage
      description="The administrator session could not be verified. Start a new sign-in attempt."
      title="Authentication required"
    >
      <Link
        className="inline-flex min-h-9 items-center gap-2 text-sm font-medium text-accent hover:underline"
        to="/app/login?redirect=%2Fadmin"
      >
        Go to sign in
      </Link>
    </SystemPage>
  );
}

export function NotFoundPage() {
  return (
    <SystemPage
      description="The requested admin page does not exist or has moved."
      title="Page not found"
    >
      <UsersLink />
    </SystemPage>
  );
}

export interface ApplicationErrorProps {
  error: Error;
  errorInfo?: ErrorInfo;
  resetErrorBoundary: (...args: unknown[]) => void;
}

export function ApplicationError({ resetErrorBoundary }: ApplicationErrorProps) {
  return (
    <SystemPage
      description="The interface stopped unexpectedly. Retry without exposing internal error details."
      role="alert"
      title="The admin interface could not continue"
    >
      <Button onPress={() => { resetErrorBoundary(); }} variant="secondary">
        <RefreshCw aria-hidden="true" className="size-4" />
        Try again
      </Button>
    </SystemPage>
  );
}
