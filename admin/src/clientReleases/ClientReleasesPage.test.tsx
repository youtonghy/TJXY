import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithAdmin } from '../test/renderWithAdmin';
import { listClientReleases, markClientReleaseLatest, uploadClientRelease, type ClientRelease } from './clientReleaseApi';
import { ClientReleasesPage } from './ClientReleasesPage';

vi.mock('./clientReleaseApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('./clientReleaseApi')>();
  return {
    ...original,
    deleteClientRelease: vi.fn(),
    listClientReleases: vi.fn(),
    markClientReleaseLatest: vi.fn(),
    updateClientReleaseNotes: vi.fn(),
    uploadClientRelease: vi.fn(),
  };
});

const release: ClientRelease = {
  id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11',
  platform: 'windows',
  arch: 'x64',
  version: '1.0.0',
  fileName: 'TJXY-1.0.0-setup-x64.exe',
  size: 2048,
  sha256: 'b'.repeat(64),
  notes: '',
  isLatest: false,
  createdAt: '2026-10-22T01:00:00Z',
  downloadPath: '/ClientUpdates/018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11/Download',
};

beforeEach(() => {
  vi.mocked(listClientReleases).mockReset().mockResolvedValue([release]);
  vi.mocked(markClientReleaseLatest).mockReset().mockResolvedValue({ ...release, isLatest: true });
  vi.mocked(uploadClientRelease).mockReset().mockResolvedValue({ ...release, version: '1.1.0' });
});

it('uploads an installer with the target inferred from its name and marks releases latest', async () => {
  renderWithAdmin(<ClientReleasesPage />, { initialEntries: ['/admin/client-releases'] });
  const user = userEvent.setup();

  const table = await screen.findByRole('grid', { name: 'Client releases' });
  expect(within(table).getByText('TJXY-1.0.0-setup-x64.exe')).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Set as latest 1.0.0' }));
  await waitFor(() => { expect(markClientReleaseLatest).toHaveBeenCalledWith(release.id); });

  const [openUpload] = screen.getAllByRole('button', { name: 'Upload release' });
  if (openUpload === undefined) throw new Error('upload button missing');
  await user.click(openUpload);
  const file = new File(['installer'], 'TJXY-1.1.0-setup-arm64.dmg');
  await user.upload(screen.getByLabelText('Installer package'), file);
  expect(screen.getByLabelText('Version')).toHaveValue('1.1.0');
  await user.type(screen.getByLabelText('Release notes'), 'Faster startup');
  await user.click(screen.getByRole('button', { name: 'Upload' }));

  await waitFor(() => {
    expect(uploadClientRelease).toHaveBeenCalledWith({ platform: 'macos', arch: 'arm64', version: '1.1.0', file, notes: 'Faster startup', markLatest: true });
  });
});
