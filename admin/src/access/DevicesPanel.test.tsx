import { Toast } from '@heroui/react';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { defaultTestAuthProvider, renderWithAdmin } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
import type { DeviceInfo } from './deviceApi';
import { deleteDevice, listDevices, updateDeviceName } from './deviceApi';
import { DevicesPanel } from './DevicesPanel';

vi.mock('./deviceApi', () => ({
  deleteDevice: vi.fn(),
  listDevices: vi.fn(),
  updateDeviceName: vi.fn(),
}));

const listMock = vi.mocked(listDevices);
const updateMock = vi.mocked(updateDeviceName);
const deleteMock = vi.mocked(deleteDevice);
const device = {
  name: 'Pixel',
  customName: 'Living room',
  id: 'Phone',
  lastUserName: 'Admin',
  appName: 'Findroid',
  appVersion: '0.16.0',
  lastUserId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f31',
  dateLastActivity: '2026-07-26T12:00:00Z',
  capabilities: {
    playableMediaTypes: ['Video'],
    supportedCommands: ['Play'],
    supportsMediaControl: true,
    supportsPersistentIdentifier: true,
    deviceProfile: null,
    appStoreUrl: null,
    iconUrl: null,
  },
  iconUrl: null,
} satisfies DeviceInfo;

function renderDevices(authProvider = defaultTestAuthProvider) {
  return renderWithAdmin(
    <>
      <DevicesPanel />
      <AdminNotifications />
    </>,
    { authProvider, initialEntries: ['/admin/access'] },
  );
}

async function devicesGrid() {
  return await screen.findByRole('grid', { name: 'Devices' });
}

beforeEach(() => {
  listMock.mockReset();
  updateMock.mockReset();
  deleteMock.mockReset();
  listMock.mockResolvedValue([device]);
  updateMock.mockResolvedValue(undefined);
  deleteMock.mockResolvedValue(undefined);
});

afterEach(() => { vi.restoreAllMocks(); });

it('renames a device, supports clearing the custom name, and refetches authoritative data', async () => {
  renderDevices();
  const user = userEvent.setup();
  const grid = await devicesGrid();

  await user.click(within(grid).getByRole('button', { name: 'Edit Living room' }));
  const name = screen.getByRole('textbox', { name: 'Custom device name' });
  await user.clear(name);
  await user.type(name, 'Bedroom');
  await user.click(screen.getByRole('button', { name: 'Save device name' }));

  expect(updateMock).toHaveBeenCalledWith('Phone', 'Bedroom');
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });

  await user.click(within(grid).getByRole('button', { name: 'Edit Living room' }));
  await user.clear(screen.getByRole('textbox', { name: 'Custom device name' }));
  await user.click(screen.getByRole('button', { name: 'Save device name' }));
  expect(updateMock).toHaveBeenLastCalledWith('Phone', null);
});

it('confirms revocation by effective name and reloads after success', async () => {
  listMock.mockResolvedValueOnce([device]).mockResolvedValueOnce([]);
  renderDevices();
  const user = userEvent.setup();
  const grid = await devicesGrid();

  await user.click(within(grid).getByRole('button', { name: 'Revoke Living room' }));
  const dialog = screen.getByRole('dialog', { name: 'Revoke device' });
  expect(dialog).toHaveTextContent('Living room');
  expect(dialog).not.toHaveTextContent('Phone');
  await user.click(within(dialog).getByRole('button', { name: 'Revoke device' }));

  expect(deleteMock).toHaveBeenCalledWith('Phone');
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  await waitFor(() => {
    expect(screen.getByRole('heading', { name: 'Devices' })).toHaveFocus();
  });
});

it('preserves the rename draft and reports only safe copy after failure', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('rename-error');
  updateMock.mockRejectedValue(new Error('private-device-detail'));
  renderDevices();
  const user = userEvent.setup();
  const grid = await devicesGrid();

  await user.click(within(grid).getByRole('button', { name: 'Edit Living room' }));
  const name = screen.getByRole('textbox', { name: 'Custom device name' });
  await user.clear(name);
  await user.type(name, 'Bedroom');
  await user.click(screen.getByRole('button', { name: 'Save device name' }));

  await waitFor(() => {
    expect(dangerToast).toHaveBeenCalledWith(
      'The device name could not be saved.',
      expect.any(Object),
    );
  });
  expect(name).toHaveValue('Bedroom');
  expect(screen.getByRole('dialog', { name: 'Edit device name' })).toBeVisible();
  expect(screen.queryByText('private-device-detail')).not.toBeInTheDocument();
});

