import { Toast } from '@heroui/react';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { defaultTestAuthProvider, renderWithAdmin } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
import type { ApiKeyInfo } from './apiKeyApi';
import { createApiKey, deleteApiKey, listApiKeys } from './apiKeyApi';
import { ApiKeysPanel } from './ApiKeysPanel';

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

let writeText: ReturnType<typeof vi.fn>;

function renderApiKeys(authProvider = defaultTestAuthProvider) {
  return renderWithAdmin(
    <>
      <ApiKeysPanel />
      <AdminNotifications />
    </>,
    { authProvider, initialEntries: ['/admin/access?tab=api-keys'] },
  );
}

async function apiKeysGrid() {
  return await screen.findByRole('grid', { name: 'API Keys' });
}

beforeEach(() => {
  listMock.mockReset();
  createMock.mockReset();
  deleteMock.mockReset();
  listMock.mockResolvedValue([apiKey]);
  createMock.mockResolvedValue(undefined);
  deleteMock.mockResolvedValue(undefined);
  writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText },
  });
});

afterEach(() => { vi.restoreAllMocks(); });

it('masks, reveals, hides, copies, and clears reveal state on refetch', async () => {
  renderApiKeys();
  const user = userEvent.setup();
  writeText = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue(undefined);
  const grid = await apiKeysGrid();

  expect(within(grid).queryByText(rawToken)).not.toBeInTheDocument();
  await user.click(within(grid).getByRole('button', { name: 'Show key for Kodi Sync' }));
  expect(within(grid).getByText(rawToken)).toBeVisible();
  await user.click(within(grid).getByRole('button', { name: 'Copy key for Kodi Sync' }));
  expect(writeText).toHaveBeenCalledWith(rawToken);
  await user.click(within(grid).getByRole('button', { name: 'Hide key for Kodi Sync' }));
  expect(within(grid).queryByText(rawToken)).not.toBeInTheDocument();

  await user.click(within(grid).getByRole('button', { name: 'Show key for Kodi Sync' }));
  await user.click(screen.getByRole('button', { name: 'Reload API keys' }));
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
  expect(window.location.href).not.toContain(rawToken);
  for (const storage of [sessionStorage, localStorage]) {
    for (let index = 0; index < storage.length; index += 1) {
      expect(storage.getItem(storage.key(index) ?? '')).not.toContain(rawToken);
    }
  }
});

it('creates and deletes by app name confirmation, then refetches', async () => {
  listMock
    .mockResolvedValueOnce([apiKey])
    .mockResolvedValueOnce([apiKey])
    .mockResolvedValueOnce([]);
  renderApiKeys();
  const user = userEvent.setup();
  const grid = await apiKeysGrid();

  await user.click(screen.getByRole('button', { name: 'Create API key' }));
  await user.type(screen.getByRole('textbox', { name: 'Application name' }), 'Automation');
  await user.click(screen.getByRole('button', { name: 'Create key' }));
  expect(createMock).toHaveBeenCalledWith('Automation');
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  await waitFor(() => {
    expect(screen.queryByRole('dialog', { name: 'Create API key' })).not.toBeInTheDocument();
  });

  await user.click(within(grid).getByRole('button', { name: 'Delete key for Kodi Sync' }));
  const dialog = screen.getByRole('dialog', { name: 'Delete API key' });
  expect(dialog).toHaveTextContent('Kodi Sync');
  expect(dialog).not.toHaveTextContent(rawToken);
  await user.click(within(dialog).getByRole('button', { name: 'Delete key' }));
  expect(deleteMock).toHaveBeenCalledWith(rawToken);
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(3); });
  await waitFor(() => {
    expect(screen.getByRole('heading', { name: 'API Keys' })).toHaveFocus();
  });
});

