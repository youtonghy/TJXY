import { Alert, Breadcrumbs, Button, Skeleton } from '@heroui/react';
import { ChevronRight, Folder, LoaderCircle, RefreshCw, TriangleAlert } from 'lucide-react';

export interface FolderChoice {
  id: string;
  name: string;
}

export interface FolderBrowserProps {
  ariaLabel: string;
  path: readonly FolderChoice[];
  directories: readonly FolderChoice[];
  isLoading: boolean;
  error: object | string | null;
  hasMore: boolean;
  isLoadingMore: boolean;
  isDisabled: boolean;
  onNavigate: (pathIndex: number) => void;
  onOpen: (folder: FolderChoice) => void;
  onLoadMore: () => void;
  onRetry: () => void;
}

export function FolderBrowser({
  ariaLabel,
  path,
  directories,
  isLoading,
  error,
  hasMore,
  isLoadingMore,
  isDisabled,
  onNavigate,
  onOpen,
  onLoadMore,
  onRetry,
}: FolderBrowserProps) {
  const currentFolder = path.at(-1);
  return (
    <section aria-label={ariaLabel} className="space-y-4">
      <div className="min-w-0 space-y-2">
        <div>
          <p className="text-xs font-semibold uppercase text-muted">Current folder</p>
          <p className="mt-1 break-words text-base font-semibold text-foreground">
            {currentFolder?.name ?? 'No folder selected'}
          </p>
        </div>
        {path.length > 0 && (
          <div className="max-w-full overflow-x-auto pb-1">
            <Breadcrumbs
              aria-label={`${ariaLabel} path`}
              className="min-w-max flex-nowrap"
              isDisabled={isDisabled}
            >
              {path.map((folder, index) => {
                const isCurrent = index === path.length - 1;
                return (
                  <Breadcrumbs.Item
                    isDisabled={isCurrent}
                    key={`${folder.id}-${String(index)}`}
                    onPress={isCurrent ? undefined : () => { onNavigate(index); }}
                  >
                    {folder.name}
                  </Breadcrumbs.Item>
                );
              })}
            </Breadcrumbs>
          </div>
        )}
      </div>

      {error !== null && (
        <Alert role="alert" status="danger">
          <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
          <Alert.Content>
            <Alert.Title>The folder list could not be loaded</Alert.Title>
            <Alert.Description>The current folder is unchanged. Retry when the provider is available.</Alert.Description>
          </Alert.Content>
          <Button isDisabled={isDisabled} onPress={onRetry} size="sm" variant="tertiary">
            <RefreshCw aria-hidden="true" className="size-4" />
            Retry
          </Button>
        </Alert>
      )}

      {isLoading ? (
        <div aria-label="Loading folders" className="space-y-2" role="status">
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-12 w-full" />
        </div>
      ) : directories.length > 0 ? (
        <ul aria-label={`${ariaLabel} folders`} className="divide-y divide-border border-y border-border">
          {directories.map((folder) => (
            <li key={folder.id}>
              <Button
                aria-label={`Open ${folder.name}`}
                className="min-h-12 w-full justify-start rounded-none px-2"
                isDisabled={isDisabled}
                onPress={() => { onOpen(folder); }}
                variant="ghost"
              >
                <Folder aria-hidden="true" className="size-4 shrink-0 text-muted" />
                <span className="min-w-0 flex-1 break-words text-left">{folder.name}</span>
                <ChevronRight aria-hidden="true" className="size-4 shrink-0 text-muted" />
              </Button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="border-y border-border py-8 text-center text-sm text-muted">
          {hasMore ? 'No folders on this page.' : 'This folder has no child folders.'}
        </p>
      )}

      {hasMore && (
        <div className="flex justify-center">
          <Button
            aria-label="Load more folders"
            isDisabled={isDisabled && !isLoadingMore}
            isPending={isLoadingMore}
            onPress={onLoadMore}
            variant="secondary"
          >
            {isLoadingMore
              ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" />
              : <RefreshCw aria-hidden="true" className="size-4" />}
            <span className="inline-flex min-h-5 items-center">Load more</span>
          </Button>
        </div>
      )}
    </section>
  );
}
