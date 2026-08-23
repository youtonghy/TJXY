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
  response = { attestationObject: new Uint8Array([2]).buffer, clientDataJSON: new Uint8Array([3]).buffer, getTransports: () => [] } as unknown as AuthenticatorAttestationResponse;
  getClientExtensionResults = () => ({});
  authenticatorAttachment = 'platform' as const;
}

const requestMock = vi.mocked(clientRequest);
const authMock = vi.mocked(startAuthentication);
const registrationMock = vi.mocked(startRegistration);

beforeEach(() => {
  requestMock.mockReset();
  authMock.mockReset().mockResolvedValue({} as never);
  registrationMock.mockReset().mockResolvedValue({} as never);
  vi.stubGlobal('PublicKeyCredential', FakePublicKeyCredential);
  vi.stubGlobal('navigator', { credentials: { create: vi.fn().mockResolvedValue(new FakePublicKeyCredential()), get: vi.fn().mockResolvedValue(new FakePublicKeyCredential()) } });
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
    .mockResolvedValueOnce({ ChallengeId: 'challenge-2', Options: { publicKey: { challenge: 'def' } } })
    .mockResolvedValueOnce({ AccessToken: 'token' });

  await authenticateWithPasskey();

  expect(authMock).not.toHaveBeenCalled();
});
