import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { checkClientUpdate } from '../../clientReleases/clientReleaseApi';
import { ClientUpdateCard } from './ClientUpdateCard';
import type { NativeUpdater } from './nativeUpdater';

vi.mock('../../clientReleases/clientReleaseApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('../../clientReleases/clientReleaseApi')>();
  return { ...original, checkClientUpdate: vi.fn() };
});
vi.mock('../auth/clientSession', () => ({ getClientToken: () => 'token-1' }));
vi.mock('../api/apiBase', () => ({ getApiBaseUrl: () => 'https://media.example', resolveApiUrl: (path: string, base: string) => `${base}${path}` }));

const id = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const latest = {
  id, platform: 'windows' as const, arch: 'x64' as const, version: '1.1.0', fileName: 'TJXY-1.1.0-setup-x64.exe', size: 2048,
  sha256: 'c'.repeat(64), notes: 'Faster startup', isLatest: true, createdAt: '2026-10-22T01:00:00Z', downloadPath: `/ClientUpdates/${id}/Download`,
};

afterEach(() => { delete window.tjxyUpdater; });

it('stays hidden in a plain browser', () => {
  const { container } = render(<ClientUpdateCard />);
  expect(container).toBeEmptyDOMElement();
});

it('checks the server and hands the verified download to the native shell', async () => {
  const install = vi.fn<NativeUpdater['install']>().mockResolvedValue({ outcome: 'restarting' });
  window.tjxyUpdater = { getAppInfo: () => Promise.resolve({ platform: 'windows', arch: 'x64', version: '1.0.0' }), install };
  vi.mocked(checkClientUpdate).mockResolvedValue({ updateAvailable: true, latest });
  render(<ClientUpdateCard />);
  const user = userEvent.setup();

  expect(await screen.findByText('Faster startup')).toBeVisible();
  expect(checkClientUpdate).toHaveBeenCalledWith('windows', 'x64', '1.0.0');
  await user.click(screen.getByRole('button', { name: 'Download and install' }));

  await waitFor(() => {
    expect(install).toHaveBeenCalledWith({
      url: `https://media.example/ClientUpdates/${id}/Download`,
      headers: { Authorization: 'MediaBrowser Token="token-1"' },
      fileName: latest.fileName,
      sha256: latest.sha256,
      size: 2048,
    });
  });
  expect(await screen.findByText(/app will close and restart/u)).toBeVisible();
});

it('shows native install failures', async () => {
  window.tjxyUpdater = { getAppInfo: () => Promise.resolve({ platform: 'android', arch: 'universal', version: '1.0.0' }), install: () => Promise.reject(new Error('下载更新失败')) };
  vi.mocked(checkClientUpdate).mockResolvedValue({ updateAvailable: true, latest: { ...latest, platform: 'android', arch: 'universal' } });
  render(<ClientUpdateCard />);
  const user = userEvent.setup();

  await user.click(await screen.findByRole('button', { name: 'Download and install' }));
  expect(await screen.findByText('下载更新失败')).toBeVisible();
});
