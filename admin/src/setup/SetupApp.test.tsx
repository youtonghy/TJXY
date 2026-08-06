import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SetupApp } from './SetupApp';
import type { CinematicSceneFactory } from './cinematicScene';
import { completeSetup, getSetupStatus, recoverSetup, testSetupDatabase, uploadSetupBranding, validateSetupNetwork } from './setupApi';

vi.mock('./setupApi', () => ({
  getSetupStatus: vi.fn(),
  testSetupDatabase: vi.fn(),
  validateSetupNetwork: vi.fn(),
  uploadSetupBranding: vi.fn(),
  completeSetup: vi.fn(),
  recoverSetup: vi.fn(),
  subscribeSetupProgress: vi.fn(() => vi.fn()),
}));

const statusMock = vi.mocked(getSetupStatus);
const databaseMock = vi.mocked(testSetupDatabase);
const networkMock = vi.mocked(validateSetupNetwork);
const brandingMock = vi.mocked(uploadSetupBranding);
const completeMock = vi.mocked(completeSetup);
const recoverMock = vi.mocked(recoverSetup);
const setupSceneFactory = vi.fn<CinematicSceneFactory>(() => ({ dispose: () => undefined, start: () => undefined }));
const renderSetup = () => render(<SetupApp createCinematicScene={setupSceneFactory} />);

beforeEach(() => {
  setupSceneFactory.mockClear();
  window.localStorage.setItem('tjxy-system-locale', 'en-US');
  statusMock.mockReset().mockResolvedValue({
    state: 'unconfigured',
    installationId: '11111111-1111-4111-8111-111111111111',
    csrfToken: 'csrf-token-0123456789abcdef',
    databaseBackends: ['sqlite', 'postgresql', 'mysql'],
    deploymentMode: 'native',
    version: '0.1.0',
    configurationWritable: true,
    sourceEligible: true,
    blockingOverrides: [],
  });
  databaseMock.mockReset().mockResolvedValue({ backend: 'sqlite', version: '3.49.1', elapsedMilliseconds: 8 });
  networkMock.mockReset().mockResolvedValue({
    listenHost: '127.0.0.1',
    port: 8096,
    publicUrl: null,
    destinationUrl: 'http://127.0.0.1:8096/admin/login',
  });
  brandingMock.mockReset().mockResolvedValue(`/Branding/Assets/logo-${'a'.repeat(64)}.png`);
  completeMock.mockReset().mockResolvedValue('http://127.0.0.1:8096/admin/login');
  recoverMock.mockReset().mockResolvedValue('http://127.0.0.1:8096/admin/login');
});

it('shows recovery instead of a new wizard for a pending installation', async () => {
  const user = userEvent.setup();
  statusMock.mockResolvedValueOnce({
    state: 'pending',
    installationId: '11111111-1111-4111-8111-111111111111',
    csrfToken: 'csrf-token-0123456789abcdef',
    databaseBackends: ['sqlite', 'postgresql', 'mysql'],
    deploymentMode: 'native',
    version: '0.1.0',
    configurationWritable: true,
    sourceEligible: true,
    blockingOverrides: [],
  });
  renderSetup();

  expect(await screen.findByRole('heading', { name: 'Resume setup' })).toBeVisible();
  expect(setupSceneFactory).not.toHaveBeenCalled();
  await user.type(screen.getByLabelText('Recovery password'), 'correct horse');
  await user.click(screen.getByRole('button', { name: 'Resume installation' }));

  expect(await screen.findByRole('heading', { name: 'Setup complete' })).toBeVisible();
  expect(recoverMock).toHaveBeenCalledWith(
    'csrf-token-0123456789abcdef',
    'admin',
    'correct horse',
  );
});

