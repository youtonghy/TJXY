import { ApiError, apiRequest } from '../api/httpClient';
import { clientRequest } from '../client/api/clientApi';
import {
  invalidResponse,
  isNonNegativeInteger,
  isRecord,
  validDate,
  validMultilineText,
  validText,
  validUuid,
} from '../api/responseValidation';
import type {
  AdminAnnouncement,
  AdminAnnouncementPage,
  AdminAnnouncementPageRequest,
  AnnouncementContent,
  AnnouncementKind,
  AnnouncementPageRequest,
  AnnouncementStatus,
  ClientAnnouncement,
  ClientAnnouncementPage,
} from './announcementTypes';

const MAX_PAGE_SIZE = 100;

export async function getAdminAnnouncements(request: AdminAnnouncementPageRequest, signal?: AbortSignal): Promise<AdminAnnouncementPage> {
  const value = await apiRequest<unknown>(`/Admin/Announcements?${pageQuery(request)}`, signal === undefined ? {} : { signal });
  if (!isRecord(value) || !exactKeys(value, ['Items', 'Total']) || !Array.isArray(value.Items) || value.Items.length > MAX_PAGE_SIZE || !isNonNegativeInteger(value.Total)) throw invalidResponse('announcement page');
  return { items: value.Items.map(toAdminAnnouncement), total: value.Total };
}

export async function createAnnouncement(content: AnnouncementContent): Promise<AdminAnnouncement> {
  return toAdminAnnouncement(await apiRequest<unknown>('/Admin/Announcements', { method: 'POST', body: JSON.stringify(contentBody(content)) }));
}

export async function updateAnnouncement(announcement: AdminAnnouncement, content: AnnouncementContent): Promise<AdminAnnouncement> {
  requireUuid(announcement.id); requirePositive(announcement.revision, 'revision');
  return toAdminAnnouncement(await apiRequest<unknown>(`/Admin/Announcements/${announcement.id}`, {
    method: 'PUT', body: JSON.stringify({ ...contentBody(content), Revision: announcement.revision }),
  }));
}

export async function publishAnnouncement(id: string, revision: number): Promise<AdminAnnouncement> {
  return lifecycle(id, revision, 'Publish');
}

export async function archiveAnnouncement(id: string, revision: number): Promise<AdminAnnouncement> {
  return lifecycle(id, revision, 'Archive');
}

export async function deleteAnnouncement(id: string, revision: number): Promise<void> {
  requireUuid(id); requirePositive(revision, 'revision');
  await apiRequest(`/Admin/Announcements/${id}`, { method: 'DELETE', body: JSON.stringify({ Revision: revision }) });
}

export async function getAnnouncements(request: AnnouncementPageRequest, signal?: AbortSignal): Promise<ClientAnnouncementPage> {
  const value = await clientRequest<unknown>(`/Announcements?${pageQuery(request)}`, signal === undefined ? {} : { signal });
  if (!isRecord(value) || !exactKeys(value, ['Items', 'Total', 'UnreadCount']) || !Array.isArray(value.Items) || value.Items.length > MAX_PAGE_SIZE || !isNonNegativeInteger(value.Total) || !isNonNegativeInteger(value.UnreadCount) || value.UnreadCount > value.Total) throw invalidResponse('announcement page');
  return { items: value.Items.map(toClientAnnouncement), total: value.Total, unreadCount: value.UnreadCount };
}

export async function getNextPopupAnnouncement(signal?: AbortSignal): Promise<ClientAnnouncement | null> {
  const value = await clientRequest<unknown>('/Announcements/NextPopup', signal === undefined ? {} : { signal });
  return value === undefined ? null : toClientAnnouncement(value);
}

export async function acknowledgeAnnouncement(id: string, contentVersion: number): Promise<void> {
  requireUuid(id); requirePositive(contentVersion, 'content version');
  await clientRequest(`/Announcements/${id}/Acknowledge`, { method: 'POST', body: JSON.stringify({ ContentVersion: contentVersion }) });
}

async function lifecycle(id: string, revision: number, action: 'Publish' | 'Archive'): Promise<AdminAnnouncement> {
  requireUuid(id); requirePositive(revision, 'revision');
  return toAdminAnnouncement(await apiRequest<unknown>(`/Admin/Announcements/${id}/${action}`, {
    method: 'POST', body: JSON.stringify({ Revision: revision }),
  }));
}

