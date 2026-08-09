import { ApiError, apiRequest } from './httpClient';

const fetchMock = vi.fn<typeof fetch>();

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubGlobal('fetch', fetchMock);
  vi.stubGlobal('crypto', { randomUUID: () => '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11' });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

it('sends canonical identity headers for login without query credentials', async () => {
  fetchMock.mockResolvedValue(new Response(JSON.stringify({ AccessToken: 'issued' }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  }));

  await apiRequest('/Users/AuthenticateByName', {
    auth: 'identity',
    method: 'POST',
    body: JSON.stringify({ Username: 'Admin', Pw: 'secret' }),
  });

  const request = fetchMock.mock.calls[0]?.[0] as Request;
  expect(request.url).not.toContain('secret');
  expect(request.headers.get('Authorization')).toBe(
    'MediaBrowser Client="TJXY Admin", Device="Browser", DeviceId="018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11", Version="0.1.0"',
  );
  expect(request.headers.get('Content-Type')).toBe('application/json');
});

it('sends the session token in a canonical header', async () => {
  sessionStorage.setItem('tjxy.web.token', 'secret-token');
  fetchMock.mockResolvedValue(new Response(JSON.stringify({ Id: 'u1' }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  }));

  await apiRequest('/Users/Me');

  const request = fetchMock.mock.calls[0]?.[0] as Request;
  expect(request.url).not.toContain('secret-token');
  expect(request.headers.get('Authorization')).toBe('MediaBrowser Token="secret-token"');
  expect(request.headers.has('Content-Type')).toBe(false);
});

it.each([204, 205])('returns undefined for an empty %s response', async (status) => {
  sessionStorage.setItem('tjxy.web.token', 'token');
  fetchMock.mockResolvedValue(new Response(null, { status }));

  await expect(apiRequest('/Users/u1/Policy', { method: 'POST' })).resolves.toBeUndefined();
});

it('rejects absolute and non-root-relative request paths', async () => {
  await expect(apiRequest('https://example.invalid/Users')).rejects.toMatchObject({
    category: 'validation',
  });
  await expect(apiRequest('Users')).rejects.toMatchObject({ category: 'validation' });
  expect(fetchMock).not.toHaveBeenCalled();
});

it('maps network failure without exposing the original message', async () => {
  sessionStorage.setItem('tjxy.web.token', 'token');
  fetchMock.mockRejectedValue(new Error('socket secret'));

  const error = await apiRequest('/Users').catch((caught: unknown) => caught);
  expect(error).toMatchObject({ name: 'ApiError', status: 0, category: 'network' });
  expect((error as Error).message).not.toContain('socket secret');
});

it('rejects malformed successful JSON', async () => {
  sessionStorage.setItem('tjxy.web.token', 'token');
  fetchMock.mockResolvedValue(new Response('{', {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  }));

  await expect(apiRequest('/Users')).rejects.toMatchObject({
    status: 200,
    category: 'invalid-response',
  });
});

it('rejects an unexpected successful content type', async () => {
  sessionStorage.setItem('tjxy.web.token', 'token');
  fetchMock.mockResolvedValue(new Response('<html></html>', {
    status: 200,
    headers: { 'Content-Type': 'text/html' },
  }));

  await expect(apiRequest('/Users')).rejects.toMatchObject({
    status: 200,
    category: 'invalid-response',
  });
});

it.each([
  [400, 'validation'],
  [401, 'authentication'],
  [403, 'authorization'],
  [404, 'not-found'],
  [409, 'conflict'],
  [503, 'unavailable'],
  [418, 'unexpected'],
] as const)('maps HTTP %s to %s without echoing the response body', async (status, category) => {
  sessionStorage.setItem('tjxy.web.token', 'token');
  fetchMock.mockResolvedValue(new Response('database detail', { status }));

  const error = await apiRequest('/Users/u1').catch((caught: unknown) => caught);
  expect(error).toBeInstanceOf(ApiError);
  expect(error).toMatchObject({ status, category });
  expect((error as Error).message).not.toContain('database detail');
});
