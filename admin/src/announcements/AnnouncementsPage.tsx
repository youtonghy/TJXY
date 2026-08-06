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
        actions={<><Button aria-label="Reload announcements" isIconOnly isPending={loading} onPress={() => { void load(); }} variant="ghost"><RefreshCw className="size-4" /></Button><Button onPress={() => { setEditor('new'); }}><Plus className="size-4" />New announcement</Button></>}
        description="Create, publish, archive, and permanently remove client announcements."
        title="Announcements"
      />
      {error && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>Announcements could not be loaded</Alert.Title><Alert.Description>Review the connection and try again.</Alert.Description></Alert.Content><Button onPress={() => { void load(); }} size="sm" variant="tertiary">Retry</Button></Alert>}
      {loading && items.length === 0 ? <AnnouncementSkeleton /> : items.length === 0 ? <EmptyAnnouncements onCreate={() => { setEditor('new'); }} /> : (
        <>
          <Table variant="secondary">
            <Table.ScrollContainer>
              <Table.Content aria-label="Announcements" className="min-w-[58rem] table-fixed">
                <Table.Header><Table.Column isRowHeader>Announcement</Table.Column><Table.Column>Type</Table.Column><Table.Column>Status</Table.Column><Table.Column>Version</Table.Column><Table.Column>Updated</Table.Column><Table.Column>Actions</Table.Column></Table.Header>
                <Table.Body>{items.map((item) => (
                  <Table.Row id={item.id} key={item.id}>
                    <Table.Cell><div className="min-w-0"><span className="block truncate font-medium">{item.title}</span><span className="block truncate text-xs text-muted">{summary(item.bodyMarkdown)}</span></div></Table.Cell>
                    <Table.Cell><Chip size="sm" variant="soft">{item.kind === 'Popup' ? 'Popup' : 'Standard'}</Chip></Table.Cell>
                    <Table.Cell><StatusChip status={item.status} /></Table.Cell>
                    <Table.Cell><span className="tabular-nums">v{item.contentVersion}</span></Table.Cell>
                    <Table.Cell>{formatDate(item.updatedAt)}</Table.Cell>
                    <Table.Cell><div className="flex items-center gap-1">
                      <Button aria-label={`Edit ${item.title}`} isIconOnly onPress={() => { setEditor(item); }} size="sm" variant="ghost"><Edit3 className="size-4" /></Button>
                      {item.status === 'Published'
                        ? <Button aria-label={`Archive ${item.title}`} isIconOnly isPending={pendingId === item.id} onPress={() => { void lifecycle(item, 'archive'); }} size="sm" variant="ghost"><Archive className="size-4" /></Button>
                        : <Button aria-label={`Publish ${item.title}`} isIconOnly isPending={pendingId === item.id} onPress={() => { void lifecycle(item, 'publish'); }} size="sm" variant="ghost"><Send className="size-4" /></Button>}
                      <ConfirmDialog
                        confirmLabel="Delete permanently"
                        description={<>Delete <strong>{item.title}</strong> and all user read receipts. This cannot be undone.</>}
                        errorDescription="The announcement was not deleted. Reload it and try again."
                        isPending={pendingId === item.id}
                        onConfirm={async () => { setPendingId(item.id); try { await deleteAnnouncement(item.id, item.revision); await load(); } finally { setPendingId(null); } }}
                        title="Delete announcement?"
                        trigger={<Button aria-label={`Delete ${item.title}`} isIconOnly size="sm" variant="ghost"><Trash2 className="size-4 text-danger" /></Button>}
                      />
                    </div></Table.Cell>
                  </Table.Row>
                ))}</Table.Body>
              </Table.Content>
            </Table.ScrollContainer>
          </Table>
          <div className="flex items-center justify-between gap-3 text-sm text-muted"><span>{startIndex + 1}-{Math.min(startIndex + items.length, total)} of {total}</span><div className="flex gap-2"><Button isDisabled={startIndex === 0} onPress={() => { setStartIndex(Math.max(0, startIndex - PAGE_SIZE)); }} size="sm" variant="secondary">Previous</Button><Button isDisabled={startIndex + PAGE_SIZE >= total} onPress={() => { setStartIndex(startIndex + PAGE_SIZE); }} size="sm" variant="secondary">Next</Button></div></div>
        </>
      )}
      <AnnouncementEditor announcement={editor} key={editorKey(editor)} onClose={() => { setEditor(null); }} onSaved={async () => { setEditor(null); await load(); }} />
    </div>
  );
}

