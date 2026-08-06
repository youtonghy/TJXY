import { apiRequest } from '../api/httpClient';
import { clientRequest } from '../client/api/clientApi';
import {
  acknowledgeAnnouncement,
  createAnnouncement,
  getAdminAnnouncements,
  getAnnouncements,
  getNextPopupAnnouncement,
  publishAnnouncement,
} from './announcementApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});
vi.mock('../client/api/clientApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('../client/api/clientApi')>();
  return { ...original, clientRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);
const clientRequestMock = vi.mocked(clientRequest);
const id = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const publishedAt = '2026-08-03T03:00:00Z';
const adminRow = {
  Id: id,
  Title: 'Library maintenance',
  BodyMarkdown: '**Playback** will pause briefly.',
  Kind: 'Popup',
  Status: 'Published',
  ContentVersion: 2,
  Revision: 4,
  PublishedAt: publishedAt,
  CreatedAt: '2026-08-02T03:00:00Z',
  UpdatedAt: publishedAt,
};
const clientRow = {
  Id: id,
  Title: 'Library maintenance',
  BodyMarkdown: '**Playback** will pause briefly.',
  Kind: 'Popup',
  ContentVersion: 2,
  PublishedAt: publishedAt,
  IsRead: false,
};

beforeEach(() => { requestMock.mockReset(); clientRequestMock.mockReset(); });

it('strictly parses administrator and client announcement pages', async () => {
  requestMock.mockResolvedValueOnce({ Items: [adminRow], Total: 1 });
  clientRequestMock.mockResolvedValueOnce({ Items: [clientRow], Total: 1, UnreadCount: 1 });

  await expect(getAdminAnnouncements({ startIndex: 0, limit: 20 })).resolves.toMatchObject({
    items: [{ id, status: 'Published', contentVersion: 2 }],
    total: 1,
  });
  await expect(getAnnouncements({ startIndex: 0, limit: 20 })).resolves.toMatchObject({
    items: [{ id, isRead: false }],
    total: 1,
    unreadCount: 1,
  });
});

it('rejects response key drift and invalid versions', async () => {
  requestMock.mockResolvedValueOnce({ Items: [{ ...adminRow, Secret: 'leaked' }], Total: 1 });
  await expect(getAdminAnnouncements({ startIndex: 0, limit: 20 })).rejects.toMatchObject({ category: 'invalid-response' });
  clientRequestMock.mockResolvedValueOnce({ Items: [{ ...clientRow, ContentVersion: 0 }], Total: 1, UnreadCount: 1 });
  await expect(getAnnouncements({ startIndex: 0, limit: 20 })).rejects.toMatchObject({ category: 'invalid-response' });
});

it('serializes lifecycle mutations and exact acknowledgement versions', async () => {
  requestMock
    .mockResolvedValueOnce({ ...adminRow, Status: 'Draft', ContentVersion: 0, Revision: 1, PublishedAt: null })
    .mockResolvedValueOnce(adminRow);
  clientRequestMock.mockResolvedValueOnce(undefined);

  await createAnnouncement({ title: 'Library maintenance', bodyMarkdown: 'Details', kind: 'Popup' });
  await publishAnnouncement(id, 1);
  await acknowledgeAnnouncement(id, 2);

  expect(requestMock).toHaveBeenNthCalledWith(1, '/Admin/Announcements', {
    method: 'POST',
    body: JSON.stringify({ Title: 'Library maintenance', BodyMarkdown: 'Details', Kind: 'Popup' }),
  });
  expect(requestMock).toHaveBeenNthCalledWith(2, `/Admin/Announcements/${id}/Publish`, {
    method: 'POST', body: JSON.stringify({ Revision: 1 }),
  });
  expect(clientRequestMock).toHaveBeenCalledWith(`/Announcements/${id}/Acknowledge`, {
    method: 'POST', body: JSON.stringify({ ContentVersion: 2 }),
  });
});

it('maps an empty next-popup response to null', async () => {
  clientRequestMock.mockResolvedValueOnce(undefined).mockResolvedValueOnce(clientRow);
  await expect(getNextPopupAnnouncement()).resolves.toBeNull();
  await expect(getNextPopupAnnouncement()).resolves.toMatchObject({ id, kind: 'Popup' });
});
