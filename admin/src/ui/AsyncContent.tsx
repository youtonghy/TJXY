import { Alert, Button } from '@heroui/react';
import { RefreshCw, TriangleAlert } from 'lucide-react';
import type { ReactNode } from 'react';

import { PageError } from './SystemPages';

export interface AsyncContentProps {
  isPending: boolean;
  error: NonNullable<unknown> | null;
  hasData: boolean;
  isEmpty: boolean;
  onRetry: () => void;
  loading: ReactNode;
  empty: ReactNode;
  children: ReactNode;
}

export function AsyncContent({
  isPending,
  error,
  hasData,
  isEmpty,
  onRetry,
  loading,
  empty,
  children,
}: AsyncContentProps) {
  if (isPending && !hasData) return loading;
  if (error !== null && !hasData) {
    return <PageError error={error} headingLevel={2} onRetry={onRetry} />;
  }

  return (
    <>
      {error !== null && <StaleDataAlert onRetry={onRetry} />}
      {isEmpty ? empty : children}
    </>
  );
}

export function StaleDataAlert({ onRetry }: { onRetry: () => void }) {
  return (
    <Alert className="mb-4" role="alert" status="warning">
      <Alert.Indicator>
        <TriangleAlert aria-hidden="true" className="size-4" />
      </Alert.Indicator>
      <Alert.Content>
        <Alert.Title>Showing the last available data</Alert.Title>
        <Alert.Description>
          The latest refresh failed. Existing information remains available.
        </Alert.Description>
      </Alert.Content>
      <Button onPress={onRetry} size="sm" variant="tertiary">
        <RefreshCw aria-hidden="true" className="size-4" />
        Retry refresh
      </Button>
    </Alert>
  );
}
