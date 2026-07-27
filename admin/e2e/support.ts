import { expect, type Page, type TestInfo } from '@playwright/test';

export async function login(page: Page, username: string, password: string) {
  await page.getByRole('textbox', { name: 'Username' }).fill(username);
  await page.getByRole('textbox', { name: 'Password' }).fill(password);
  await page.getByRole('button', { name: /sign in/i }).click();
}

export function safeRequestPath(url: string): string {
  if (url.length === 0) return '';
  const pathname = new URL(url).pathname;
  return /^\/Auth\/Keys\/[^/]+$/u.test(pathname)
    ? '/Auth/Keys/[REDACTED]'
    : pathname;
}

export function monitorPage(page: Page) {
  const pageErrors: string[] = [];
  const consoleErrors: string[] = [];
  const failedRequests: string[] = [];
  const expectedHttpConsoleErrors: RegExp[] = [];
  const expectedFailedHttpResponses: Array<{ status: number; path: RegExp }> = [];
  const responseStatuses = new WeakMap<object, number>();
  page.on('pageerror', (error) => { pageErrors.push(safeDiagnostic(error.message)); });
  page.on('console', (message) => {
    if (message.type() !== 'error') return;
    const pathname = safeRequestPath(message.location().url);
    const expected = expectedHttpConsoleErrors.findIndex((matcher) => matcher.test(pathname));
    if (expected >= 0) {
      expectedHttpConsoleErrors.splice(expected, 1);
      return;
    }
    consoleErrors.push(`${safeDiagnostic(message.text())} @ ${pathname}`);
  });
  page.on('response', (response) => {
    responseStatuses.set(response.request(), response.status());
  });
  page.on('requestfailed', (request) => {
    if (new URL(request.url()).origin !== new URL(page.url()).origin) return;
    const status = responseStatuses.get(request);
    const error = request.failure()?.errorText ?? 'failed';
    if (status === 204 && error === 'net::ERR_ABORTED') return;
    const pathname = safeRequestPath(request.url());
    const expected = expectedFailedHttpResponses.findIndex(
      (entry) => entry.status === status && entry.path.test(pathname),
    );
    if (expected >= 0 && error === 'net::ERR_ABORTED') {
      expectedFailedHttpResponses.splice(expected, 1);
      return;
    }
    failedRequests.push(`${request.method()} ${pathname}: ${safeDiagnostic(error)}`);
  });
  return {
    pageErrors,
    consoleErrors,
    failedRequests,
    expectHttpConsoleError(matcher: RegExp) {
      expectedHttpConsoleErrors.push(matcher);
    },
    expectFailedHttpResponse(status: number, path: RegExp) {
      expectedFailedHttpResponses.push({ status, path });
    },
  };
}

export async function assertNoHorizontalOverflow(page: Page) {
  await expect.poll(async () => {
    const layout = await page.evaluate(() => ({
      documentWidth: document.documentElement.scrollWidth,
      viewportWidth: window.innerWidth,
      offenders: Array.from(document.querySelectorAll<HTMLElement>('body *'))
        .map((element) => {
          const box = element.getBoundingClientRect();
          return {
            element: element.tagName,
            className: element.className.toString().slice(0, 120),
            left: Math.round(box.left),
            right: Math.round(box.right),
            scrollWidth: element.scrollWidth,
            clientWidth: element.clientWidth,
          };
        })
        .filter((entry) => entry.right > window.innerWidth + 1 || entry.left < -1)
        .slice(0, 8),
    }));
    return layout.documentWidth <= layout.viewportWidth ? true : JSON.stringify(layout);
  }).toBe(true);
}

export async function assertActionControlsDoNotIntersect(page: Page) {
  const controls = page.locator('a:visible, button:visible');
  const boxes = await controls.evaluateAll((elements) => elements.flatMap((element) => {
    const box = element.getBoundingClientRect();
    const style = window.getComputedStyle(element);
    if (box.width === 0 || box.height === 0 || box.right <= 0 || box.bottom <= 0
      || box.left >= window.innerWidth || box.top >= window.innerHeight || style.opacity === '0') {
      return [];
    }
    return [{
      label: element.getAttribute('aria-label') ?? element.textContent?.trim() ?? element.tagName,
      left: box.left,
      right: box.right,
      top: box.top,
      bottom: box.bottom,
    }];
  }));
  for (let first = 0; first < boxes.length; first += 1) {
    for (let second = first + 1; second < boxes.length; second += 1) {
      const a = boxes[first];
      const b = boxes[second];
      const intersects = a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;
      expect(intersects, `action controls "${a.label}" and "${b.label}" intersect`).toBe(false);
    }
  }
}

export async function screenshot(page: Page, testInfo: TestInfo, name: string) {
  await page.screenshot({ path: testInfo.outputPath(name), fullPage: true });
}

function safeDiagnostic(value: string): string {
  return value.replace(/\/Auth\/Keys\/[^\s?"')]+/gu, '/Auth/Keys/[REDACTED]');
}
