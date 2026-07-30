import { clientRequest } from './clientApi';
export interface PlaybackSource { Id: string; Container?: string; SupportsDirectPlay?: boolean; DirectStreamUrl?: string; MediaStreams?: { Type?: string; DeliveryUrl?: string; IsExternal?: boolean; Index?: number; }[]; }
export interface PlaybackInfo { MediaSources?: PlaybackSource[]; PlaySessionId?: string; }
export interface PlaybackTicket { Id: string; Ticket: string; ExpiresAt: string; StreamUrl: string; }
export async function getPlaybackInfo(itemId: string): Promise<PlaybackInfo> { return clientRequest<PlaybackInfo>(`/Items/${itemId}/PlaybackInfo`, { method: 'POST', body: JSON.stringify({}) }); }
export async function issuePlaybackTicket(itemId: string, mediaSourceId: string, playSessionId: string): Promise<PlaybackTicket> { return clientRequest<PlaybackTicket>(`/Items/${itemId}/PlaybackTicket`, { method: 'POST', body: JSON.stringify({ MediaSourceId: mediaSourceId, PlaySessionId: playSessionId }) }); }
export async function revokePlaybackTicket(id: string): Promise<void> { await clientRequest(`/PlaybackTickets/${id}`, { method: 'DELETE' }); }
