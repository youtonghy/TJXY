import { Alert, Breadcrumbs, Button, Modal, Skeleton, Tooltip } from '@heroui/react';
import { FileTree } from '@heroui-pro/react/file-tree';
import { ListView } from '@heroui-pro/react/list-view';
import {
  ArrowLeft,
  ChevronRight,
  Folder,
  FolderOpen,
  HardDrive,
  RefreshCw,
  TriangleAlert,
} from 'lucide-react';
import { useCallback, useEffect, useState, type ReactNode } from 'react';

import { useTranslate } from '../settings/i18n';
import type { FilesystemDirectory, FilesystemRoot, FilesystemSelection } from './filesystemApi';
import { listFilesystemDirectories, listFilesystemRoots } from './filesystemApi';

interface TrailEntry {
  name: string;
  relativePath: string;
}

export interface FolderPickerDialogProps {
  isOpen: boolean;
  isDisabled?: boolean;
  onClose: () => void;
  onSelect: (selection: FilesystemSelection, displayPath: string) => void;
}

export function FolderPickerDialog({
  isOpen,
  isDisabled = false,
  onClose,
  onSelect,
}: FolderPickerDialogProps) {
  const tr = useTranslate();
  const [roots, setRoots] = useState<FilesystemRoot[]>([]);
  const [root, setRoot] = useState<FilesystemRoot | null>(null);
  const [trail, setTrail] = useState<TrailEntry[]>([]);
  const [directories, setDirectories] = useState<FilesystemDirectory[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);

  const loadDirectory = useCallback(async (
    nextRoot: FilesystemRoot,
    nextTrail: TrailEntry[],
    signal?: AbortSignal,
  ) => {
    setLoading(true);
    setError(false);
    try {
      const currentPath = nextTrail.at(-1)?.relativePath ?? '';
      const items = await listFilesystemDirectories(nextRoot.id, currentPath, signal);
      setRoot(nextRoot);
      setTrail(nextTrail);
      setDirectories(items);
    } catch (loadError: unknown) {
      if (!(loadError instanceof DOMException && loadError.name === 'AbortError')) setError(true);
    } finally {
      if (signal?.aborted !== true) setLoading(false);
    }
  }, []);

  const loadRoots = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    setError(false);
    try {
      const nextRoots = await listFilesystemRoots(signal);
      if (signal?.aborted === true) return;
      setRoots(nextRoots);
      if (nextRoots[0] !== undefined) {
        await loadDirectory(nextRoots[0], [], signal);
      } else {
        setRoot(null);
        setDirectories([]);
        setLoading(false);
      }
    } catch (loadError: unknown) {
      if (!(loadError instanceof DOMException && loadError.name === 'AbortError')) setError(true);
      if (signal?.aborted !== true) setLoading(false);
    }
  }, [loadDirectory]);

  useEffect(() => {
    if (!isOpen) return undefined;
    const controller = new AbortController();
    void Promise.resolve().then(() => {
      if (controller.signal.aborted) return undefined;
      return loadRoots(controller.signal);
    });
    return () => { controller.abort(); };
  }, [isOpen, loadRoots]);

  const navigateToRoot = (nextRoot: FilesystemRoot) => {
    void loadDirectory(nextRoot, []);
  };
  const openDirectory = (directory: FilesystemDirectory) => {
    if (root === null) return;
    void loadDirectory(root, [...trail, {
      name: directory.name,
      relativePath: directory.relativePath,
    }]);
  };
  const navigateTrail = (index: number) => {
    if (root === null) return;
    void loadDirectory(root, trail.slice(0, index + 1));
  };
  const goBack = () => {
    if (root === null || trail.length === 0) return;
    void loadDirectory(root, trail.slice(0, -1));
  };
  const retry = () => {
    if (root === null) {
      void loadRoots();
    } else {
      void loadDirectory(root, trail);
    }
  };
  const select = () => {
    if (root === null) return;
    onSelect(
      { rootId: root.id, relativePath: trail.at(-1)?.relativePath ?? '' },
      [root.name, ...trail.map((entry) => entry.name)].join(' / '),
    );
    onClose();
  };

  return (
    <Modal isOpen={isOpen} onOpenChange={(open) => { if (!open && !isDisabled) onClose(); }}>
      <Modal.Backdrop isDismissable={!isDisabled} isKeyboardDismissDisabled={isDisabled}>
        <Modal.Container placement="center" size="lg">
          <Modal.Dialog className="max-h-[min(48rem,calc(100vh-2rem))]">
            <Modal.CloseTrigger aria-label={tr('Close folder picker', '关闭文件夹选择器')} isDisabled={isDisabled} />
            <Modal.Header>
              <Modal.Heading>{tr('Select media folder', '选择媒体文件夹')}</Modal.Heading>
            </Modal.Header>
            <Modal.Body className="min-h-0">
              {error && (
                <Alert role="alert" status="danger">
                  <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
                  <Alert.Content>
                    <Alert.Title>{tr('The folder list could not be loaded', '无法加载文件夹列表')}</Alert.Title>
                    <Alert.Description>{tr('Check that the server media roots are available.', '请检查服务器媒体根目录是否可用。')}</Alert.Description>
                  </Alert.Content>
                  <Button onPress={retry} size="sm" variant="tertiary">
                    <RefreshCw aria-hidden="true" className="size-4" /> {tr('Retry', '重试')}
                  </Button>
                </Alert>
              )}

              <div className="grid min-h-96 overflow-hidden border border-border md:grid-cols-[15rem_minmax(0,1fr)]">
                <aside className="min-h-0 overflow-auto border-b border-border bg-surface-secondary p-3 md:border-b-0 md:border-r">
                  <p className="mb-2 px-2 text-xs font-semibold uppercase text-muted">{tr('Server folders', '服务器文件夹')}</p>
                  <FileTree
                    aria-label={tr('Server folder tree', '服务器文件夹树')}
                    className="min-w-0"
                    expandedKeys={root === null ? [] : [
                      `root:${root.id}`,
                      ...trail.map((entry) => `path:${root.id}:${entry.relativePath}`),
                    ]}
                    onAction={(key) => {
                      const value = String(key);
                      const nextRoot = roots.find((candidate) => `root:${candidate.id}` === value);
                      if (nextRoot !== undefined) navigateToRoot(nextRoot);
                      const pathIndex = trail.findIndex((entry) => `path:${root?.id ?? ''}:${entry.relativePath}` === value);
                      if (pathIndex >= 0) navigateTrail(pathIndex);
                    }}
                    showGuideLines="hover"
                  >
                    {roots.map((candidate) => (
                      <FileTree.Item
                        id={`root:${candidate.id}`}
                        icon={candidate.id === root?.id ? <FolderOpen aria-hidden="true" /> : <HardDrive aria-hidden="true" />}
                        key={candidate.id}
                        title={candidate.name}
                      >
                        {candidate.id === root?.id ? renderTrail(root.id, trail, 0) : null}
                      </FileTree.Item>
                    ))}
                  </FileTree>
                </aside>

                <section aria-label={tr('Folder contents', '文件夹内容')} className="flex min-h-0 min-w-0 flex-col">
                  <div className="flex min-h-14 items-center gap-2 border-b border-border px-3">
                    <Tooltip>
                      <Button
                        aria-label={tr('Go to parent folder', '返回上级文件夹')}
                        isDisabled={trail.length === 0 || loading}
                        isIconOnly
                        onPress={goBack}
                        size="sm"
                        variant="ghost"
                      >
                        <ArrowLeft aria-hidden="true" className="size-4" />
                      </Button>
                      <Tooltip.Content>{tr('Parent folder', '上级文件夹')}</Tooltip.Content>
                    </Tooltip>
                    <div className="min-w-0 flex-1 overflow-x-auto">
                      <Breadcrumbs aria-label={tr('Selected folder path', '所选文件夹路径')} className="min-w-max flex-nowrap">
                        {root !== null && (
                          <Breadcrumbs.Item isDisabled={trail.length === 0} onPress={() => { navigateToRoot(root); }}>
                            {root.name}
                          </Breadcrumbs.Item>
                        )}
                        {trail.map((entry, index) => (
                          <Breadcrumbs.Item
                            isDisabled={index === trail.length - 1}
                            key={entry.relativePath}
                            onPress={() => { navigateTrail(index); }}
                          >
                            {entry.name}
                          </Breadcrumbs.Item>
                        ))}
                      </Breadcrumbs>
                    </div>
                    <Tooltip>
                      <Button
                        aria-label={tr('Refresh folder', '刷新文件夹')}
                        isDisabled={root === null}
                        isIconOnly
                        isPending={loading}
                        onPress={retry}
                        size="sm"
                        variant="ghost"
                      >
                        <RefreshCw aria-hidden="true" className={`size-4${loading ? ' animate-spin' : ''}`} />
                      </Button>
                      <Tooltip.Content>{tr('Refresh folder', '刷新文件夹')}</Tooltip.Content>
                    </Tooltip>
                  </div>
                  <div className="min-h-0 flex-1 overflow-auto p-2">
                    {loading ? (
                      <div aria-label={tr('Loading folder contents', '正在加载文件夹内容')} className="space-y-2 p-2" role="status">
                        <Skeleton className="h-12 w-full" />
                        <Skeleton className="h-12 w-full" />
                        <Skeleton className="h-12 w-full" />
                      </div>
                    ) : (
                      <ListView
                        aria-label={tr('Folder list view', '文件夹列表')}
                        onAction={(key) => {
                          const directory = directories.find((item) => item.relativePath === String(key));
                          if (directory !== undefined) openDirectory(directory);
                        }}
                        renderEmptyState={() => (
                          <p className="py-12 text-center text-sm text-muted">{tr('This folder has no child folders.', '该文件夹没有子文件夹。')}</p>
                        )}
                        selectionMode="none"
                        variant="secondary"
                      >
                        {directories.map((directory) => (
                          <ListView.Item id={directory.relativePath} key={directory.relativePath} textValue={directory.name}>
                            <Folder aria-hidden="true" className="size-5 shrink-0 text-accent" />
                            <ListView.ItemContent>
                              <ListView.Title>{directory.name}</ListView.Title>
                              <ListView.Description>{formatModified(directory.modifiedAt, tr)}</ListView.Description>
                            </ListView.ItemContent>
                            <ListView.ItemAction><ChevronRight aria-hidden="true" className="size-4 text-muted" /></ListView.ItemAction>
                          </ListView.Item>
                        ))}
                      </ListView>
                    )}
                  </div>
                </section>
              </div>
            </Modal.Body>
            <Modal.Footer>
              <Button isDisabled={isDisabled} onPress={onClose} variant="tertiary">{tr('Cancel', '取消')}</Button>
              <Button isDisabled={root === null || loading || isDisabled} onPress={select}>
                <FolderOpen aria-hidden="true" className="size-4" /> {tr('Select folder', '选择文件夹')}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function renderTrail(rootId: string, trail: TrailEntry[], index: number): ReactNode {
  const entry = trail[index];
  if (entry === undefined) return null;
  return (
    <FileTree.Item
      id={`path:${rootId}:${entry.relativePath}`}
      icon={<FolderOpen aria-hidden="true" />}
      key={entry.relativePath}
      title={entry.name}
    >
      {renderTrail(rootId, trail, index + 1)}
    </FileTree.Item>
  );
}

function formatModified(
  value: string | null,
  tr: (english: string, chinese: string) => string,
): string {
  if (value === null) return tr('Folder', '文件夹');
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? tr('Folder', '文件夹')
    : tr(`Modified ${date.toLocaleDateString()}`, `修改于 ${date.toLocaleDateString()}`);
}
