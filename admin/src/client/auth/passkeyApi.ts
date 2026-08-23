import {
  startAuthentication,
  startRegistration,
  type PublicKeyCredentialCreationOptionsJSON,
  type PublicKeyCredentialRequestOptionsJSON,
} from '@simplewebauthn/browser';
import { clientRequest } from '../api/clientApi';

interface CeremonyStart<T> { ChallengeId: string; Options: T }
export interface PasskeyAuthentication { AccessToken?: string }
export interface PasskeySummary { Id: string; Name: string; CreatedAt: string; LastUsedAt: string }

export async function authenticateWithPasskey(): Promise<PasskeyAuthentication> {
  const start = await clientRequest<CeremonyStart<PublicKeyCredentialRequestOptionsJSON>>('/Auth/Passkey/Authenticate/Start', { method: 'POST' });
  const response = await startAuthentication({ optionsJSON: start.Options });
  return clientRequest('/Auth/Passkey/Authenticate/Finish', { method: 'POST', body: JSON.stringify({ challengeId: start.ChallengeId, response }) });
}

export async function registerPasskey(): Promise<void> {
  const start = await clientRequest<CeremonyStart<PublicKeyCredentialCreationOptionsJSON>>('/Users/Me/Passkeys/Register/Start', { method: 'POST' });
  const response = await startRegistration({ optionsJSON: start.Options });
  await clientRequest('/Users/Me/Passkeys/Register/Finish', { method: 'POST', body: JSON.stringify({ challengeId: start.ChallengeId, response }) });
}

export function listPasskeys(): Promise<PasskeySummary[]> { return clientRequest('/Users/Me/Passkeys'); }
export function deletePasskey(id: string): Promise<void> { return clientRequest(`/Users/Me/Passkeys/${encodeURIComponent(id)}`, { method: 'DELETE' }); }
