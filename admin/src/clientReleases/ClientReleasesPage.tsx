/* eslint-disable react-hooks/set-state-in-effect */
import {
  Alert,
  Button,
  Chip,
  Input,
  Label,
  ListBox,
  Modal,
  Select,
  Skeleton,
  Switch,
  Table,
  TextArea,
  TextField,
} from '@heroui/react';
import { Edit3, MonitorDown, Plus, RefreshCw, Star, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState, type SyntheticEvent } from 'react';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { PageHeader } from '../ui/PageHeader';
import { useTranslate } from '../settings/i18n';
import {
  CLIENT_ARCHES,
  CLIENT_PACKAGE_EXTENSIONS,
  CLIENT_PLATFORMS,
  deleteClientRelease,
  isArch,
  isClientVersion,
  isPlatform,
  listClientReleases,
  markClientReleaseLatest,
  packageExtension,
  updateClientReleaseNotes,
  uploadClientRelease,
  type ClientArch,
  type ClientPlatform,
  type ClientRelease,
} from './clientReleaseApi';
import { formatBytes, guessTarget } from './releaseFormat';

const PLATFORM_LABELS: Record<ClientPlatform, string> = { windows: 'Windows', macos: 'macOS', linux: 'Linux', android: 'Android' };

export function ClientReleasesPage() {
  const tr = useTranslate();
  const [items, setItems] = useState<ClientRelease[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [editing, setEditing] = useState<ClientRelease | null>(null);
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [actionFailed, setActionFailed] = useState(false);

  const load = useCallback(async () => {
    setLoading(true); setError(false);
    try { setItems(await listClientReleases()); }
    catch { setError(true); }
    finally { setLoading(false); }
  }, []);

  useEffect(() => { void load(); }, [load]);

  const markLatest = async (item: ClientRelease) => {
    setPendingId(item.id); setActionFailed(false);
    try { await markClientReleaseLatest(item.id); await load(); }
    catch { setActionFailed(true); }
    finally { setPendingId(null); }
  };

  return (
    <div className="space-y-5">
      <PageHeader
        actions={<><Button aria-label={tr('Reload client releases', '重新加载客户端版本')} isIconOnly isPending={loading} onPress={() => { void load(); }} variant="ghost"><RefreshCw className="size-4" /></Button><Button onPress={() => { setUploading(true); }}><Plus className="size-4" />{tr('Upload release', '上传版本')}</Button></>}
        description={tr('Upload client installers and choose the latest version that desktop and Android clients update to.', '上传客户端安装包，并指定桌面端和 Android 客户端要更新到的最新版本。')}
        title={tr('Client releases', '客户端版本')}
      />
      {error && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('Client releases could not be loaded', '无法加载客户端版本')}</Alert.Title><Alert.Description>{tr('Review the connection and try again.', '请检查连接后重试。')}</Alert.Description></Alert.Content><Button onPress={() => { void load(); }} size="sm" variant="tertiary">{tr('Retry', '重试')}</Button></Alert>}
      {actionFailed && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('The release could not be updated', '无法更新该版本')}</Alert.Title><Alert.Description>{tr('Reload the list and try again.', '请重新加载列表后重试。')}</Alert.Description></Alert.Content></Alert>}
      {loading && items.length === 0 ? <ReleaseSkeleton /> : items.length === 0 ? <EmptyReleases onUpload={() => { setUploading(true); }} /> : (
        <Table variant="secondary">
          <Table.ScrollContainer>
            <Table.Content aria-label={tr('Client releases', '客户端版本')} className="min-w-[60rem] table-fixed">
              <Table.Header><Table.Column isRowHeader>{tr('Version', '版本')}</Table.Column><Table.Column>{tr('Platform', '平台')}</Table.Column><Table.Column>{tr('Package', '安装包')}</Table.Column><Table.Column>{tr('Status', '状态')}</Table.Column><Table.Column>{tr('Uploaded', '上传时间')}</Table.Column><Table.Column>{tr('Actions', '操作')}</Table.Column></Table.Header>
              <Table.Body>{items.map((item) => (
                <Table.Row id={item.id} key={item.id}>
                  <Table.Cell><div className="min-w-0"><span className="block truncate font-medium tabular-nums">{item.version}</span>{item.notes.trim().length > 0 && <span className="block truncate text-xs text-muted">{item.notes}</span>}</div></Table.Cell>
                  <Table.Cell>{PLATFORM_LABELS[item.platform]} · {item.arch}</Table.Cell>
                  <Table.Cell><div className="min-w-0"><span className="block truncate">{item.fileName}</span><span className="block truncate font-mono text-xs text-muted" title={item.sha256}>{formatBytes(item.size)} · {item.sha256.slice(0, 12)}</span></div></Table.Cell>
                  <Table.Cell>{item.isLatest ? <Chip color="success" size="sm" variant="soft">{tr('Latest', '最新版')}</Chip> : <Chip size="sm" variant="soft">{tr('Archived', '历史版本')}</Chip>}</Table.Cell>
                  <Table.Cell>{formatDate(item.createdAt)}</Table.Cell>
                  <Table.Cell><div className="flex items-center gap-1">
                    {!item.isLatest && <Button aria-label={`${tr('Set as latest', '设为最新版')} ${item.version}`} isIconOnly isPending={pendingId === item.id} onPress={() => { void markLatest(item); }} size="sm" variant="ghost"><Star className="size-4" /></Button>}
                    <Button aria-label={`${tr('Edit notes', '编辑更新说明')} ${item.version}`} isIconOnly onPress={() => { setEditing(item); }} size="sm" variant="ghost"><Edit3 className="size-4" /></Button>
                    <ConfirmDialog
                      confirmLabel={tr('Delete permanently', '永久删除')}
                      description={<>{tr('Delete', '删除')} <strong>{item.fileName}</strong>{item.isLatest ? tr('. Clients on this platform will have no update until another release is set as latest.', '。在另一个版本被设为最新版之前，该平台的客户端将无法获取更新。') : tr('. This cannot be undone.', '。此操作无法撤销。')}</>}
                      errorDescription={tr('The release was not deleted. Reload it and try again.', '版本未删除，请重新加载后重试。')}
                      isPending={pendingId === item.id}
                      onConfirm={async () => { setPendingId(item.id); try { await deleteClientRelease(item.id); await load(); } finally { setPendingId(null); } }}
                      title={tr('Delete client release?', '删除客户端版本？')}
                      trigger={<Button aria-label={`${tr('Delete', '删除')} ${item.version}`} isIconOnly size="sm" variant="ghost"><Trash2 className="size-4 text-danger" /></Button>}
                    />
                  </div></Table.Cell>
                </Table.Row>
              ))}</Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      )}
      {uploading && <UploadDialog onClose={() => { setUploading(false); }} onUploaded={async () => { setUploading(false); await load(); }} />}
      {editing !== null && <NotesDialog key={editing.id} onClose={() => { setEditing(null); }} onSaved={async () => { setEditing(null); await load(); }} release={editing} />}
    </div>
  );
}

