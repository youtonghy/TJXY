export type AnnouncementKind = 'Popup' | 'Standard';
export type AnnouncementStatus = 'Draft' | 'Published' | 'Archived';

export interface AnnouncementContent {
  title: string;
  bodyMarkdown: string;
  kind: AnnouncementKind;
}

export interface AdminAnnouncement extends AnnouncementContent {
  id: string;
  status: AnnouncementStatus;
  contentVersion: number;
  revision: number;
  publishedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ClientAnnouncement extends AnnouncementContent {
  id: string;
  contentVersion: number;
  publishedAt: string;
  isRead: boolean;
}

export interface AdminAnnouncementPage {
  items: AdminAnnouncement[];
  total: number;
}

export interface ClientAnnouncementPage {
  items: ClientAnnouncement[];
  total: number;
  unreadCount: number;
}

export interface AnnouncementPageRequest {
  startIndex: number;
  limit: number;
}

export interface AdminAnnouncementPageRequest extends AnnouncementPageRequest {
  status?: AnnouncementStatus;
  kind?: AnnouncementKind;
}

