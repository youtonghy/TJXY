/* eslint-disable react-hooks/set-state-in-effect */
import {
  Alert,
  Button,
  Chip,
  Input,
  Label,
  Modal,
  Radio,
  RadioGroup,
  Skeleton,
  Table,
  TextArea,
  TextField,
} from '@heroui/react';
import { Archive, Edit3, Megaphone, Plus, RefreshCw, Send, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState, type SyntheticEvent } from 'react';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { PageHeader } from '../ui/PageHeader';
import { useTranslate } from '../settings/i18n';
import { AnnouncementMarkdown } from './AnnouncementMarkdown';
import {
  archiveAnnouncement,
  createAnnouncement,
  deleteAnnouncement,
  getAdminAnnouncements,
  publishAnnouncement,
  updateAnnouncement,
} from './announcementApi';
import type { AdminAnnouncement, AnnouncementContent, AnnouncementKind } from './announcementTypes';

const PAGE_SIZE = 20;

export function AnnouncementsPage() {
  const tr = useTranslate();
  const [items, setItems] = useState<AdminAnnouncement[]>([]);
  const [total, setTotal] = useState(0);
  const [startIndex, setStartIndex] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [editor, setEditor] = useState<AdminAnnouncement | 'new' | null>(null);
  const [pendingId, setPendingId] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true); setError(false);
    try {
      const page = await getAdminAnnouncements({ startIndex, limit: PAGE_SIZE });
      setItems(page.items); setTotal(page.total);
    } catch { setError(true); }
    finally { setLoading(false); }
  }, [startIndex]);

  useEffect(() => { void load(); }, [load]);

  const lifecycle = async (item: AdminAnnouncement, action: 'publish' | 'archive') => {
    setPendingId(item.id);
    try {
      if (action === 'publish') await publishAnnouncement(item.id, item.revision);
      else await archiveAnnouncement(item.id, item.revision);
      await load();
    } finally { setPendingId(null); }
  };

  return (
    <div className="space-y-5">
      <PageHeader
        actions={<><Button aria-label={tr('Reload announcements', '重新加载公告')} isIconOnly isPending={loading} onPress={() => { void load(); }} variant="ghost"><RefreshCw className="size-4" /></Button><Button onPress={() => { setEditor('new'); }}><Plus className="size-4" />{tr('New announcement', '新建公告')}</Button></>}
        description={tr('Create, publish, archive, and permanently remove client announcements.', '创建、发布、归档和永久删除前台公告。')}
        title={tr('Announcements', '公告管理')}
      />
      {error && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('Announcements could not be loaded', '无法加载公告')}</Alert.Title><Alert.Description>{tr('Review the connection and try again.', '请检查连接后重试。')}</Alert.Description></Alert.Content><Button onPress={() => { void load(); }} size="sm" variant="tertiary">{tr('Retry', '重试')}</Button></Alert>}
      {loading && items.length === 0 ? <AnnouncementSkeleton /> : items.length === 0 ? <EmptyAnnouncements onCreate={() => { setEditor('new'); }} /> : (
        <>
          <Table variant="secondary">
            <Table.ScrollContainer>
              <Table.Content aria-label={tr('Announcements', '公告')} className="min-w-[58rem] table-fixed">
                <Table.Header><Table.Column isRowHeader>{tr('Announcement', '公告')}</Table.Column><Table.Column>{tr('Type', '类型')}</Table.Column><Table.Column>{tr('Status', '状态')}</Table.Column><Table.Column>{tr('Version', '版本')}</Table.Column><Table.Column>{tr('Updated', '更新时间')}</Table.Column><Table.Column>{tr('Actions', '操作')}</Table.Column></Table.Header>
                <Table.Body>{items.map((item) => (
                  <Table.Row id={item.id} key={item.id}>
                    <Table.Cell><div className="min-w-0"><span className="block truncate font-medium">{item.title}</span><span className="block truncate text-xs text-muted">{summary(item.bodyMarkdown)}</span></div></Table.Cell>
                    <Table.Cell><Chip size="sm" variant="soft">{item.kind === 'Popup' ? tr('Popup', '弹窗') : tr('Standard', '普通')}</Chip></Table.Cell>
                    <Table.Cell><StatusChip status={item.status} /></Table.Cell>
                    <Table.Cell><span className="tabular-nums">v{item.contentVersion}</span></Table.Cell>
                    <Table.Cell>{formatDate(item.updatedAt)}</Table.Cell>
                    <Table.Cell><div className="flex items-center gap-1">
                      <Button aria-label={`${tr('Edit', '编辑')} ${item.title}`} isIconOnly onPress={() => { setEditor(item); }} size="sm" variant="ghost"><Edit3 className="size-4" /></Button>
                      {item.status === 'Published'
                        ? <Button aria-label={`${tr('Archive', '归档')} ${item.title}`} isIconOnly isPending={pendingId === item.id} onPress={() => { void lifecycle(item, 'archive'); }} size="sm" variant="ghost"><Archive className="size-4" /></Button>
                        : <Button aria-label={`${tr('Publish', '发布')} ${item.title}`} isIconOnly isPending={pendingId === item.id} onPress={() => { void lifecycle(item, 'publish'); }} size="sm" variant="ghost"><Send className="size-4" /></Button>}
                      <ConfirmDialog
                        confirmLabel={tr('Delete permanently', '永久删除')}
                        description={<>{tr('Delete', '删除')} <strong>{item.title}</strong>{tr(' and all user read receipts. This cannot be undone.', ' 及所有用户的已读记录。此操作无法撤销。')}</>}
                        errorDescription={tr('The announcement was not deleted. Reload it and try again.', '公告未删除，请重新加载后重试。')}
                        isPending={pendingId === item.id}
                        onConfirm={async () => { setPendingId(item.id); try { await deleteAnnouncement(item.id, item.revision); await load(); } finally { setPendingId(null); } }}
                        title={tr('Delete announcement?', '删除公告？')}
                        trigger={<Button aria-label={`${tr('Delete', '删除')} ${item.title}`} isIconOnly size="sm" variant="ghost"><Trash2 className="size-4 text-danger" /></Button>}
                      />
                    </div></Table.Cell>
                  </Table.Row>
                ))}</Table.Body>
              </Table.Content>
            </Table.ScrollContainer>
          </Table>
          <div className="flex items-center justify-between gap-3 text-sm text-muted"><span>{startIndex + 1}-{Math.min(startIndex + items.length, total)} {tr('of', '共')} {total}</span><div className="flex gap-2"><Button isDisabled={startIndex === 0} onPress={() => { setStartIndex(Math.max(0, startIndex - PAGE_SIZE)); }} size="sm" variant="secondary">{tr('Previous', '上一页')}</Button><Button isDisabled={startIndex + PAGE_SIZE >= total} onPress={() => { setStartIndex(startIndex + PAGE_SIZE); }} size="sm" variant="secondary">{tr('Next', '下一页')}</Button></div></div>
        </>
      )}
      <AnnouncementEditor announcement={editor} key={editorKey(editor)} onClose={() => { setEditor(null); }} onSaved={async () => { setEditor(null); await load(); }} />
    </div>
  );
}

