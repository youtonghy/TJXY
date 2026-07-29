import '@testing-library/jest-dom/vitest';
import { Toast } from '@heroui/react';
import { act } from '@testing-library/react';

const matchMedia = (query: string): MediaQueryList => ({
  matches: false,
  media: query,
  onchange: null,
  addEventListener: () => undefined,
  addListener: () => undefined,
  dispatchEvent: () => false,
  removeEventListener: () => undefined,
  removeListener: () => undefined,
});

function installMatchMedia() {
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: matchMedia,
    writable: true,
  });
}

installMatchMedia();

afterEach(() => {
  act(() => { Toast.toast.clear(); });
  document.querySelectorAll('[data-overlay-container="true"], [data-slot="toast-region"]')
    .forEach((element) => { element.remove(); });
  installMatchMedia();
  sessionStorage.clear();
});
