import { defineConfig, devices } from '@playwright/test';

const port = process.env.TJXY_E2E_PORT ?? '18096';
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: './e2e',
  timeout: 120_000,
  fullyParallel: false,
  workers: 1,
  // The lifecycle specs share one disposable database and are intentionally not replayed in-place.
  retries: 0,
  reporter: process.env.CI
    ? [['github'], ['html', { open: 'never', outputFolder: 'output/playwright/report' }]]
    : 'list',
  outputDir: 'output/playwright/test-results',
  use: {
    baseURL,
    actionTimeout: 10_000,
    colorScheme: 'light',
    contextOptions: { reducedMotion: 'reduce' },
    locale: 'en-US',
    navigationTimeout: 15_000,
    trace: 'off',
    screenshot: 'off',
    timezoneId: 'UTC',
    video: 'off',
  },
  webServer: {
    command: './scripts/run-e2e-server.sh',
    url: `${baseURL}/health/ready`,
    timeout: 180_000,
    reuseExistingServer: false,
    stdout: 'pipe',
    stderr: 'pipe',
  },
  projects: [
    {
      name: 'chromium',
      testIgnore: '**/webkit-smoke.spec.ts',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 1440, height: 900 },
      },
    },
    {
      name: 'webkit',
      testMatch: '**/webkit-smoke.spec.ts',
      use: {
        ...devices['Desktop Safari'],
        viewport: { width: 1440, height: 900 },
      },
    },
  ],
});
