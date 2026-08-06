import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { SystemLocaleProvider } from '../settings/SystemLocaleProvider';
import {
  acknowledgeAnnouncement,
  getAnnouncements,
  getNextPopupAnnouncement,
} from './announcementApi';
import { ClientAnnouncements } from './ClientAnnouncements';
import type { ClientAnnouncement } from './announcementTypes';

vi.mock('./announcementApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('./announcementApi')>();
  return {
    ...original,
    acknowledgeAnnouncement: vi.fn(),
    getAnnouncements: vi.fn(),
    getNextPopupAnnouncement: vi.fn(),
  };
});

const popup: ClientAnnouncement = {
  id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f21',
  title: 'Service notice',
  bodyMarkdown: '**Playback** returns at 20:00.',
  kind: 'Popup',
  contentVersion: 2,
  publishedAt: '2026-08-03T01:00:00Z',
  isRead: false,
};
const standard: ClientAnnouncement = {
  ...popup,
  id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f22',
  title: 'New library',
  kind: 'Standard',
  contentVersion: 1,
};

beforeEach(() => {
  localStorage.setItem('tjxy-system-locale', 'en-US');
  vi.mocked(getAnnouncements).mockReset().mockResolvedValue({
    items: [popup, standard],
    total: 2,
    unreadCount: 2,
  });
  vi.mocked(getNextPopupAnnouncement).mockReset()
    .mockResolvedValueOnce(popup)
    .mockResolvedValue(null);
  vi.mocked(acknowledgeAnnouncement).mockReset().mockResolvedValue(undefined);
});

it('requires acknowledgement for popup announcements and exposes standard announcements from the bell', async () => {
  const user = userEvent.setup();
  render(
    <MemoryRouter>
      <SystemLocaleProvider><ClientAnnouncements /></SystemLocaleProvider>
    </MemoryRouter>,
  );

  const popupDialog = await screen.findByRole('dialog', { name: 'Service notice' });
  expect(popupDialog).toHaveTextContent('Playback returns at 20:00.');
  expect(screen.queryByRole('button', { name: 'Close announcement' })).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'I understand' }));
  await waitFor(() => {
    expect(acknowledgeAnnouncement).toHaveBeenCalledWith(popup.id, popup.contentVersion);
  });
  await waitFor(() => { expect(screen.queryByRole('dialog', { name: 'Service notice' })).not.toBeInTheDocument(); });

  const bell = screen.getByRole('button', { name: 'Announcements, 1 unread' });
  await user.click(bell);
  expect(await screen.findByRole('dialog', { name: 'Announcements' })).toBeVisible();
  expect(screen.getByText('New library')).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Mark New library as read' }));
  await waitFor(() => {
    expect(acknowledgeAnnouncement).toHaveBeenCalledWith(standard.id, standard.contentVersion);
  });
});
