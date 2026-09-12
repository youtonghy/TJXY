import { Button, Modal } from '@heroui/react';
import { ArrowLeft, ChevronRight, File, Folder, FolderOpen, FolderPlus, RefreshCw, Trash2 } from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useEffect, useState } from 'react';

import { useTranslate } from '../settings/i18n';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import type { LibraryOption } from './libraryApi';
import { detachLibraryFolder, listFolderContents, listLibraryFolders, type FolderContents, type LibraryFolder } from './libraryFoldersApi';

export function StorageFoldersSection({ isPending, library, onChanged, onOpen }: { isPending: boolean; library: LibraryOption; onChanged: () => void; onOpen: () => void }) {
  const tr = useTranslate();
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [folders, setFolders] = useState<LibraryFolder[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [revision, setRevision] = useState(0);
  const [selected, setSelected] = useState<LibraryFolder | null>(null);
  const [detaching, setDetaching] = useState(false);
  const removeFolder = async (folder: LibraryFolder) => {
    setDetaching(true);
    try {
      await detachLibraryFolder(library.name, folder.id);
      setSelected((current) => (current?.id === folder.id ? null : current));
      setRevision((value) => value + 1);
      onChanged();
      notify(tr('Media folder removed.', '媒体文件夹已移除。'), { type: 'success' });
    } catch (error: unknown) {
      if (await logoutIfAccessDenied(error)) return;
      throw error;
    } finally {
      setDetaching(false);
    }
  };
  useEffect(() => {
    const controller = new AbortController();
    const isCurrent = () => !controller.signal.aborted;
    void Promise.resolve().then(async () => {
      if (!isCurrent()) return;
      setLoading(true);
      setError(false);
      try {
        const next = await listLibraryFolders(library.id, controller.signal);
        if (isCurrent()) setFolders(next);
      } catch (error: unknown) {
        if (isCurrent() && !(await logoutIfAccessDenied(error)) && isCurrent()) setError(true);
      } finally {
        if (isCurrent()) setLoading(false);
      }
    });
    return () => { controller.abort(); };
  }, [library, revision, logoutIfAccessDenied]);
  return (
    <section aria-labelledby="storage-folders-heading" className="space-y-5 border-t border-border py-7">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <h2 className="text-base font-semibold text-foreground" id="storage-folders-heading">{tr('Media folders', '媒体文件夹')}</h2>
            <span aria-label={tr('Folder count', '文件夹数量')} className="rounded-full bg-surface-secondary px-2.5 py-0.5 text-xs tabular-nums text-muted">{library.locations.length}</span>
          </div>
          <p className="mt-1 text-sm text-muted">{tr('Select a folder to preview its contents.', '点击文件夹，查看其中的目录与文件。')}</p>
        </div>
        <Button isPending={isPending} onPress={onOpen} variant="secondary"><FolderPlus aria-hidden="true" className="size-4" />{tr('Add folder', '添加文件夹')}</Button>
      </div>
      {loading ? <p role="status" className="text-sm text-muted">{tr('Loading folders…', '正在加载文件夹…')}</p> : error ? (
        <div role="alert" className="flex items-center gap-3 text-sm text-danger">{tr('Could not load folder paths.', '无法加载文件夹路径。')}<Button onPress={() => { setRevision((value) => value + 1); }} size="sm" variant="tertiary">{tr('Retry', '重试')}</Button></div>
      ) : folders.length === 0 ? <p className="rounded-xl border border-dashed border-border py-8 text-center text-sm text-muted">{tr('No media folders attached.', '尚未添加媒体文件夹。')}</p> : (
        <ul aria-label={tr('Attached media folders', '已添加的媒体文件夹')} className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {folders.map((folder) => (
            <li className="relative min-w-0" key={folder.id}>
              <Button aria-label={`${tr('Preview folder', '预览文件夹')} ${folder.path ?? folder.name}`} className="h-full min-h-36 w-full items-start justify-start whitespace-normal rounded-2xl border border-border bg-surface p-5 text-left shadow-sm" onPress={() => { setSelected(folder); }} variant="tertiary">
                <span className="flex min-w-0 flex-1 flex-col gap-3">
                  <span className="flex items-center gap-3 pr-9"><Folder aria-hidden="true" className="size-6 shrink-0 text-accent" /><span className="min-w-0 flex-1 break-words font-semibold">{folder.name}</span><ChevronRight aria-hidden="true" className="size-4 shrink-0 text-muted" /></span>
                  <span className="break-all font-mono text-xs font-normal leading-relaxed text-muted">{folder.path ?? folder.name}</span>
                  <span className="text-xs font-normal text-muted">{providerLabel(folder.provider, tr)}{library.unavailableLocations?.includes(`tjxy://storage-root/${folder.id}`) ? ` · ${tr('Unavailable', '不可用')}` : ''}</span>
                </span>
              </Button>
              <ConfirmDialog
                confirmLabel={tr('Remove folder', '移除文件夹')}
                description={(
                  <>
                    {tr('Remove', '将 ')}<strong className="break-all font-semibold text-foreground">{folder.path ?? folder.name}</strong>
                    {tr(' from this library? Items scanned from this folder, including their NFO metadata and images, will be removed from the catalog. Media files on disk are not deleted.', ' 从此媒体库中移除？该文件夹扫描入库的条目及其 NFO 元数据和图片将从目录中移除，但不会删除磁盘上的媒体文件。')}
                  </>
                )}
                isPending={detaching}
                onConfirm={() => removeFolder(folder)}
                title={tr('Remove media folder?', '移除媒体文件夹？')}
                trigger={(
                  <Button
                    aria-label={`${tr('Remove folder', '移除文件夹')} ${folder.path ?? folder.name}`}
                    className="absolute right-3 top-3"
                    isDisabled={isPending || detaching}
                    isIconOnly
                    size="sm"
                    variant="danger-soft"
                  >
                    <Trash2 aria-hidden="true" className="size-4" />
                  </Button>
                )}
              />
            </li>
          ))}
        </ul>
      )}
      {selected !== null && <FolderPreview folder={selected} key={selected.id} libraryId={library.id} onClose={() => { setSelected(null); }} />}
    </section>
  );
}

function FolderPreview({ folder, libraryId, onClose }: { folder: LibraryFolder; libraryId: string; onClose: () => void }) {
  const tr = useTranslate();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [trail, setTrail] = useState<{ name: string; path: string }[]>([]);
  const [contents, setContents] = useState<FolderContents | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const [revision, setRevision] = useState(0);
  const path = trail.at(-1)?.path ?? '';
  useEffect(() => {
    const controller = new AbortController();
    const isCurrent = () => !controller.signal.aborted;
    void Promise.resolve().then(async () => {
      if (!isCurrent()) return;
      setLoading(true);
      setError(null);
      try {
        const next = await listFolderContents(libraryId, folder.id, path, controller.signal);
        if (isCurrent()) setContents(next);
      } catch (error: unknown) {
        if (isCurrent() && !(await logoutIfAccessDenied(error)) && isCurrent()) setError(error);
      } finally {
        if (isCurrent()) setLoading(false);
      }
    });
    return () => { controller.abort(); };
  }, [libraryId, folder.id, path, revision, logoutIfAccessDenied]);
  const displayPath = [folder.path?.replace(/\/$/u, '') ?? folder.name, ...trail.map((entry) => entry.name)].join('/') || '/';
  const items = [...(contents?.items ?? [])].sort((a, b) => Number(b.isDirectory) - Number(a.isDirectory) || a.name.localeCompare(b.name, undefined, { numeric: true }));
  const directories = items.filter((entry) => entry.isDirectory).length;
  return (
    <Modal isOpen onOpenChange={(open) => { if (!open) onClose(); }}>
      <Modal.Backdrop><Modal.Container size="lg"><Modal.Dialog className="max-h-[calc(100dvh-2rem)] sm:w-[min(44rem,calc(100vw-5rem))] sm:max-w-3xl">
        <Modal.CloseTrigger aria-label={tr('Close folder preview', '关闭文件夹预览')} />
        <Modal.Header><Modal.Heading>{tr('Folder contents', '文件夹内容')}</Modal.Heading><p className="break-all font-mono text-xs text-muted">{displayPath}</p></Modal.Header>
        <Modal.Body className="min-h-0 overflow-y-auto">
          <div className="mb-4 flex items-center justify-between gap-3">
            <Button isDisabled={trail.length === 0} onPress={() => { setTrail((value) => value.slice(0, -1)); }} size="sm" variant="tertiary"><ArrowLeft aria-hidden="true" className="size-4" />{tr('Parent folder', '上级文件夹')}</Button>
            <Button aria-label={tr('Refresh folder contents', '刷新文件夹内容')} isIconOnly isPending={loading} onPress={() => { setRevision((value) => value + 1); }} size="sm" variant="ghost"><RefreshCw aria-hidden="true" className="size-4" /></Button>
          </div>
          {loading ? <p role="status" className="py-10 text-center text-sm text-muted">{tr('Loading contents…', '正在加载内容…')}</p> : error !== null ? (
            <div role="alert" className="space-y-3 py-6 text-sm text-danger"><p>{typeof error === 'object' && 'status' in error && error.status === 413 ? tr('This directory exceeds the 10,000-entry preview limit.', '此目录超过 10,000 项预览上限。') : tr('This folder could not be read. Check that it exists and is accessible.', '无法读取此文件夹，请检查目录是否存在且可访问。')}</p><Button onPress={() => { setRevision((value) => value + 1); }} variant="tertiary">{tr('Retry', '重试')}</Button></div>
          ) : (
            <>
              <p className="mb-3 text-sm text-muted" role="status">{tr(`${String(directories)} folders · ${String(items.length - directories)} files`, `${String(directories)} 个文件夹 · ${String(items.length - directories)} 个文件`)}</p>
              {contents?.indexed && <p className="mb-3 text-xs text-muted">{tr('Synchronized cloud inventory; unsynchronized files may not appear yet. Paths are relative to the attached cloud folder.', '云盘已同步内容，尚未同步的文件可能暂未显示；路径以已添加的云盘文件夹为起点。')}</p>}
              {items.length === 0 ? <div className="flex flex-col items-center gap-3 py-12 text-muted"><FolderOpen aria-hidden="true" className="size-8" /><p className="text-sm">{contents?.indexed ? tr('No synchronized entries yet.', '暂无已同步的内容。') : tr('This folder is empty.', '此文件夹为空。')}</p></div> : (
                <ul aria-label={tr('Directory entries', '目录条目')} className="divide-y divide-border rounded-xl border border-border">
                  {items.map((entry) => {
                    const details = <><span className="flex min-w-0 flex-1 items-center gap-3">{entry.isDirectory ? <Folder aria-hidden="true" className="size-5 shrink-0 text-accent" /> : <File aria-hidden="true" className="size-5 shrink-0 text-muted" />}<span className="min-w-0"><span className="block break-all text-sm font-medium">{entry.name}</span><span className="mt-1 block text-xs font-normal text-muted">{entry.isDirectory ? tr('Folder', '文件夹') : `${entry.name.includes('.') ? entry.name.split('.').at(-1)?.toUpperCase() ?? tr('File', '文件') : tr('File', '文件')} · ${formatSize(entry.size)}`}{entry.modifiedAt !== null ? ` · ${new Date(entry.modifiedAt).toLocaleString()}` : ''}</span></span></span>{entry.isDirectory && <ChevronRight aria-hidden="true" className="size-4 shrink-0 text-muted" />}</>;
                    return <li key={entry.path}>{entry.isDirectory ? <Button aria-label={`${tr('Open folder', '打开文件夹')} ${entry.name}`} className="h-auto min-h-16 w-full justify-start whitespace-normal rounded-none px-4 py-3 text-left" onPress={() => { setTrail((value) => [...value, { name: entry.name, path: entry.path }]); }} variant="ghost">{details}</Button> : <div className="flex min-h-16 items-center px-4 py-3">{details}</div>}</li>;
                  })}
                </ul>
              )}
            </>
          )}
        </Modal.Body>
        <Modal.Footer><Button onPress={onClose} variant="tertiary">{tr('Close', '关闭')}</Button></Modal.Footer>
      </Modal.Dialog></Modal.Container></Modal.Backdrop>
    </Modal>
  );
}

function formatSize(bytes: number | null) {
  if (bytes === null) return '—';
  if (bytes < 1024) return `${String(bytes)} B`;
  const power = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), 4);
  return `${(bytes / 1024 ** power).toFixed(1)} ${['B', 'KB', 'MB', 'GB', 'TB'][power] ?? 'B'}`;
}
function providerLabel(provider: string, tr: ReturnType<typeof useTranslate>) {
  if (provider === 'filesystem') return tr('Server folder', '服务器文件夹');
  if (provider === 'google-drive') return 'Google Drive';
  if (provider === 'onedrive') return 'OneDrive';
  return provider;
}
