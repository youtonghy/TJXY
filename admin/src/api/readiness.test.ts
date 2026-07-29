import { checkServerReadiness } from './readiness';

const fetchMock = vi.fn<typeof fetch>();

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

it('checks the same-origin readiness endpoint without authorization', async () => {
  fetchMock.mockResolvedValue(new Response('ready', { status: 200 }));
  const controller = new AbortController();

  await expect(checkServerReadiness(controller.signal)).resolves.toBe(true);

  const request = fetchMock.mock.calls[0]?.[0];
  expect(request).toBeInstanceOf(Request);
  expect((request as Request).url).toBe(`${window.location.origin}/health/ready`);
  expect((request as Request).headers.has('Authorization')).toBe(false);
  expect((request as Request).signal.aborted).toBe(false);
});

it.each([
  ['unavailable response', () => Promise.resolve(new Response('not ready', { status: 503 }))],
  ['network failure', () => Promise.reject(new TypeError('offline'))],
])('returns false for %s', async (_label, response) => {
  fetchMock.mockImplementation(response);
  await expect(checkServerReadiness(new AbortController().signal)).resolves.toBe(false);
});

it('passes request abortion through the supplied signal', async () => {
  fetchMock.mockImplementation((input) => new Promise((_resolve, reject) => {
    const request = input as Request;
    request.signal.addEventListener('abort', () => {
      reject(new DOMException('Aborted', 'AbortError'));
    });
  }));
  const controller = new AbortController();

  const result = checkServerReadiness(controller.signal);
  controller.abort();

  await expect(result).resolves.toBe(false);
  expect((fetchMock.mock.calls[0]?.[0] as Request).signal.aborted).toBe(true);
});