function AnnouncementEditor({ announcement, onClose, onSaved }: { announcement: AdminAnnouncement | 'new' | null; onClose: () => void; onSaved: () => Promise<void> }) {
  const tr = useTranslate();
  const existing = announcement === null || announcement === 'new' ? null : announcement;
  const [title, setTitle] = useState(existing?.title ?? '');
  const [bodyMarkdown, setBodyMarkdown] = useState(existing?.bodyMarkdown ?? '');
  const [kind, setKind] = useState<AnnouncementKind>(existing?.kind ?? 'Standard');
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState(false);

  const submit = async (event: SyntheticEvent<HTMLFormElement, SubmitEvent>) => {
    event.preventDefault(); if (pending) return;
    setPending(true); setFailed(false);
    const content: AnnouncementContent = { title, bodyMarkdown, kind };
    try { if (existing === null) await createAnnouncement(content); else await updateAnnouncement(existing, content); await onSaved(); }
    catch { setFailed(true); }
    finally { setPending(false); }
  };

  return (
    <Modal isOpen={announcement !== null} onOpenChange={(open) => { if (!open && !pending) onClose(); }}>
      <Modal.Backdrop isDismissable={!pending} isKeyboardDismissDisabled={pending}>
        <Modal.Container placement="center" size="lg"><Modal.Dialog><Modal.CloseTrigger aria-label={tr('Close announcement editor', '关闭公告编辑器')} isDisabled={pending} /><Modal.Header><Modal.Heading>{existing === null ? tr('New announcement', '新建公告') : tr('Edit announcement', '编辑公告')}</Modal.Heading></Modal.Header><Modal.Body>
          <form className="grid gap-5 lg:grid-cols-2" id="announcement-editor" onSubmit={(event) => { void submit(event); }}>
            <div className="space-y-4">
              <TextField fullWidth isRequired><Label>{tr('Title', '标题')}</Label><Input maxLength={200} value={title} onChange={(event) => { setTitle(event.currentTarget.value); }} /></TextField>
              <TextField fullWidth isRequired><Label>{tr('Markdown body', 'Markdown 正文')}</Label><TextArea className="min-h-56" maxLength={32000} value={bodyMarkdown} onChange={(event) => { setBodyMarkdown(event.currentTarget.value); }} /></TextField>
              <RadioGroup name="announcement-kind" onChange={(value) => { setKind(value as AnnouncementKind); }} value={kind}><Label>{tr('Delivery', '展示方式')}</Label><Radio aria-label={tr('Standard announcement', '普通公告')} value="Standard"><Radio.Control><Radio.Indicator /></Radio.Control><Radio.Content><span className="font-medium">{tr('Standard announcement', '普通公告')}</span><span className="text-sm text-muted">{tr('Available from the bell without opening automatically.', '可从通知铃中查看，不会自动弹出。')}</span></Radio.Content></Radio><Radio aria-label={tr('Popup announcement', '弹窗公告')} value="Popup"><Radio.Control><Radio.Indicator /></Radio.Control><Radio.Content><span className="font-medium">{tr('Popup announcement', '弹窗公告')}</span><span className="text-sm text-muted">{tr('Shown once per version until each user confirms it.', '每个版本会向每位用户弹出一次，直到用户确认。')}</span></Radio.Content></Radio></RadioGroup>
              {existing?.status === 'Published' && <Alert status="warning"><Alert.Content><Alert.Title>{tr('This update will be republished', '本次更新将重新发布')}</Alert.Title><Alert.Description>{tr('Changing visible content creates a new version and makes it unread for every user.', '修改可见内容会创建新版本，并将所有用户的状态重置为未读。')}</Alert.Description></Alert.Content></Alert>}
              {failed && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('The announcement could not be saved', '无法保存公告')}</Alert.Title><Alert.Description>{tr('Reload the current version and try again.', '请重新加载当前版本后重试。')}</Alert.Description></Alert.Content></Alert>}
            </div>
            <section aria-label={tr('Markdown preview', 'Markdown 预览')} className="min-h-72 border-l-0 border-border lg:border-l lg:pl-5"><p className="mb-3 text-sm font-medium">{tr('Preview', '预览')}</p>{bodyMarkdown.trim().length === 0 ? <p className="text-sm text-muted">{tr('The preview appears as you type.', '输入内容后将在此显示预览。')}</p> : <AnnouncementMarkdown>{bodyMarkdown}</AnnouncementMarkdown>}</section>
          </form>
        </Modal.Body><Modal.Footer><Button isDisabled={pending} onPress={onClose} variant="tertiary">{tr('Cancel', '取消')}</Button><Button form="announcement-editor" isDisabled={title.trim().length === 0 || bodyMarkdown.trim().length === 0} isPending={pending} type="submit">{existing === null ? tr('Save draft', '保存草稿') : existing.status === 'Published' ? tr('Update and republish', '更新并重新发布') : tr('Save changes', '保存修改')}</Button></Modal.Footer></Modal.Dialog></Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function StatusChip({ status }: { status: AdminAnnouncement['status'] }) { const tr = useTranslate(); return <Chip color={status === 'Published' ? 'success' : status === 'Archived' ? 'default' : 'warning'} size="sm" variant="soft">{status === 'Published' ? tr('Published', '已发布') : status === 'Archived' ? tr('Archived', '已归档') : tr('Draft', '草稿')}</Chip>; }
function AnnouncementSkeleton() { const tr = useTranslate(); return <div aria-label={tr('Loading announcements', '正在加载公告')} className="space-y-3" role="status"><Skeleton className="h-12 rounded-md" /><Skeleton className="h-72 rounded-md" /></div>; }
function EmptyAnnouncements({ onCreate }: { onCreate: () => void }) { const tr = useTranslate(); return <div className="flex min-h-72 flex-col items-center justify-center border border-dashed border-border px-6 text-center"><Megaphone className="mb-4 size-8 text-muted" /><h2 className="font-semibold">{tr('No announcements yet', '暂无公告')}</h2><p className="mt-1 text-sm text-muted">{tr('Create a draft before publishing it to users.', '先创建草稿，再向用户发布。')}</p><Button className="mt-4" onPress={onCreate} variant="secondary"><Plus className="size-4" />{tr('New announcement', '新建公告')}</Button></div>; }
function editorKey(value: AdminAnnouncement | 'new' | null) { return value === null ? 'closed' : value === 'new' ? 'new' : `${value.id}:${String(value.revision)}`; }
function summary(value: string) { return value.replace(/[#*_>`()[\]]/gu, ' ').replace(/\s+/gu, ' ').trim(); }
function formatDate(value: string) { return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value)); }
