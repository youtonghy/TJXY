import { clientRequest } from '../api/clientApi';

export interface QrChallenge {
  ChallengeId: string;
  PollToken: string;
  QrPayload: string;
  ExpiresAt: string;
}

export interface QrPreview {
  ChallengeId: string;
  DeviceName: string;
  ClientName: string;
  ApplicationVersion: string;
  ExpiresAt: string;
}

export interface QrAuthentication {
  User: { Id: string; Name: string; Policy?: { IsDisabled?: boolean; IsAdministrator?: boolean } };
  SessionInfo: { Id: string };
  AccessToken: string;
}

export interface QrPollResult {
  State: 'Pending' | 'Approved';
  ExpiresAt: string;
  Authentication?: QrAuthentication;
}

export function createQrChallenge(): Promise<QrChallenge> {
  return clientRequest<QrChallenge>('/Auth/Qr/Challenges', { body: JSON.stringify({}), method: 'POST' });
}

export function pollQrChallenge(challengeId: string, token: string, signal?: AbortSignal): Promise<QrPollResult> {
  return clientRequest<QrPollResult>(`/Auth/Qr/Challenges/${encodeURIComponent(challengeId)}/Poll`, {
    body: JSON.stringify({ Token: token }),
    method: 'POST',
    signal,
  });
}

export function previewQrApproval(token: string): Promise<QrPreview> {
  return clientRequest<QrPreview>('/Auth/Qr/Preview', { body: JSON.stringify({ Token: token }), method: 'POST' });
}

export function approveQrLogin(token: string): Promise<void> {
  return clientRequest('/Auth/Qr/Approve', { body: JSON.stringify({ Token: token }), method: 'POST' });
}
