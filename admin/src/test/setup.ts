import '@testing-library/jest-dom/vitest';
import { Toast } from '@heroui/react';
import { act } from '@testing-library/react';

const initialMatchMedia = Object.getOwnPropertyDescriptor(window, 'matchMedia');

afterEach(() => {
  act(() => { Toast.toast.clear(); });
  document.querySelectorAll('[data-overlay-container="true"], [data-slot="toast-region"]')
    .forEach((element) => { element.remove(); });
  if (initialMatchMedia === undefined) {
    Reflect.deleteProperty(window, 'matchMedia');
  } else {
    Object.defineProperty(window, 'matchMedia', initialMatchMedia);
  }
  sessionStorage.clear();
});
