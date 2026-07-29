import { Toast } from '@heroui/react';
import { useEffect, useRef } from 'react';
import {
  useNotificationContext,
  useTranslate,
  type NotificationPayload,
} from 'ra-core';

export function AdminNotifications() {
  const { notifications, takeNotification } = useNotificationContext();
  const translate = useTranslate();
  const handledQueueRef = useRef<NotificationPayload[] | null>(null);

  useEffect(() => {
    if (notifications.length === 0 || handledQueueRef.current === notifications) return;
    handledQueueRef.current = notifications;
    const notification = takeNotification();
    if (notification === undefined) return;

    const options = notification.notificationOptions;
    if (options?.undoable === true) {
      Toast.toast.danger('Undoable actions are not supported in TJXY Admin.', { timeout: 0 });
      return;
    }

    const message = typeof notification.message === 'string'
      ? translate(notification.message, options?.messageArgs)
      : notification.message;
    const timeout = options?.autoHideDuration === null
      ? 0
      : options?.autoHideDuration ?? 4000;

    showToast(notification.type, message, timeout);
  }, [notifications, takeNotification, translate]);

  return null;
}

function showToast(
  type: NotificationPayload['type'],
  message: NotificationPayload['message'],
  timeout: number,
) {
  switch (type) {
    case 'success':
      Toast.toast.success(message, { timeout });
      return;
    case 'warning':
      Toast.toast.warning(message, { timeout });
      return;
    case 'error':
      Toast.toast.danger(message, { timeout });
      return;
    case 'info':
      Toast.toast.info(message, { timeout });
  }
}
