import { Toast } from '@heroui/react';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderWithAdmin } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
import { MetadataSettingsPage } from './MetadataSettingsPage';
import {
  deleteMusicBrainzSettings,
  deleteTheAudioDbSettings,
  deleteTmdbSettings,
  getMusicBrainzSettings,
  getLocalMetadataStorage,
  getTheAudioDbSettings,
  getTmdbSettings,
  saveMusicBrainzSettings,
  saveTheAudioDbSettings,
  saveTmdbSettings,
  testMusicBrainzConnection,
  testTheAudioDbConnection,
  testTmdbConnection,
  type MusicBrainzSettings,
  type TheAudioDbSettings,
  type TmdbSettings,
  type LocalMetadataStorage,
} from './metadataSettingsApi';

vi.mock('./metadataSettingsApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('./metadataSettingsApi')>();
  return {
    ...original,
    deleteMusicBrainzSettings: vi.fn(),
    deleteTheAudioDbSettings: vi.fn(),
    deleteTmdbSettings: vi.fn(),
    getMusicBrainzSettings: vi.fn(),
    getLocalMetadataStorage: vi.fn(),
    cleanupLocalMetadata: vi.fn(),
    saveLocalMetadataLocation: vi.fn(),
    getTheAudioDbSettings: vi.fn(),
    getTmdbSettings: vi.fn(),
    saveMusicBrainzSettings: vi.fn(),
    saveTheAudioDbSettings: vi.fn(),
    saveTmdbSettings: vi.fn(),
    testMusicBrainzConnection: vi.fn(),
    testTheAudioDbConnection: vi.fn(),
    testTmdbConnection: vi.fn(),
  };
});

const getMock = vi.mocked(getTmdbSettings);
const saveMock = vi.mocked(saveTmdbSettings);
const testMock = vi.mocked(testTmdbConnection);
const deleteMock = vi.mocked(deleteTmdbSettings);
const getTheAudioDbMock = vi.mocked(getTheAudioDbSettings);
const saveTheAudioDbMock = vi.mocked(saveTheAudioDbSettings);
const testTheAudioDbMock = vi.mocked(testTheAudioDbConnection);
const deleteTheAudioDbMock = vi.mocked(deleteTheAudioDbSettings);
const getMusicBrainzMock = vi.mocked(getMusicBrainzSettings);
const saveMusicBrainzMock = vi.mocked(saveMusicBrainzSettings);
const testMusicBrainzMock = vi.mocked(testMusicBrainzConnection);
const deleteMusicBrainzMock = vi.mocked(deleteMusicBrainzSettings);
const getLocalMetadataMock = vi.mocked(getLocalMetadataStorage);

const databaseSettings: TmdbSettings = {
  provider: 'Tmdb',
  configured: true,
  enabled: true,
  language: 'zh-CN',
  revision: 2,
  source: 'Database',
  encryptionAvailable: true,
};

const theAudioDbSettings: TheAudioDbSettings = {
  provider: 'TheAudioDB',
  configured: true,
  enabled: true,
  revision: 4,
  source: 'Environment',
  encryptionAvailable: true,
};

const musicBrainzSettings: MusicBrainzSettings = {
  provider: 'MusicBrainz',
  configured: false,
  enabled: false,
  userAgent: '',
  revision: null,
  source: 'None',
  encryptionAvailable: true,
};

