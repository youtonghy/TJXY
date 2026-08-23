import {
  startAuthentication,
  startRegistration,
  type PublicKeyCredentialCreationOptionsJSON,
  type PublicKeyCredentialRequestOptionsJSON,
} from '@simplewebauthn/browser';
import { clientRequest } from '../api/clientApi';

export function passkeyErrorMessage(reason: unknown): string {
  if (!(reason instanceof Error)) return 'Passkey operation failed.';
  if (reason.name === 'NotAllowedError') return 'Passkey request was cancelled or no authenticator is available.';
  if (reason.name === 'SecurityError') return 'Passkey requires a secure site origin such as HTTPS or localhost.';
  if (reason.name === 'NotSupportedError') return 'This browser does not support Passkey login.';
  return reason.message || 'Passkey operation failed.';
}

interface CeremonyStart<T> { ChallengeId: string; Options: { publicKey: T } }
export interface PasskeyAuthentication { AccessToken?: string }
export interface PasskeySummary { Id: string; Name: string; CreatedAt: string; LastUsedAt: string }

function isLoopbackHost(): boolean {
  return globalThis.location.hostname === 'localhost'
    || globalThis.location.hostname === '127.0.0.1'
    || globalThis.location.hostname === '[::1]'
    || globalThis.location.hostname === '::1';
}

function decodeBase64Url(value: string): ArrayBuffer {
  const normalized = value.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(value.length / 4) * 4, '=');
  const binary = globalThis.atob(normalized);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0)).buffer;
}

function encodeBase64Url(value: ArrayBuffer): string {
  const binary = String.fromCharCode(...new Uint8Array(value));
  return globalThis.btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/u, '');
}

async function startRegistrationCompat(optionsJSON: PublicKeyCredentialCreationOptionsJSON) {
  if (!isLoopbackHost()) return startRegistration({ optionsJSON });
  if (typeof globalThis.PublicKeyCredential !== 'function') throw new Error('WebAuthn is not supported in this browser');
  const credential = await globalThis.navigator.credentials.create({
    publicKey: {
      ...optionsJSON,
      challenge: decodeBase64Url(optionsJSON.challenge),
      user: { ...optionsJSON.user, id: decodeBase64Url(optionsJSON.user.id) },
      excludeCredentials: optionsJSON.excludeCredentials?.map((item) => ({ id: decodeBase64Url(item.id), type: item.type })),
    },
  });
  if (!(credential instanceof globalThis.PublicKeyCredential)) throw new Error('Registration was not completed');
  const response = credential.response as AuthenticatorAttestationResponse;
  return {
    id: credential.id,
    rawId: encodeBase64Url(credential.rawId),
    response: { attestationObject: encodeBase64Url(response.attestationObject), clientDataJSON: encodeBase64Url(response.clientDataJSON), transports: response.getTransports() },
    type: credential.type,
    clientExtensionResults: credential.getClientExtensionResults(),
    authenticatorAttachment: credential.authenticatorAttachment,
  };
}

async function startAuthenticationCompat(optionsJSON: PublicKeyCredentialRequestOptionsJSON) {
  if (!isLoopbackHost()) return startAuthentication({ optionsJSON });
  if (typeof globalThis.PublicKeyCredential !== 'function') throw new Error('WebAuthn is not supported in this browser');
  const credential = await globalThis.navigator.credentials.get({
    publicKey: {
      ...optionsJSON,
      challenge: decodeBase64Url(optionsJSON.challenge),
      allowCredentials: optionsJSON.allowCredentials?.map((item) => ({ id: decodeBase64Url(item.id), type: item.type })),
    },
  });
  if (!(credential instanceof globalThis.PublicKeyCredential)) throw new Error('Authentication was not completed');
  const response = credential.response as AuthenticatorAssertionResponse;
  return {
    id: credential.id,
    rawId: encodeBase64Url(credential.rawId),
    response: { authenticatorData: encodeBase64Url(response.authenticatorData), clientDataJSON: encodeBase64Url(response.clientDataJSON), signature: encodeBase64Url(response.signature), userHandle: response.userHandle ? encodeBase64Url(response.userHandle) : undefined },
    type: credential.type,
    clientExtensionResults: credential.getClientExtensionResults(),
    authenticatorAttachment: credential.authenticatorAttachment,
  };
}

export async function authenticateWithPasskey(): Promise<PasskeyAuthentication> {
  const start = await clientRequest<CeremonyStart<PublicKeyCredentialRequestOptionsJSON>>('/Auth/Passkey/Authenticate/Start', { method: 'POST' });
  const response = await startAuthenticationCompat(start.Options.publicKey);
  return clientRequest('/Auth/Passkey/Authenticate/Finish', { method: 'POST', body: JSON.stringify({ challengeId: start.ChallengeId, response }) });
}

export async function registerPasskey(): Promise<void> {
  const start = await clientRequest<CeremonyStart<PublicKeyCredentialCreationOptionsJSON>>('/Users/Me/Passkeys/Register/Start', { method: 'POST' });
  const response = await startRegistrationCompat(start.Options.publicKey);
  await clientRequest('/Users/Me/Passkeys/Register/Finish', { method: 'POST', body: JSON.stringify({ challengeId: start.ChallengeId, response }) });
}

export function listPasskeys(): Promise<PasskeySummary[]> { return clientRequest('/Users/Me/Passkeys'); }
export function deletePasskey(id: string): Promise<void> { return clientRequest(`/Users/Me/Passkeys/${encodeURIComponent(id)}`, { method: 'DELETE' }); }
