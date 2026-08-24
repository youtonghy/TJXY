import { startAuthentication, startRegistration } from '@simplewebauthn/browser';
import { clientRequest } from '../api/clientApi';
import { authenticateWithPasskey, registerPasskey } from './passkeyApi';

vi.mock('@simplewebauthn/browser', () => ({
  startAuthentication: vi.fn(),
  startRegistration: vi.fn(),
}));
vi.mock('../api/clientApi', () => ({ clientRequest: vi.fn() }));

class FakePublicKeyCredential {
  id = 'credential-1';
  rawId = new Uint8Array([1]).buffer;
  type = 'public-key' as const;
  response = {
    attestationObject: new Uint8Array([2]).buffer,
    authenticatorData: new Uint8Array([4]).buffer,
    clientDataJSON: new Uint8Array([3]).buffer,
    getTransports: () => [],
    signature: new Uint8Array([5]).buffer,
    userHandle: null,
  } as unknown as AuthenticatorAttestationResponse & AuthenticatorAssertionResponse;
  getClientExtensionResults = () => ({});
  authenticatorAttachment = 'platform' as const;
}

const requestMock = vi.mocked(clientRequest);
const authMock = vi.mocked(startAuthentication);
const registrationMock = vi.mocked(startRegistration);
const credentialCreateMock = vi.fn<(options?: CredentialCreationOptions) => Promise<Credential | null>>();
const credentialGetMock = vi.fn<(options?: CredentialRequestOptions) => Promise<Credential | null>>();

beforeEach(() => {
  requestMock.mockReset();
  authMock.mockReset().mockResolvedValue({} as never);
  registrationMock.mockReset().mockResolvedValue({} as never);
  credentialCreateMock.mockReset().mockResolvedValue(new FakePublicKeyCredential());
  credentialGetMock.mockReset().mockResolvedValue(new FakePublicKeyCredential());
  vi.stubGlobal('PublicKeyCredential', FakePublicKeyCredential);
  vi.stubGlobal('navigator', { credentials: { create: credentialCreateMock, get: credentialGetMock } });
});

it('unwraps the standard publicKey envelope for registration', async () => {
  requestMock
    .mockResolvedValueOnce({ ChallengeId: 'challenge-1', Options: { publicKey: { challenge: 'abc', user: { id: 'user' } } } })
    .mockResolvedValueOnce(undefined);

  await registerPasskey();

  expect(registrationMock).not.toHaveBeenCalled();
});

it('unwraps the standard publicKey envelope for authentication', async () => {
  requestMock
    .mockResolvedValueOnce({ ChallengeId: 'challenge-2', Options: { publicKey: { challenge: 'def', allowCredentials: [{ id: 'AQID', type: 'public-key', transports: ['usb'] }] } } })
    .mockResolvedValueOnce({ AccessToken: 'token' });

  await authenticateWithPasskey('Alice');

  expect(authMock).not.toHaveBeenCalled();
  expect(requestMock).toHaveBeenNthCalledWith(1, '/Auth/Passkey/Authenticate/Start', {
    method: 'POST',
    body: JSON.stringify({ username: 'Alice' }),
  });
  const publicKey = credentialGetMock.mock.calls[0]?.[0]?.publicKey;
  expect(publicKey?.allowCredentials?.[0]?.type).toBe('public-key');
  expect(publicKey?.allowCredentials?.[0]?.transports).toEqual(['usb']);
  expect(requestMock.mock.calls[1]?.[0]).toBe('/Auth/Passkey/Authenticate/Finish');
  expect(requestMock.mock.calls[1]?.[1]?.method).toBe('POST');
  expect(requestMock.mock.calls[1]?.[1]?.body).toContain('"challengeId":"challenge-2"');
});

it('keeps username-less authentication available', async () => {
  requestMock
    .mockResolvedValueOnce({ ChallengeId: 'challenge-3', Options: { publicKey: { challenge: 'def', allowCredentials: [] } } })
    .mockResolvedValueOnce({ AccessToken: 'token' });

  await authenticateWithPasskey();

  expect(requestMock).toHaveBeenNthCalledWith(1, '/Auth/Passkey/Authenticate/Start', {
    method: 'POST',
    body: '{}',
  });
});

it('does not call finish when the browser rejects the credential request', async () => {
  requestMock.mockResolvedValueOnce({ ChallengeId: 'challenge-4', Options: { publicKey: { challenge: 'def' } } });
  credentialGetMock.mockRejectedValueOnce(new DOMException('No matching credential', 'NotAllowedError'));

  await expect(authenticateWithPasskey('Alice')).rejects.toMatchObject({ name: 'NotAllowedError' });

  expect(requestMock).toHaveBeenCalledTimes(1);
});