function UploadDialog({ onClose, onUploaded }: { onClose: () => void; onUploaded: () => Promise<void> }) {
  const tr = useTranslate();
  const [file, setFile] = useState<File | null>(null);
  const [platform, setPlatform] = useState<ClientPlatform>('windows');
  const [arch, setArch] = useState<ClientArch>('x64');
  const [version, setVersion] = useState('');
  const [notes, setNotes] = useState('');
  const [markLatest, setMarkLatest] = useState(true);
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const extensionValid = file !== null && packageExtension(platform, file.name) !== null;
  const versionValid = isClientVersion(version.trim());

  const choose = (selected: File | undefined) => {
    setFailed(null);
    if (selected === undefined) { setFile(null); return; }
    setFile(selected);
    const guess = guessTarget(selected.name);
    if (guess.platform !== undefined) setPlatform(guess.platform);
    if (guess.arch !== undefined) setArch(guess.arch);
    if (guess.version !== undefined && version.trim().length === 0) setVersion(guess.version);
  };

  const submit = async (event: SyntheticEvent<HTMLFormElement, SubmitEvent>) => {
    event.preventDefault();
    if (pending || file === null || !extensionValid || !versionValid) return;
    setPending(true); setFailed(null);
    try { await uploadClientRelease({ platform, arch, version, file, notes, markLatest }); await onUploaded(); }
    catch (reason) {
      const status = typeof reason === 'object' && reason !== null && 'status' in reason ? reason.status : 0;
      setFailed(status === 409 ? tr('This version already exists for the selected platform and architecture.', '所选平台和架构已存在该版本。') : status === 413 ? tr('The package is larger than the 2 GiB limit.', '安装包超过 2 GiB 上限。') : tr('The release could not be uploaded. Check the package and try again.', '无法上传该版本，请检查安装包后重试。'));
    }
    finally { setPending(false); }
  };

  return (
    <Modal isOpen onOpenChange={(open) => { if (!open && !pending) onClose(); }}>
      <Modal.Backdrop isDismissable={!pending} isKeyboardDismissDisabled={pending}>
        <Modal.Container placement="center" size="md"><Modal.Dialog><Modal.CloseTrigger aria-label={tr('Close upload dialog', '关闭上传窗口')} isDisabled={pending} /><Modal.Header><Modal.Heading>{tr('Upload client release', '上传客户端版本')}</Modal.Heading></Modal.Header><Modal.Body>
          <form className="space-y-4" id="client-release-upload" onSubmit={(event) => { void submit(event); }}>
            <div className="space-y-1">
              <label className="text-sm font-medium" htmlFor="client-release-file">{tr('Installer package', '安装包')}</label>
              <input accept={Object.values(CLIENT_PACKAGE_EXTENSIONS).flat().map((extension) => `.${extension}`).join(',')} className="block w-full text-sm" disabled={pending} id="client-release-file" onChange={(event) => { choose(event.currentTarget.files?.[0]); }} type="file" />
              {file !== null && <p className="text-xs text-muted">{formatBytes(file.size)}</p>}
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <Select fullWidth isDisabled={pending} onChange={(key) => { if (isPlatform(key)) setPlatform(key); }} value={platform}>
                <Label>{tr('Platform', '平台')}</Label>
                <Select.Trigger><Select.Value /><Select.Indicator /></Select.Trigger>
                <Select.Popover><ListBox>{CLIENT_PLATFORMS.map((item) => <ListBox.Item id={item} key={item} textValue={PLATFORM_LABELS[item]}>{PLATFORM_LABELS[item]}<ListBox.ItemIndicator /></ListBox.Item>)}</ListBox></Select.Popover>
              </Select>
              <Select fullWidth isDisabled={pending} onChange={(key) => { if (isArch(key)) setArch(key); }} value={arch}>
                <Label>{tr('Architecture', '架构')}</Label>
                <Select.Trigger><Select.Value /><Select.Indicator /></Select.Trigger>
                <Select.Popover><ListBox>{CLIENT_ARCHES.map((item) => <ListBox.Item id={item} key={item} textValue={item}>{item}<ListBox.ItemIndicator /></ListBox.Item>)}</ListBox></Select.Popover>
              </Select>
            </div>
            {file !== null && !extensionValid && <p className="text-sm text-danger" role="alert">{tr('This file type is not an installer for the selected platform', '该文件类型不是所选平台的安装包')} ({CLIENT_PACKAGE_EXTENSIONS[platform].map((extension) => `.${extension}`).join(', ')})</p>}
            <TextField fullWidth isDisabled={pending} isInvalid={version.trim().length > 0 && !versionValid} isRequired><Label>{tr('Version', '版本号')}</Label><Input maxLength={64} placeholder="1.2.0" value={version} onChange={(event) => { setVersion(event.currentTarget.value); }} /></TextField>
            <TextField fullWidth isDisabled={pending}><Label>{tr('Release notes', '更新说明')}</Label><TextArea className="min-h-28" maxLength={8000} value={notes} onChange={(event) => { setNotes(event.currentTarget.value); }} /></TextField>
            <Switch isDisabled={pending} isSelected={markLatest} onChange={setMarkLatest}>
              <Switch.Content><Switch.Control><Switch.Thumb /></Switch.Control><span><span className="block text-sm font-medium">{tr('Set as latest', '设为最新版')}</span><span className="block text-xs text-muted">{tr('Clients on this platform and architecture will be offered this version.', '该平台和架构的客户端将收到此版本的更新提示。')}</span></span></Switch.Content>
            </Switch>
            {failed !== null && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('Upload failed', '上传失败')}</Alert.Title><Alert.Description>{failed}</Alert.Description></Alert.Content></Alert>}
          </form>
        </Modal.Body><Modal.Footer><Button isDisabled={pending} onPress={onClose} variant="tertiary">{tr('Cancel', '取消')}</Button><Button form="client-release-upload" isDisabled={!extensionValid || !versionValid} isPending={pending} type="submit">{tr('Upload', '上传')}</Button></Modal.Footer></Modal.Dialog></Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function NotesDialog({ release, onClose, onSaved }: { release: ClientRelease; onClose: () => void; onSaved: () => Promise<void> }) {
  const tr = useTranslate();
  const [notes, setNotes] = useState(release.notes);
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState(false);

  const submit = async (event: SyntheticEvent<HTMLFormElement, SubmitEvent>) => {
    event.preventDefault(); if (pending) return;
    setPending(true); setFailed(false);
    try { await updateClientReleaseNotes(release.id, notes); await onSaved(); }
    catch { setFailed(true); }
    finally { setPending(false); }
  };

  return (
    <Modal isOpen onOpenChange={(open) => { if (!open && !pending) onClose(); }}>
      <Modal.Backdrop isDismissable={!pending} isKeyboardDismissDisabled={pending}>
        <Modal.Container placement="center" size="md"><Modal.Dialog><Modal.CloseTrigger aria-label={tr('Close notes editor', '关闭更新说明编辑器')} isDisabled={pending} /><Modal.Header><Modal.Heading>{tr('Release notes', '更新说明')} · {release.version}</Modal.Heading></Modal.Header><Modal.Body>
          <form className="space-y-4" id="client-release-notes" onSubmit={(event) => { void submit(event); }}>
            <TextField fullWidth><Label>{tr('Release notes', '更新说明')}</Label><TextArea className="min-h-40" maxLength={8000} value={notes} onChange={(event) => { setNotes(event.currentTarget.value); }} /></TextField>
            {failed && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('The notes could not be saved', '无法保存更新说明')}</Alert.Title></Alert.Content></Alert>}
          </form>
        </Modal.Body><Modal.Footer><Button isDisabled={pending} onPress={onClose} variant="tertiary">{tr('Cancel', '取消')}</Button><Button form="client-release-notes" isPending={pending} type="submit">{tr('Save changes', '保存修改')}</Button></Modal.Footer></Modal.Dialog></Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function ReleaseSkeleton() { const tr = useTranslate(); return <div aria-label={tr('Loading client releases', '正在加载客户端版本')} className="space-y-3" role="status"><Skeleton className="h-12 rounded-md" /><Skeleton className="h-72 rounded-md" /></div>; }
function EmptyReleases({ onUpload }: { onUpload: () => void }) { const tr = useTranslate(); return <div className="flex min-h-72 flex-col items-center justify-center border border-dashed border-border px-6 text-center"><MonitorDown className="mb-4 size-8 text-muted" /><h2 className="font-semibold">{tr('No client releases yet', '暂无客户端版本')}</h2><p className="mt-1 text-sm text-muted">{tr('Upload an installer so clients can update from this server.', '上传安装包后，客户端即可从此服务器更新。')}</p><Button className="mt-4" onPress={onUpload} variant="secondary"><Plus className="size-4" />{tr('Upload release', '上传版本')}</Button></div>; }
function formatDate(value: string) { return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value)); }
