import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderWithAdmin } from '../test/renderWithAdmin';
import { SystemSettingsPage } from './SystemSettingsPage';
import {
  getSystemSettings,
  restartSystem,
  saveSystemSettings,
  uploadBrandAsset,
} from './systemSettingsApi';

vi.mock('./systemSettingsApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('./systemSettingsApi')>();
  return {
    ...original,
    getSystemSettings: vi.fn(),
    restartSystem: vi.fn(),
    saveSystemSettings: vi.fn(),
    uploadBrandAsset: vi.fn(),
  };
});

const settings = {
  locale: 'en-US' as const,
  siteTitle: 'TJXY',
  siteSubtitle: 'Your media library',
  logoUrl: '/brand/tjxy-mark.webp',
  iconUrl: '/brand/favicon.svg',
  publicUrl: 'https://media.example.com',
  listenHost: '127.0.0.1',
  port: 8096,
  mediaBrowserRoots: ['/media/movies'],
  revision: 2,
  restartRequired: false,
  environmentOverrides: {
    siteTitle: false,
    publicUrl: false,
    listenAddress: false,
    mediaBrowserRoots: false,
  },
};

const getMock = vi.mocked(getSystemSettings);
const saveMock = vi.mocked(saveSystemSettings);
const restartMock = vi.mocked(restartSystem);
const uploadMock = vi.mocked(uploadBrandAsset);

beforeEach(() => {
  getMock.mockReset().mockResolvedValue(settings);
  saveMock.mockReset().mockResolvedValue({ ...settings, siteTitle: 'Cinema', revision: 3, restartRequired: true });
  restartMock.mockReset().mockResolvedValue(undefined);
  uploadMock.mockReset().mockResolvedValue({ url: '/Branding/Assets/logo-upload.webp' });
});

it('edits branding and network settings through one save action', async () => {
  renderWithAdmin(<SystemSettingsPage />, { initialEntries: ['/admin/settings/system'] });
  const user = userEvent.setup();

  await user.clear(await screen.findByLabelText('Site title'));
  await user.type(screen.getByLabelText('Site title'), 'Cinema');
  await user.clear(screen.getByLabelText('Port'));
  await user.type(screen.getByLabelText('Port'), '9000');
  await user.click(screen.getByRole('button', { name: 'Save settings' }));

  await waitFor(() => {
    expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({
      siteTitle: 'Cinema',
      port: 9000,
      revision: 2,
    }));
  });
  expect(await screen.findByText('Restart required')).toBeVisible();
});

it('uploads a local logo and keeps a URL field as the advanced option', async () => {
  renderWithAdmin(<SystemSettingsPage />, { initialEntries: ['/admin/settings/system'] });
  const user = userEvent.setup();
  const file = new File(['image'], 'logo.webp', { type: 'image/webp' });

  await user.upload(await screen.findByLabelText('Upload logo'), file);

  await waitFor(() => { expect(uploadMock).toHaveBeenCalledWith('logo', file); });
  expect(screen.getByLabelText('Logo URL')).toHaveValue('/Branding/Assets/logo-upload.webp');
});

it('offers an explicit restart beside the shared save action', async () => {
  renderWithAdmin(<SystemSettingsPage />, { initialEntries: ['/admin/settings/system'] });
  const user = userEvent.setup();

  await user.click(await screen.findByRole('button', { name: 'Restart service' }));

  await waitFor(() => { expect(restartMock).toHaveBeenCalledOnce(); });
});

it('manages multiple media browser roots through the shared save action', async () => {
  renderWithAdmin(<SystemSettingsPage />, { initialEntries: ['/admin/settings/system'] });
  const user = userEvent.setup();

  const firstRoot = await screen.findByLabelText('Media browser root 1');
  await user.clear(firstRoot);
  await user.type(firstRoot, '/media/music');
  await user.click(screen.getByRole('button', { name: 'Add media browser root' }));
  await user.type(screen.getByLabelText('Media browser root 2'), '/media/concerts');
  await user.click(screen.getByRole('button', { name: 'Save settings' }));

  await waitFor(() => {
    expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({
      mediaBrowserRoots: ['/media/music', '/media/concerts'],
    }));
  });
});