const localMetadataStorage: LocalMetadataStorage = {
  currentPath: '/var/lib/tjxy/assets', pendingPath: null, historicalLocations: [],
  source: 'Database', locationEditable: true, restartRequired: false,
  checkedAt: '2026-08-12T03:00:00Z', cleanupInProgress: false,
  statistics: {
    total: { count: 3, bytes: 300 }, linked: { count: 2, bytes: 200 },
    orphaned: { count: 1, bytes: 100 }, missing: { count: 0, bytes: 0 },
    unregistered: { count: 0, bytes: 0 },
  },
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
  getTheAudioDbMock.mockReset();
  saveTheAudioDbMock.mockReset();
  testTheAudioDbMock.mockReset();
  deleteTheAudioDbMock.mockReset();
  getMusicBrainzMock.mockReset();
  saveMusicBrainzMock.mockReset();
  testMusicBrainzMock.mockReset();
  deleteMusicBrainzMock.mockReset();
  getLocalMetadataMock.mockReset();
  getMock.mockResolvedValue(databaseSettings);
  getLocalMetadataMock.mockResolvedValue(localMetadataStorage);
  saveMock.mockResolvedValue({ ...databaseSettings, revision: 3 });
  testMock.mockResolvedValue(undefined);
  deleteMock.mockResolvedValue(undefined);
  getTheAudioDbMock.mockResolvedValue(theAudioDbSettings);
  saveTheAudioDbMock.mockResolvedValue({
    ...theAudioDbSettings,
    source: 'Database',
    revision: 5,
  });
  testTheAudioDbMock.mockResolvedValue(undefined);
  deleteTheAudioDbMock.mockResolvedValue(undefined);
  getMusicBrainzMock.mockResolvedValue(musicBrainzSettings);
  saveMusicBrainzMock.mockResolvedValue({
    ...musicBrainzSettings,
    configured: true,
    enabled: true,
    userAgent: 'TJXY/1.0 (admin@example.invalid)',
    source: 'Database',
    revision: 1,
  });
  testMusicBrainzMock.mockResolvedValue(undefined);
  deleteMusicBrainzMock.mockResolvedValue(undefined);
});

it('loads a redacted configured state and never fills the token input', async () => {
  renderPage();

  expect(await screen.findByRole('heading', { name: 'Metadata' })).toBeVisible();
  expect(screen.getByText('Database override')).toBeVisible();
  expect(screen.getAllByText('Enabled')).not.toHaveLength(0);
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
  const tmdbCard = (await screen.findByRole('heading', { name: 'TMDB' }))
    .closest('[data-slot="card"]');
  expect(tmdbCard).not.toBeNull();
  expect(within(tmdbCard as HTMLElement).getByText('Environment fallback')).toBeVisible();
});

it('loads both music providers and saves their provider-specific configuration', async () => {
  renderPage();
  const user = userEvent.setup();

  expect(await screen.findByRole('heading', { name: 'TheAudioDB' })).toBeVisible();
  expect(screen.getByRole('heading', { name: 'MusicBrainz' })).toBeVisible();

  const apiKey = screen.getByLabelText('TheAudioDB API key');
  await user.type(apiKey, 'private-audio-key');
  await user.click(screen.getByRole('button', { name: 'Test TheAudioDB connection' }));
  expect(testTheAudioDbMock).toHaveBeenCalledWith({ apiKey: 'private-audio-key' });
  await user.click(screen.getByRole('button', { name: 'Save TheAudioDB settings' }));
  await waitFor(() => {
    expect(saveTheAudioDbMock).toHaveBeenCalledWith({
      enabled: true,
      apiKey: 'private-audio-key',
      revision: 4,
    });
  });
  expect(apiKey).toHaveValue('');

  const userAgent = screen.getByLabelText('MusicBrainz User-Agent');
  await user.type(userAgent, 'TJXY/1.0 (admin@example.invalid)');
  await user.click(screen.getByRole('button', { name: 'Test MusicBrainz connection' }));
  expect(testMusicBrainzMock).toHaveBeenCalledWith({
    userAgent: 'TJXY/1.0 (admin@example.invalid)',
  });
  await user.click(screen.getByRole('button', { name: 'Save MusicBrainz settings' }));
  await waitFor(() => {
    expect(saveMusicBrainzMock).toHaveBeenCalledWith({
      enabled: false,
      userAgent: 'TJXY/1.0 (admin@example.invalid)',
      revision: null,
    });
  });
});
