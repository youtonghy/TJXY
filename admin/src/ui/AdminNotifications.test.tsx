import { Button, Toast } from '@heroui/react';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useNotify, type I18nProvider } from 'ra-core';
import type { MockInstance } from 'vitest';

import { renderWithAdmin } from '../test/renderWithAdmin';
import { AdminNotifications } from './AdminNotifications';

const i18nProvider: I18nProvider = {
  translate: (key: string, options?: unknown) => {
    const name = isRecord(options) && typeof options.name === 'string' ? options.name : '';
    return `${key}:${name}`;
  },
  changeLocale: () => Promise.resolve(undefined),
  getLocale: () => 'en',
};
const safeNodeMessage = <span>Safe node message</span>;

let successToast: MockInstance;
let warningToast: MockInstance;
let infoToast: MockInstance;
let dangerToast: MockInstance;

function QueueNotifications() {
  const notify = useNotify();
  return (
    <>
      <Button onPress={() => {
        notify('admin.saved', { type: 'success', messageArgs: { name: 'Ada' } });
        notify('admin.warning', { type: 'warning', autoHideDuration: null });
      }}>
        Queue two
      </Button>
      <Button onPress={() => {
        notify('admin.undo', { type: 'info', undoable: true });
      }}>
        Queue undoable
      </Button>
      <Button onPress={() => {
        notify(safeNodeMessage, { type: 'info', autoHideDuration: 1250 });
        notify('admin.failed', { type: 'error' });
      }}>
        Queue remaining variants
      </Button>
    </>
  );
}

describe('AdminNotifications', () => {
  beforeEach(() => {
    successToast = vi.spyOn(Toast.toast, 'success').mockReturnValue('success-toast');
    warningToast = vi.spyOn(Toast.toast, 'warning').mockReturnValue('warning-toast');
    infoToast = vi.spyOn(Toast.toast, 'info').mockReturnValue('info-toast');
    dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('danger-toast');
  });

  it('drains and translates queued notifications exactly once in Strict Mode', async () => {
    const user = userEvent.setup();
    renderWithAdmin(
      <><QueueNotifications /><AdminNotifications /></>,
      { i18nProvider, strict: true },
    );

    await user.click(screen.getByRole('button', { name: 'Queue two' }));
    await waitFor(() => {
      expect(successToast).toHaveBeenCalledWith('admin.saved:Ada', { timeout: 4000 });
      expect(warningToast).toHaveBeenCalledWith('admin.warning:', { timeout: 0 });
    });
    expect(successToast).toHaveBeenCalledOnce();
    expect(warningToast).toHaveBeenCalledOnce();
  });

  it('fails visibly if an unsupported undoable notification reaches the bridge', async () => {
    const user = userEvent.setup();
    renderWithAdmin(<><QueueNotifications /><AdminNotifications /></>, { i18nProvider });

    await user.click(screen.getByRole('button', { name: 'Queue undoable' }));
    await waitFor(() => {
      expect(dangerToast).toHaveBeenCalledWith(
        'Undoable actions are not supported in TJXY Admin.',
        { timeout: 0 },
      );
    });
    expect(infoToast).not.toHaveBeenCalled();
  });

  it('preserves safe React nodes and maps info and error with exact timeouts', async () => {
    const user = userEvent.setup();
    renderWithAdmin(<><QueueNotifications /><AdminNotifications /></>, { i18nProvider });

    await user.click(screen.getByRole('button', { name: 'Queue remaining variants' }));
    await waitFor(() => {
      expect(infoToast).toHaveBeenCalledWith(safeNodeMessage, { timeout: 1250 });
      expect(dangerToast).toHaveBeenCalledWith('admin.failed:', { timeout: 4000 });
    });
    expect(infoToast).toHaveBeenCalledOnce();
    expect(dangerToast).toHaveBeenCalledOnce();
  });
});

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
