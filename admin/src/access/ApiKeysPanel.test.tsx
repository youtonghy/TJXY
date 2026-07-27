import { ThemeProvider } from '@mui/material/styles';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { theme } from '../theme';
import type { ApiKeyInfo } from './apiKeyApi';
import { createApiKey, deleteApiKey, listApiKeys } from './apiKeyApi';
import { ApiKeysPanel } from './ApiKeysPanel';

const notify = vi.fn();
vi.mock('react-admin', () => ({ useNotify: () => notify }));
vi.mock('./apiKeyApi', () => ({
  createApiKey: vi.fn(),
  deleteApiKey: vi.fn(),
  listApiKeys: vi.fn(),
}));

const listMock = vi.mocked(listApiKeys);
const createMock = vi.mocked(createApiKey);
const deleteMock = vi.mocked(deleteApiKey);
const rawToken = 'secret-token-that-must-not-persist';
const apiKey = {
  id: 7,
  accessToken: rawToken,
  deviceId: null,
  appName: 'Kodi Sync',
  appVersion: null,
  deviceName: null,
  userId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f31',
  isActive: true,
  dateCreated: '2026-07-26T12:00:00Z',
  dateRevoked: null,
  dateLastActivity: null,
  userName: 'Admin',
} satisfies ApiKeyInfo;

beforeEach(() => {
  notify.mockReset();
  listMock.mockReset();
  createMock.mockReset();
  deleteMock.mockReset();
  listMock.mockResolvedValue([apiKey]);
  createMock.mockResolvedValue(undefined);
  deleteMock.mockResolvedValue(undefined);
});

it('masks, reveals, hides, copies, and clears reveal state on refetch', async () => {
  render(<ThemeProvider theme={theme}><ApiKeysPanel /></ThemeProvider>);
  const user = userEvent.setup();
  const writeText = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue();

  await screen.findByText('Kodi Sync');
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Show key for Kodi Sync' }));
  expect(screen.getByText(rawToken)).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Copy key for Kodi Sync' }));
  expect(writeText).toHaveBeenCalledWith(rawToken);
  await user.click(screen.getByRole('button', { name: 'Hide key for Kodi Sync' }));
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Show key for Kodi Sync' }));
  await user.click(screen.getByRole('button', { name: 'Reload API keys' }));
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
  for (const storage of [sessionStorage, localStorage]) {
    for (let index = 0; index < storage.length; index += 1) {
      expect(storage.getItem(storage.key(index) ?? '')).not.toContain(rawToken);
    }
  }
});

it('creates and deletes by app name confirmation, then refetches', async () => {
  render(<ThemeProvider theme={theme}><ApiKeysPanel /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByText('Kodi Sync');
  await user.click(screen.getByRole('button', { name: 'Create API key' }));
  await user.type(screen.getByRole('textbox', { name: 'Application name' }), 'Automation');
  await user.click(screen.getByRole('button', { name: 'Create key' }));
  expect(createMock).toHaveBeenCalledWith('Automation');
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  await waitFor(() => {
    expect(screen.queryByRole('dialog', { name: 'Create API key' })).not.toBeInTheDocument();
  });

  await user.click(screen.getByRole('button', { name: 'Delete key for Kodi Sync' }));
  expect(screen.getByRole('dialog')).toHaveTextContent('Kodi Sync');
  expect(screen.getByRole('dialog')).not.toHaveTextContent(rawToken);
  await user.click(screen.getByRole('button', { name: 'Delete key' }));
  expect(deleteMock).toHaveBeenCalledWith(rawToken);
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(3); });
});

it('does not render a secret from failed operations and resets it after unmount', async () => {
  deleteMock.mockRejectedValue(new Error(rawToken));
  const view = render(<ThemeProvider theme={theme}><ApiKeysPanel /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByText('Kodi Sync');
  await user.click(screen.getByRole('button', { name: 'Show key for Kodi Sync' }));
  expect(screen.getByText(rawToken)).toBeVisible();
  view.unmount();

  render(<ThemeProvider theme={theme}><ApiKeysPanel /></ThemeProvider>);
  await screen.findByText('Kodi Sync');
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Delete key for Kodi Sync' }));
  await user.click(screen.getByRole('button', { name: 'Delete key' }));
  await waitFor(() => { expect(deleteMock).toHaveBeenCalled(); });
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
});

it('disables create controls while the mutation is pending', async () => {
  let finishCreate: (() => void) | undefined;
  createMock.mockReturnValue(new Promise((resolve) => { finishCreate = resolve; }));
  render(<ThemeProvider theme={theme}><ApiKeysPanel /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByText('Kodi Sync');
  await user.click(screen.getByRole('button', { name: 'Create API key' }));
  await user.type(screen.getByRole('textbox', { name: 'Application name' }), 'Automation');
  await user.click(screen.getByRole('button', { name: 'Create key' }));
  expect(screen.getByRole('button', { name: 'Create key' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
  finishCreate?.();
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
});

it('disables creation during loading and aborts the current reload on unmount', async () => {
  let finishInitial: ((records: ApiKeyInfo[]) => void) | undefined;
  let reloadSignal: AbortSignal | undefined;
  listMock
    .mockImplementationOnce(() => new Promise((resolve) => { finishInitial = resolve; }))
    .mockImplementationOnce((signal) => {
      reloadSignal = signal;
      return new Promise(() => undefined);
    });
  const view = render(<ThemeProvider theme={theme}><ApiKeysPanel /></ThemeProvider>);
  const user = userEvent.setup();

  expect(screen.getByRole('button', { name: 'Create API key' })).toBeDisabled();
  finishInitial?.([apiKey]);
  await screen.findByText('Kodi Sync');
  await user.click(screen.getByRole('button', { name: 'Reload API keys' }));
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  expect(screen.getByRole('button', { name: 'Delete key for Kodi Sync' })).toBeDisabled();
  view.unmount();
  expect(reloadSignal?.aborted).toBe(true);
});

it('uses a fixed-width table and permits long application names to wrap', async () => {
  const longName = 'application'.repeat(40);
  listMock.mockResolvedValue([{ ...apiKey, appName: longName }]);
  render(<ThemeProvider theme={theme}><ApiKeysPanel /></ThemeProvider>);

  const name = await screen.findByText(longName);
  expect(screen.getByRole('table', { name: 'API Keys' })).toHaveStyle({ tableLayout: 'fixed' });
  expect(name).toHaveStyle({ overflowWrap: 'anywhere' });
});

it('reports clipboard failure without rendering the token', async () => {
  vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValue(new Error(rawToken));
  render(<ThemeProvider theme={theme}><ApiKeysPanel /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByText('Kodi Sync');
  await user.click(screen.getByRole('button', { name: 'Copy key for Kodi Sync' }));
  await waitFor(() => {
    expect(notify).toHaveBeenCalledWith('The API key could not be copied.', { type: 'error' });
  });
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
});