it('blocks setup when an environment override would replace the chosen configuration', async () => {
  const user = userEvent.setup();
  statusMock.mockResolvedValueOnce({
    state: 'unconfigured',
    installationId: '11111111-1111-4111-8111-111111111111',
    csrfToken: 'csrf-token-0123456789abcdef',
    databaseBackends: ['sqlite', 'postgresql', 'mysql'],
    deploymentMode: 'native',
    version: '0.1.0',
    configurationWritable: true,
    sourceEligible: true,
    blockingOverrides: ['TJXY_DATABASE_URL'],
  });
  renderSetup();

  await user.click(await screen.findByRole('button', { name: 'Skip intro' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('TJXY_DATABASE_URL would override setup.');
  expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
});

it('submits again when the user retries a failed installation', async () => {
  const user = userEvent.setup();
  completeMock.mockRejectedValueOnce(new Error('failed'));
  renderSetup();

  await user.click(await screen.findByRole('button', { name: 'Skip intro' }));
  await user.click(await screen.findByRole('button', { name: 'Continue' }));
  await user.click(screen.getByRole('button', { name: 'Continue' }));
  await user.click(screen.getByRole('button', { name: 'Test connection' }));
  await screen.findByText('SQLite 3.49.1 · 8 ms');
  await user.click(screen.getByRole('button', { name: 'Continue' }));
  await user.click(screen.getByRole('button', { name: 'Validate network' }));
  await user.click(screen.getByRole('button', { name: 'Continue' }));
  await user.type(screen.getByLabelText('Password'), 'correct horse');
  await user.type(screen.getByLabelText('Confirm password'), 'correct horse');
  await user.click(screen.getByRole('button', { name: 'Continue' }));
  await user.click(screen.getByRole('button', { name: 'Install TJXY' }));

  expect(await screen.findByRole('alert')).toHaveTextContent('Installation stopped');
  expect(completeMock).toHaveBeenCalledTimes(1);
  await user.click(screen.getByRole('button', { name: 'Retry' }));
  expect(await screen.findByRole('heading', { name: 'Setup complete' })).toBeVisible();
  expect(completeMock).toHaveBeenCalledTimes(2);
});

it('requires a successful network preflight for the unchanged draft', async () => {
  const user = userEvent.setup();
  renderSetup();

  await user.click(await screen.findByRole('button', { name: 'Skip intro' }));
  await user.click(await screen.findByRole('button', { name: 'Continue' }));
  await user.click(screen.getByRole('button', { name: 'Continue' }));
  await user.click(screen.getByRole('button', { name: 'Test connection' }));
  await screen.findByText('SQLite 3.49.1 · 8 ms');
  await user.click(screen.getByRole('button', { name: 'Continue' }));

  expect(screen.getByRole('heading', { name: 'Network' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
  await user.click(screen.getByRole('button', { name: 'Validate network' }));
  expect(await screen.findByText('http://127.0.0.1:8096/admin/login')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled();

  await user.clear(screen.getByLabelText('Port'));
  await user.type(screen.getByLabelText('Port'), '9090');
  expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
});

it('uses the HeroUI setup flow and gates database continuation on a successful test', async () => {
  const user = userEvent.setup();
  renderSetup();

  expect(await screen.findByRole('region', { name: 'TJXY setup introduction' })).toBeVisible();
  expect(screen.getByTestId('setup-cinematic-canvas')).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Skip intro' }));
  expect(await screen.findByRole('heading', { name: 'Before we begin' })).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Continue' }));
  expect(screen.getByRole('heading', { name: 'Basic information' })).toBeVisible();
  expect(screen.getAllByText('Step 1 of 4').some((element) => element.isConnected)).toBe(true);
  expect(document.querySelectorAll('[data-slot="stepper"]')).toHaveLength(2);
  expect(document.querySelectorAll('[data-slot="inline-select"]')).toHaveLength(1);
  const languageSelector = screen.getByRole('button', { name: /Interface language/ });
  await user.click(languageSelector);
  await user.click(await screen.findByRole('option', { name: '中文' }));
  expect(await screen.findByRole('heading', { name: '基础信息' })).toBeVisible();
  expect(document.documentElement).toHaveAttribute('lang', 'zh-CN');
  expect(window.localStorage.getItem('tjxy-system-locale')).toBe('zh-CN');
  await user.click(screen.getByRole('button', { name: /界面语言/ }));
  await user.click(await screen.findByRole('option', { name: 'English' }));
  expect(await screen.findByRole('heading', { name: 'Basic information' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Choose logo' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Choose icon' })).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Continue' }));

  expect(screen.getByRole('heading', { name: 'Connect database' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
  expect(screen.getByRole('radio', { name: 'SQLite' })).toBeChecked();

  const mysqlRadio = screen.getByRole('radio', { name: 'MySQL' });
  const mysqlControl = mysqlRadio.closest('[data-slot="radio"]')?.querySelector('[data-slot="radio-control"]');
  expect(mysqlControl).toBeInstanceOf(HTMLElement);
  await user.click(mysqlControl as HTMLElement);
  expect(mysqlRadio).toBeChecked();
  expect(screen.getByLabelText('Port')).toHaveValue(3306);

  const sqliteRadio = screen.getByRole('radio', { name: 'SQLite' });
  const sqliteControl = sqliteRadio.closest('[data-slot="radio"]')?.querySelector('[data-slot="radio-control"]');
  expect(sqliteControl).toBeInstanceOf(HTMLElement);
  await user.click(sqliteControl as HTMLElement);
  expect(sqliteRadio).toBeChecked();
  expect(screen.getByLabelText('Database file')).toBeVisible();

  await user.click(screen.getByRole('button', { name: 'Test connection' }));
  expect(await screen.findByText('SQLite 3.49.1 · 8 ms')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled();
});
