import react from '@vitejs/plugin-react';
import { configDefaults, defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    exclude: [...configDefaults.exclude, 'e2e/**'],
    globals: true,
    restoreMocks: true,
    setupFiles: ['./src/test/setup.ts'],
    testTimeout: 15_000,
  },
});
