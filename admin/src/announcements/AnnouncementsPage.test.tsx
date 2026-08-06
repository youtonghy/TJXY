import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithAdmin } from '../test/renderWithAdmin';
import {
  archiveAnnouncement,
  createAnnouncement,
  deleteAnnouncement,
  getAdminAnnouncements,
  publishAnnouncement,
  updateAnnouncement,
} from './announcementApi';
import { AnnouncementsPage } from './AnnouncementsPage';
import type { AdminAnnouncement } from './announcementTypes';

vi.mock('./announcementApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('./announcementApi')>();
  return {
    ...original,
    archiveAnnouncement: vi.fn(),
    createAnnouncement: vi.fn(),
    deleteAnnouncement: vi.fn(),
    getAdminAnnouncements: vi.fn(),
    publishAnnouncement: vi.fn(),
    updateAnnouncement: vi.fn(),
  };
});

const draft: AdminAnnouncement = {
  id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11',
  title: 'Library maintenance',
  bodyMarkdown: '**Playback** will pause briefly.',
  kind: 'Popup',
  status: 'Draft',
  contentVersion: 0,
  revision: 1,
  publishedAt: null,
  createdAt: '2026-08-03T01:00:00Z',
  updatedAt: '2026-08-03T01:00:00Z',
};

const getMock = vi.mocked(getAdminAnnouncements);
const createMock = vi.mocked(createAnnouncement);
const publishMock = vi.mocked(publishAnnouncement);

beforeEach(() => {
  getMock.mockReset().mockResolvedValue({ items: [draft], total: 1 });
  createMock.mockReset().mockResolvedValue({ ...draft, id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12' });
  publishMock.mockReset().mockResolvedValue({ ...draft, status: 'Published', contentVersion: 1, revision: 2, publishedAt: '2026-08-03T02:00:00Z' });
  vi.mocked(updateAnnouncement).mockReset();
  vi.mocked(archiveAnnouncement).mockReset();
  vi.mocked(deleteAnnouncement).mockReset();
});

it('creates a Markdown draft and publishes an existing announcement', async () => {
  renderWithAdmin(<AnnouncementsPage />, { initialEntries: ['/admin/announcements'] });
  const user = userEvent.setup();

  const table = await screen.findByRole('grid', { name: 'Announcements' });
  expect(within(table).getByText('Library maintenance')).toBeVisible();
  expect(screen.getByText('Draft')).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'New announcement' }));
  await user.type(screen.getByLabelText('Title'), 'New release');
  await user.type(screen.getByLabelText('Markdown body'), '## Highlights');
  await user.click(screen.getByRole('radio', { name: 'Popup announcement' }));
  expect(screen.getByRole('heading', { name: 'Highlights' })).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Save draft' }));

  await waitFor(() => {
    expect(createMock).toHaveBeenCalledWith({ title: 'New release', bodyMarkdown: '## Highlights', kind: 'Popup' });
  });
  await user.click(screen.getByRole('button', { name: 'Publish Library maintenance' }));
  await waitFor(() => { expect(publishMock).toHaveBeenCalledWith(draft.id, 1); });
});
