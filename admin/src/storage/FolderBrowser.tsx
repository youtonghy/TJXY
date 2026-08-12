import { Alert, Breadcrumbs, Button, Skeleton } from '@heroui/react';
import { ChevronRight, Folder, LoaderCircle, RefreshCw, TriangleAlert } from 'lucide-react';
import { useTranslate } from '../settings/i18n';

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
  const tr = useTranslate();
  const currentFolder = path.at(-1);
  return (
    <section aria-label={ariaLabel} className="space-y-4">
      <div className="min-w-0 space-y-2">
        <div>
          <p className="text-xs font-semibold uppercase text-muted">{tr('Current folder', '当前文件夹')}</p>
          <p className="mt-1 break-words text-base font-semibold text-foreground">
            {currentFolder?.name ?? tr('No folder selected', '未选择文件夹')}
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
            <Alert.Title>{tr('The folder list could not be loaded', '无法加载文件夹列表')}</Alert.Title>
            <Alert.Description>{tr('The current folder is unchanged. Retry when the provider is available.', '当前文件夹未改变。服务可用后请重试。')}</Alert.Description>
          </Alert.Content>
          <Button isDisabled={isDisabled} onPress={onRetry} size="sm" variant="tertiary">
            <RefreshCw aria-hidden="true" className="size-4" />
            {tr('Retry', '重试')}
          </Button>
        </Alert>
      )}

      {isLoading ? (
        <div aria-label={tr('Loading folders', '正在加载文件夹')} className="space-y-2" role="status">
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-12 w-full" />
        </div>
      ) : directories.length > 0 ? (
        <ul aria-label={`${ariaLabel} folders`} className="divide-y divide-border border-y border-border">
          {directories.map((folder) => (
            <li key={folder.id}>
              <Button
                aria-label={`${tr('Open', '打开')} ${folder.name}`}
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
          {hasMore ? tr('No folders on this page.', '此页没有文件夹。') : tr('This folder has no child folders.', '此文件夹没有子文件夹。')}
        </p>
      )}

      {hasMore && (
        <div className="flex justify-center">
          <Button
            aria-label={tr('Load more folders', '加载更多文件夹')}
            isDisabled={isDisabled && !isLoadingMore}
            isPending={isLoadingMore}
            onPress={onLoadMore}
            variant="secondary"
          >
            {isLoadingMore
              ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" />
              : <RefreshCw aria-hidden="true" className="size-4" />}
            <span className="inline-flex min-h-5 items-center">{tr('Load more', '加载更多')}</span>
          </Button>
        </div>
      )}
    </section>
  );
}
