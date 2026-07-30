import { Toast } from '@heroui/react';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderWithAdmin } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
import { MetadataSettingsPage } from './MetadataSettingsPage';
import {
  deleteTmdbSettings,
  getTmdbSettings,
  saveTmdbSettings,
  testTmdbConnection,
  type TmdbSettings,
} from './metadataSettingsApi';

vi.mock('./metadataSettingsApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('./metadataSettingsApi')>();
  return {
    ...original,
    deleteTmdbSettings: vi.fn(),
    getTmdbSettings: vi.fn(),
    saveTmdbSettings: vi.fn(),
    testTmdbConnection: vi.fn(),
  };
});

const getMock = vi.mocked(getTmdbSettings);
const saveMock = vi.mocked(saveTmdbSettings);
const testMock = vi.mocked(testTmdbConnection);
const deleteMock = vi.mocked(deleteTmdbSettings);

const databaseSettings: TmdbSettings = {
  provider: 'Tmdb',
  configured: true,
  enabled: true,
  language: 'zh-CN',
  revision: 2,
  source: 'Database',
  encryptionAvailable: true,
};

function renderPage() {
  return renderWithAdmin(
    <>
      <MetadataSettingsPage />
      <AdminNotifications />
    </>,
    { initialEntries: ['/admin/settings/metadata'], strict: true },
  );
}

beforeEach(() => {
  getMock.mockReset();
  saveMock.mockReset();
  testMock.mockReset();
  deleteMock.mockReset();
  getMock.mockResolvedValue(databaseSettings);
  saveMock.mockResolvedValue({ ...databaseSettings, revision: 3 });
  testMock.mockResolvedValue(undefined);
  deleteMock.mockResolvedValue(undefined);
});

it('loads a redacted configured state and never fills the token input', async () => {
  renderPage();

  expect(await screen.findByRole('heading', { name: 'Metadata' })).toBeVisible();
  expect(screen.getByText('Database override')).toBeVisible();
  expect(screen.getByText('Enabled')).toBeVisible();
  const token = screen.getByLabelText('TMDB API Read Access Token');
  expect(token).toHaveValue('');
  expect(token).toHaveAttribute('type', 'password');
  expect(screen.queryByDisplayValue(/token/iu)).not.toBeInTheDocument();
});

it('shows and hides only the local draft token', async () => {
  renderPage();
  const user = userEvent.setup();
  const token = await screen.findByLabelText('TMDB API Read Access Token');

  await user.type(token, 'private-draft');
  await user.click(screen.getByRole('button', { name: 'Show access token' }));
  expect(token).toHaveAttribute('type', 'text');
  expect(token).toHaveValue('private-draft');
  await user.click(screen.getByRole('button', { name: 'Hide access token' }));
  expect(token).toHaveAttribute('type', 'password');
});

it('tests a draft, saves it with the current revision, and clears the secret', async () => {
  const successToast = vi.spyOn(Toast.toast, 'success').mockReturnValue('metadata-success');
  renderPage();
  const user = userEvent.setup();
  const token = await screen.findByLabelText('TMDB API Read Access Token');
  await user.type(token, 'private-draft');
  await user.clear(screen.getByLabelText('Metadata language'));
  await user.type(screen.getByLabelText('Metadata language'), 'en-AU');

  await user.click(screen.getByRole('button', { name: 'Test connection' }));
  expect(testMock).toHaveBeenCalledWith({
    accessToken: 'private-draft',
    language: 'en-AU',
  });

  await user.click(screen.getByRole('button', { name: 'Save settings' }));
  await waitFor(() => {
    expect(saveMock).toHaveBeenCalledWith({
      enabled: true,
      language: 'en-AU',
      accessToken: 'private-draft',
      revision: 2,
    });
  });
  expect(token).toHaveValue('');
  expect(successToast).toHaveBeenCalledWith(
    'TMDB metadata settings saved.',
    expect.any(Object),
  );
  expect(JSON.stringify(successToast.mock.calls)).not.toContain('private-draft');
});

it('tests the configured token when the replacement field is empty', async () => {
  renderPage();
  const user = userEvent.setup();

  await user.click(await screen.findByRole('button', { name: 'Test connection' }));

  expect(testMock).toHaveBeenCalledWith({});
});

it('keeps a conflicting draft and offers a safe reload', async () => {
  saveMock.mockRejectedValue({ category: 'conflict', message: 'private-server-detail' });
  renderPage();
  const user = userEvent.setup();
  const token = await screen.findByLabelText('TMDB API Read Access Token');
  await user.type(token, 'private-draft');
  await user.click(screen.getByRole('button', { name: 'Save settings' }));

  const alert = await screen.findByRole('alert');
  expect(alert).toHaveTextContent('Settings changed elsewhere');
  expect(alert).not.toHaveTextContent('private-server-detail');
  expect(token).toHaveValue('private-draft');
  await user.click(within(alert).getByRole('button', { name: 'Reload latest' }));
  await waitFor(() => { expect(getMock).toHaveBeenCalledTimes(2); });
  expect(token).toHaveValue('');
});

it('removes only the database override after confirmation', async () => {
  getMock
    .mockResolvedValueOnce(databaseSettings)
    .mockResolvedValueOnce({
      ...databaseSettings,
      revision: null,
      source: 'Environment',
    });
  renderPage();
  const user = userEvent.setup();

  await user.click(await screen.findByRole('button', { name: 'Remove database override' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog).toHaveTextContent('environment configuration');
  await user.click(within(dialog).getByRole('button', { name: 'Remove override' }));

  await waitFor(() => { expect(deleteMock).toHaveBeenCalledOnce(); });
  expect(await screen.findByText('Environment fallback')).toBeVisible();
});