function contentBody(content: AnnouncementContent): Record<string, unknown> {
  if (!validText(content.title, 200) || !validMultilineText(content.bodyMarkdown, 32_000) || !isKind(content.kind)) throw new ApiError(400, 'validation', 'Announcement content is invalid.');
  return { Title: content.title.trim(), BodyMarkdown: content.bodyMarkdown, Kind: content.kind };
}

function pageQuery(request: AdminAnnouncementPageRequest | AnnouncementPageRequest): string {
  if (!isNonNegativeInteger(request.startIndex) || !Number.isSafeInteger(request.limit) || request.limit <= 0 || request.limit > MAX_PAGE_SIZE) throw new ApiError(400, 'validation', 'Announcement page is invalid.');
  const query = new URLSearchParams();
  query.set('startIndex', String(request.startIndex)); query.set('limit', String(request.limit));
  if ('status' in request && request.status !== undefined) { if (!isStatus(request.status)) throw new ApiError(400, 'validation', 'Announcement status is invalid.'); query.set('status', request.status); }
  if ('kind' in request && request.kind !== undefined) { if (!isKind(request.kind)) throw new ApiError(400, 'validation', 'Announcement kind is invalid.'); query.set('kind', request.kind); }
  return query.toString();
}

function toAdminAnnouncement(value: unknown): AdminAnnouncement {
  if (!isRecord(value) || !exactKeys(value, ['Id', 'Title', 'BodyMarkdown', 'Kind', 'Status', 'ContentVersion', 'Revision', 'PublishedAt', 'CreatedAt', 'UpdatedAt']) || !validUuid(value.Id) || !validText(value.Title, 200) || !validMultilineText(value.BodyMarkdown, 32_000) || !isKind(value.Kind) || !isStatus(value.Status) || !positiveInteger(value.Revision) || !isNonNegativeInteger(value.ContentVersion) || (value.PublishedAt !== null && !validDate(value.PublishedAt)) || !validDate(value.CreatedAt) || !validDate(value.UpdatedAt)) throw invalidResponse('announcement');
  if ((value.Status === 'Published') !== (value.PublishedAt !== null) || (value.Status === 'Published' && value.ContentVersion <= 0)) throw invalidResponse('announcement');
  return { id: value.Id, title: value.Title, bodyMarkdown: value.BodyMarkdown, kind: value.Kind, status: value.Status, contentVersion: value.ContentVersion, revision: value.Revision, publishedAt: value.PublishedAt, createdAt: value.CreatedAt, updatedAt: value.UpdatedAt };
}

function toClientAnnouncement(value: unknown): ClientAnnouncement {
  if (!isRecord(value) || !exactKeys(value, ['Id', 'Title', 'BodyMarkdown', 'Kind', 'ContentVersion', 'PublishedAt', 'IsRead']) || !validUuid(value.Id) || !validText(value.Title, 200) || !validMultilineText(value.BodyMarkdown, 32_000) || !isKind(value.Kind) || !positiveInteger(value.ContentVersion) || !validDate(value.PublishedAt) || typeof value.IsRead !== 'boolean') throw invalidResponse('announcement');
  return { id: value.Id, title: value.Title, bodyMarkdown: value.BodyMarkdown, kind: value.Kind, contentVersion: value.ContentVersion, publishedAt: value.PublishedAt, isRead: value.IsRead };
}

function exactKeys(value: Record<string, unknown>, keys: string[]): boolean { const actual = Object.keys(value); return actual.length === keys.length && keys.every((key) => Object.hasOwn(value, key)); }
function positiveInteger(value: unknown): value is number { return typeof value === 'number' && Number.isSafeInteger(value) && value > 0; }
function requirePositive(value: number, subject: string) { if (!positiveInteger(value)) throw new ApiError(400, 'validation', `Announcement ${subject} is invalid.`); }
function requireUuid(value: string) { if (!validUuid(value)) throw new ApiError(400, 'validation', 'Announcement ID is invalid.'); }
function isKind(value: unknown): value is AnnouncementKind { return value === 'Popup' || value === 'Standard'; }
function isStatus(value: unknown): value is AnnouncementStatus { return value === 'Draft' || value === 'Published' || value === 'Archived'; }