it('locks rename controls while the mutation is pending', async () => {
  let finishRename: (() => void) | undefined;
  updateMock.mockReturnValue(new Promise((resolve) => { finishRename = resolve; }));
  renderDevices();
  const user = userEvent.setup();
  const grid = await devicesGrid();

  await user.click(within(grid).getByRole('button', { name: 'Edit Living room' }));
  await user.click(screen.getByRole('button', { name: 'Save device name' }));
  expect(screen.getByRole('button', { name: 'Save device name' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();

  finishRename?.();
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
});

it('aborts a reload on unmount and disables stale record actions while refreshing', async () => {
  let reloadSignal: AbortSignal | undefined;
  listMock
    .mockResolvedValueOnce([device])
    .mockImplementationOnce((signal) => {
      reloadSignal = signal;
      return new Promise(() => undefined);
    });
  const view = renderDevices();
  const user = userEvent.setup();
  const grid = await devicesGrid();

  await user.click(screen.getByRole('button', { name: 'Reload devices' }));
  expect(await screen.findByRole('status')).toHaveTextContent('Refreshing devices');
  expect(within(grid).getByRole('button', { name: 'Edit Living room' })).toBeDisabled();
  view.unmount();
  expect(reloadSignal?.aborted).toBe(true);
});

it('retains device records and uses an inline safe error after a failed refresh', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  listMock
    .mockResolvedValueOnce([device])
    .mockRejectedValueOnce(new Error('private-refresh-detail'));
  renderDevices();
  const user = userEvent.setup();
  const grid = await devicesGrid();

  await user.click(screen.getByRole('button', { name: 'Reload devices' }));

  expect(await screen.findByText('Showing the last available data')).toBeVisible();
  expect(within(grid).getByText('Living room')).toBeVisible();
  expect(screen.queryByText('private-refresh-detail')).not.toBeInTheDocument();
  expect(dangerToast).not.toHaveBeenCalled();
});

it('renders fixed HeroUI desktop data and a complete mobile record', async () => {
  const longName = 'device'.repeat(60);
  listMock.mockResolvedValue([{ ...device, customName: longName }]);
  renderDevices();

  const grid = await devicesGrid();
  expect(grid).toHaveClass('table-fixed');
  expect(within(grid).getByText(longName)).toHaveClass('break-words');
  const mobile = screen.getByRole('list', { name: 'Devices mobile' });
  const record = within(mobile).getByRole('listitem', { name: longName });
  expect(record).toHaveTextContent('Application');
  expect(record).toHaveTextContent('Last user');
  expect(record).toHaveTextContent('Last activity');
});

it('keeps revoke confirmation open and never exposes a raw server error', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('revoke-error');
  deleteMock.mockRejectedValue(new Error('internal-device-id'));
  renderDevices();
  const user = userEvent.setup();
  const grid = await devicesGrid();

  await user.click(within(grid).getByRole('button', { name: 'Revoke Living room' }));
  const dialog = screen.getByRole('dialog', { name: 'Revoke device' });
  await user.click(within(dialog).getByRole('button', { name: 'Revoke device' }));

  expect(await within(dialog).findByText('Review the current state and try again.')).toBeVisible();
  expect(dialog).toBeVisible();
  expect(dangerToast).toHaveBeenCalledWith(
    'Device access could not be revoked.',
    expect.any(Object),
  );
  expect(screen.queryByText('internal-device-id')).not.toBeInTheDocument();
});

it('delegates authorization failures without showing a local error or toast', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  const checkError = vi.fn().mockRejectedValue({ logoutUser: false, message: false });
  listMock.mockRejectedValue({ status: 403, message: 'private-auth-detail' });
  renderDevices({ ...defaultTestAuthProvider, checkError });

  await waitFor(() => { expect(checkError).toHaveBeenCalled(); });
  await waitFor(() => {
    expect(screen.queryByRole('status', { name: 'Loading devices' })).not.toBeInTheDocument();
  });
  expect(screen.queryByRole('heading', { name: 'Unable to load this content' })).not.toBeInTheDocument();
  expect(dangerToast).not.toHaveBeenCalled();
  expect(screen.queryByText('private-auth-detail')).not.toBeInTheDocument();
});