function AnnouncementEditor({ announcement, onClose, onSaved }: { announcement: AdminAnnouncement | 'new' | null; onClose: () => void; onSaved: () => Promise<void> }) {
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
        <Modal.Container placement="center" size="lg"><Modal.Dialog><Modal.CloseTrigger aria-label="Close announcement editor" isDisabled={pending} /><Modal.Header><Modal.Heading>{existing === null ? 'New announcement' : 'Edit announcement'}</Modal.Heading></Modal.Header><Modal.Body>
          <form className="grid gap-5 lg:grid-cols-2" id="announcement-editor" onSubmit={(event) => { void submit(event); }}>
            <div className="space-y-4">
              <TextField fullWidth isRequired><Label>Title</Label><Input maxLength={200} value={title} onChange={(event) => { setTitle(event.currentTarget.value); }} /></TextField>
              <TextField fullWidth isRequired><Label>Markdown body</Label><TextArea className="min-h-56" maxLength={32000} value={bodyMarkdown} onChange={(event) => { setBodyMarkdown(event.currentTarget.value); }} /></TextField>
              <RadioGroup name="announcement-kind" onChange={(value) => { setKind(value as AnnouncementKind); }} value={kind}><Label>Delivery</Label><Radio aria-label="Standard announcement" value="Standard"><Radio.Control><Radio.Indicator /></Radio.Control><Radio.Content><span className="font-medium">Standard announcement</span><span className="text-sm text-muted">Available from the bell without opening automatically.</span></Radio.Content></Radio><Radio aria-label="Popup announcement" value="Popup"><Radio.Control><Radio.Indicator /></Radio.Control><Radio.Content><span className="font-medium">Popup announcement</span><span className="text-sm text-muted">Shown once per version until each user confirms it.</span></Radio.Content></Radio></RadioGroup>
              {existing?.status === 'Published' && <Alert status="warning"><Alert.Content><Alert.Title>This update will be republished</Alert.Title><Alert.Description>Changing visible content creates a new version and makes it unread for every user.</Alert.Description></Alert.Content></Alert>}
              {failed && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>The announcement could not be saved</Alert.Title><Alert.Description>Reload the current version and try again.</Alert.Description></Alert.Content></Alert>}
            </div>
            <section aria-label="Markdown preview" className="min-h-72 border-l-0 border-border lg:border-l lg:pl-5"><p className="mb-3 text-sm font-medium">Preview</p>{bodyMarkdown.trim().length === 0 ? <p className="text-sm text-muted">The preview appears as you type.</p> : <AnnouncementMarkdown>{bodyMarkdown}</AnnouncementMarkdown>}</section>
          </form>
        </Modal.Body><Modal.Footer><Button isDisabled={pending} onPress={onClose} variant="tertiary">Cancel</Button><Button form="announcement-editor" isDisabled={title.trim().length === 0 || bodyMarkdown.trim().length === 0} isPending={pending} type="submit">{existing === null ? 'Save draft' : existing.status === 'Published' ? 'Update and republish' : 'Save changes'}</Button></Modal.Footer></Modal.Dialog></Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function StatusChip({ status }: { status: AdminAnnouncement['status'] }) { return <Chip color={status === 'Published' ? 'success' : status === 'Archived' ? 'default' : 'warning'} size="sm" variant="soft">{status}</Chip>; }
function AnnouncementSkeleton() { return <div aria-label="Loading announcements" className="space-y-3" role="status"><Skeleton className="h-12 rounded-md" /><Skeleton className="h-72 rounded-md" /></div>; }
function EmptyAnnouncements({ onCreate }: { onCreate: () => void }) { return <div className="flex min-h-72 flex-col items-center justify-center border border-dashed border-border px-6 text-center"><Megaphone className="mb-4 size-8 text-muted" /><h2 className="font-semibold">No announcements yet</h2><p className="mt-1 text-sm text-muted">Create a draft before publishing it to users.</p><Button className="mt-4" onPress={onCreate} variant="secondary"><Plus className="size-4" />New announcement</Button></div>; }
function editorKey(value: AdminAnnouncement | 'new' | null) { return value === null ? 'closed' : value === 'new' ? 'new' : `${value.id}:${String(value.revision)}`; }
function summary(value: string) { return value.replace(/[#*_>`()[\]]/gu, ' ').replace(/\s+/gu, ' ').trim(); }
function formatDate(value: string) { return new Intl.DateTimeFormat('en', { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value)); }