it('does not render a secret from failed operations and resets it after unmount', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('delete-error');
  deleteMock.mockRejectedValue(new Error(rawToken));
  const view = renderApiKeys();
  const user = userEvent.setup();
  let grid = await apiKeysGrid();

  await user.click(within(grid).getByRole('button', { name: 'Show key for Kodi Sync' }));
  expect(within(grid).getByText(rawToken)).toBeVisible();
  view.unmount();

  renderApiKeys();
  grid = await apiKeysGrid();
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
  await user.click(within(grid).getByRole('button', { name: 'Delete key for Kodi Sync' }));
  const dialog = screen.getByRole('dialog', { name: 'Delete API key' });
  await user.click(within(dialog).getByRole('button', { name: 'Delete key' }));
  expect(await within(dialog).findByText('Review the current state and try again.')).toBeVisible();
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
  expect(JSON.stringify(dangerToast.mock.calls)).not.toContain(rawToken);
});

it('locks create controls while the mutation is pending', async () => {
  let finishCreate: (() => void) | undefined;
  createMock.mockReturnValue(new Promise((resolve) => { finishCreate = resolve; }));
  renderApiKeys();
  const user = userEvent.setup();

  await apiKeysGrid();
  await user.click(screen.getByRole('button', { name: 'Create API key' }));
  await user.type(screen.getByRole('textbox', { name: 'Application name' }), 'Automation');
  await user.click(screen.getByRole('button', { name: 'Create key' }));
  expect(screen.getByRole('button', { name: 'Create key' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();

  finishCreate?.();
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
});

it('disables creation during loading and aborts a current reload on unmount', async () => {
  let finishInitial: ((records: ApiKeyInfo[]) => void) | undefined;
  let reloadSignal: AbortSignal | undefined;
  listMock
    .mockImplementationOnce(() => new Promise((resolve) => { finishInitial = resolve; }))
    .mockImplementationOnce((signal) => {
      reloadSignal = signal;
      return new Promise(() => undefined);
    });
  const view = renderApiKeys();
  const user = userEvent.setup();

  expect(screen.getByRole('button', { name: 'Create API key' })).toBeDisabled();
  finishInitial?.([apiKey]);
  const grid = await apiKeysGrid();
  await user.click(within(grid).getByRole('button', { name: 'Show key for Kodi Sync' }));
  expect(within(grid).getByText(rawToken)).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Reload API keys' }));
  expect(await screen.findByRole('status')).toHaveTextContent('Refreshing API keys');
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
  expect(within(grid).getByRole('button', { name: 'Delete key for Kodi Sync' })).toBeDisabled();
  view.unmount();
  expect(reloadSignal?.aborted).toBe(true);
});

it('renders fixed HeroUI desktop data and a complete mobile record', async () => {
  const longName = 'application'.repeat(40);
  listMock.mockResolvedValue([{ ...apiKey, appName: longName }]);
  renderApiKeys();

  const grid = await apiKeysGrid();
  expect(grid).toHaveClass('table-fixed');
  expect(within(grid).getByText(longName)).toHaveClass('break-words');
  const mobile = screen.getByRole('list', { name: 'API Keys mobile' });
  const record = within(mobile).getByRole('listitem', { name: longName });
  expect(record).toHaveTextContent('Key');
  expect(record).toHaveTextContent('Created');
  expect(record).toHaveTextContent('Last used');
});

it('reports clipboard failure with safe copy and never renders the token', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('copy-error');
  renderApiKeys();
  const user = userEvent.setup();
  writeText = vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValue(new Error(rawToken));
  const grid = await apiKeysGrid();

  await user.click(within(grid).getByRole('button', { name: 'Copy key for Kodi Sync' }));
  await waitFor(() => {
    expect(dangerToast).toHaveBeenCalledWith(
      'The API key could not be copied.',
      expect.any(Object),
    );
  });
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
});

it('delegates authorization failures without showing a local error or toast', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  const checkError = vi.fn().mockRejectedValue({ logoutUser: false, message: false });
  listMock.mockRejectedValue({ status: 401, message: rawToken });
  renderApiKeys({ ...defaultTestAuthProvider, checkError });

  await waitFor(() => { expect(checkError).toHaveBeenCalled(); });
  await waitFor(() => {
    expect(screen.queryByRole('status', { name: 'Loading API keys' })).not.toBeInTheDocument();
  });
  expect(screen.queryByRole('heading', { name: 'Unable to load this content' })).not.toBeInTheDocument();
  expect(dangerToast).not.toHaveBeenCalled();
  expect(screen.queryByText(rawToken)).not.toBeInTheDocument();
});
