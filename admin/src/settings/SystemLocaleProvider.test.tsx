import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { API_BASE_CHANGED_EVENT } from '../client/api/apiBase';
import { SystemLocaleProvider, useSystemLocale } from './SystemLocaleProvider';

const settingsApi = vi.hoisted(() => ({ getPublicSystemSettings: vi.fn() }));

vi.mock('./systemSettingsApi', () => ({
  getPublicSystemSettings: settingsApi.getPublicSystemSettings,
}));

const publicSettings = {
  locale: 'zh-CN' as const,
  siteTitle: 'TJXY',
  siteSubtitle: 'Media',
  logoUrl: '/brand/tjxy-mark.webp',
  iconUrl: '/brand/favicon.svg',
  publicUrl: '',
  listenHost: '127.0.0.1',
  port: 8096,
  revision: 0,
  restartRequired: false,
  environmentOverrides: {
    siteTitle: false,
    publicUrl: false,
    listenAddress: false,
  },
  theme: { id: 'classic', schemaVersion: 1, options: {}, revision: 0 },
};

function LocaleProbe() {
  const { locale, setLocale } = useSystemLocale();
  return <button onClick={() => { setLocale('zh-CN'); }}>{locale}</button>;
}

beforeEach(() => {
  window.localStorage.clear();
  settingsApi.getPublicSystemSettings.mockReset();
  settingsApi.getPublicSystemSettings.mockResolvedValue(publicSettings);
});

it('keeps an explicit device locale when server settings load', async () => {
  window.localStorage.setItem('tjxy-device-locale', 'en-US');
  const user = userEvent.setup();
  render(<SystemLocaleProvider><LocaleProbe /></SystemLocaleProvider>);

  expect(await screen.findByRole('button', { name: 'en-US' })).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'en-US' }));
  expect(window.localStorage.getItem('tjxy-device-locale')).toBe('zh-CN');
});

it('reloads public settings after the configured server changes', async () => {
  render(<SystemLocaleProvider><LocaleProbe /></SystemLocaleProvider>);
  await waitFor(() => { expect(settingsApi.getPublicSystemSettings).toHaveBeenCalledTimes(1); });

  window.dispatchEvent(new Event(API_BASE_CHANGED_EVENT));
  await waitFor(() => { expect(settingsApi.getPublicSystemSettings).toHaveBeenCalledTimes(2); });
});
