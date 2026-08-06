/* eslint-disable react-hooks/set-state-in-effect */
import { Alert, Badge, Button, Chip, Modal, Skeleton, Tooltip } from '@heroui/react';
import { Bell, Check, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslate } from '../settings/i18n';
import { AnnouncementMarkdown } from './AnnouncementMarkdown';
import {
  acknowledgeAnnouncement,
  getAnnouncements,
  getNextPopupAnnouncement,
} from './announcementApi';
import type { ClientAnnouncement } from './announcementTypes';

const PAGE_SIZE = 50;
const REFRESH_INTERVAL_MS = 60_000;

export function ClientAnnouncements() {
  const tr = useTranslate();
  const [items, setItems] = useState<ClientAnnouncement[]>([]);
  const [unreadCount, setUnreadCount] = useState(0);
  const [popup, setPopup] = useState<ClientAnnouncement | null>(null);
  const [centerOpen, setCenterOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [pendingId, setPendingId] = useState<string | null>(null);

  const load = useCallback(async (signal?: AbortSignal) => {
    try {
      const [page, nextPopup] = await Promise.all([
        getAnnouncements({ startIndex: 0, limit: PAGE_SIZE }, signal),
        getNextPopupAnnouncement(signal),
      ]);
      if (signal?.aborted === true) return;
      setItems(page.items);
      setUnreadCount(page.unreadCount);
      setPopup(nextPopup);
      setFailed(false);
    } catch {
      if (signal?.aborted !== true) setFailed(true);
    } finally {
      if (signal?.aborted !== true) setLoading(false);
    }
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    const refresh = () => { if (document.visibilityState === 'visible') void load(); };
    const interval = window.setInterval(refresh, REFRESH_INTERVAL_MS);
    window.addEventListener('focus', refresh);
    document.addEventListener('visibilitychange', refresh);
    return () => {
      controller.abort();
      window.clearInterval(interval);
      window.removeEventListener('focus', refresh);
      document.removeEventListener('visibilitychange', refresh);
    };
  }, [load]);

  const acknowledge = async (announcement: ClientAnnouncement, continuePopups: boolean) => {
    setPendingId(announcement.id);
    try {
      await acknowledgeAnnouncement(announcement.id, announcement.contentVersion);
      const wasUnread = items.some((item) => item.id === announcement.id && !item.isRead);
      setItems((current) => current.map((item) => item.id === announcement.id ? { ...item, isRead: true } : item));
      if (wasUnread) setUnreadCount((current) => Math.max(0, current - 1));
      if (continuePopups) setPopup(await getNextPopupAnnouncement());
      setFailed(false);
    } catch {
      setFailed(true);
    } finally {
      setPendingId(null);
    }
  };

  const label = unreadCount > 0
    ? tr(`Announcements, ${String(unreadCount)} unread`, `公告，${String(unreadCount)} 条未读`)
    : tr('Announcements', '公告');

  return (
    <>
      <Badge.Anchor>
        <Tooltip>
          <Button
            aria-label={label}
            isDisabled={popup !== null}
            isIconOnly
            onPress={() => { setCenterOpen(true); }}
            size="sm"
            variant="ghost"
          >
            <Bell aria-hidden="true" className="size-4" />
          </Button>
          <Tooltip.Content>{tr('View announcements', '查看公告')}</Tooltip.Content>
        </Tooltip>
        {unreadCount > 0 ? <Badge aria-hidden="true" className="size-2 min-h-2 min-w-2 p-0" color="danger" placement="top-right" size="sm" /> : null}
      </Badge.Anchor>

      <AnnouncementCenter
        failed={failed}
        isOpen={centerOpen}
        items={items}
        loading={loading}
        onAcknowledge={(announcement) => { void acknowledge(announcement, false); }}
        onClose={() => { setCenterOpen(false); }}
        onRetry={() => { void load(); }}
        pendingId={pendingId}
      />
      <PopupAnnouncement
        announcement={popup}
        failed={failed}
        isPending={pendingId === popup?.id}
        onAcknowledge={(announcement) => { void acknowledge(announcement, true); }}
      />
    </>
  );
}

function PopupAnnouncement({ announcement, failed, isPending, onAcknowledge }: {
  announcement: ClientAnnouncement | null;
  failed: boolean;
  isPending: boolean;
  onAcknowledge: (announcement: ClientAnnouncement) => void;
}) {
  const tr = useTranslate();
  return (
    <Modal isOpen={announcement !== null}>
      <Modal.Backdrop isDismissable={false} isKeyboardDismissDisabled>
        <Modal.Container placement="center" size="md"><Modal.Dialog>
          <Modal.Header><Modal.Heading>{announcement?.title}</Modal.Heading></Modal.Header>
          <Modal.Body>
            {announcement === null ? null : <AnnouncementMarkdown>{announcement.bodyMarkdown}</AnnouncementMarkdown>}
            {failed ? <Alert status="danger"><Alert.Content><Alert.Title>{tr('Confirmation failed', '确认失败')}</Alert.Title><Alert.Description>{tr('Please try again before continuing.', '请重试，确认成功后才能继续。')}</Alert.Description></Alert.Content></Alert> : null}
          </Modal.Body>
          <Modal.Footer><Button isPending={isPending} onPress={() => { if (announcement !== null) onAcknowledge(announcement); }}><Check className="size-4" />{tr('I understand', '我已了解')}</Button></Modal.Footer>
        </Modal.Dialog></Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function AnnouncementCenter({ failed, isOpen, items, loading, onAcknowledge, onClose, onRetry, pendingId }: {
  failed: boolean;
  isOpen: boolean;
  items: ClientAnnouncement[];
  loading: boolean;
  onAcknowledge: (announcement: ClientAnnouncement) => void;
  onClose: () => void;
  onRetry: () => void;
  pendingId: string | null;
}) {
  const tr = useTranslate();
  return (
    <Modal isOpen={isOpen} onOpenChange={(open) => { if (!open) onClose(); }}>
      <Modal.Backdrop><Modal.Container placement="center" size="lg"><Modal.Dialog>
        <Modal.CloseTrigger aria-label={tr('Close announcement', '关闭公告')} />
        <Modal.Header><Modal.Heading>{tr('Announcements', '公告')}</Modal.Heading></Modal.Header>
        <Modal.Body>
          {loading ? <div aria-label={tr('Loading announcements', '正在加载公告')} className="space-y-3" role="status"><Skeleton className="h-24 rounded-md" /><Skeleton className="h-24 rounded-md" /></div> : null}
          {failed ? <Alert status="danger"><Alert.Content><Alert.Title>{tr('Announcements could not be loaded', '公告加载失败')}</Alert.Title></Alert.Content><Button isIconOnly onPress={onRetry} size="sm" variant="tertiary"><RefreshCw className="size-4" /></Button></Alert> : null}
          {!loading && !failed && items.length === 0 ? <p className="py-10 text-center text-sm text-muted">{tr('No announcements have been published.', '暂无已发布公告。')}</p> : null}
          <div className="divide-y divide-border">{items.map((item) => (
            <article className="py-5 first:pt-0" key={`${item.id}:${String(item.contentVersion)}`}>
              <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
                <div><h3 className="font-semibold">{item.title}</h3><p className="mt-1 text-xs text-muted">{formatDate(item.publishedAt)}</p></div>
                <Chip color={item.isRead ? 'default' : 'danger'} size="sm" variant="soft">{item.isRead ? tr('Read', '已读') : tr('Unread', '未读')}</Chip>
              </div>
              <AnnouncementMarkdown>{item.bodyMarkdown}</AnnouncementMarkdown>
              {!item.isRead ? <div className="mt-4 flex justify-end"><Button aria-label={tr(`Mark ${item.title} as read`, `将 ${item.title} 标记为已读`)} isPending={pendingId === item.id} onPress={() => { onAcknowledge(item); }} size="sm" variant="secondary"><Check className="size-4" />{tr('Mark as read', '标记已读')}</Button></div> : null}
            </article>
          ))}</div>
        </Modal.Body>
      </Modal.Dialog></Modal.Container></Modal.Backdrop>
    </Modal>
  );
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value));
}
