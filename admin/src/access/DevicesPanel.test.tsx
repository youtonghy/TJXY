import { ThemeProvider } from '@mui/material/styles';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { theme } from '../theme';
import type { DeviceInfo } from './deviceApi';
import { deleteDevice, listDevices, updateDeviceName } from './deviceApi';
import { DevicesPanel } from './DevicesPanel';

const notify = vi.fn();
vi.mock('react-admin', () => ({ useNotify: () => notify }));
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

beforeEach(() => {
  notify.mockReset();
  listMock.mockReset();
  updateMock.mockReset();
  deleteMock.mockReset();
  listMock.mockResolvedValue([device]);
  updateMock.mockResolvedValue(undefined);
  deleteMock.mockResolvedValue(undefined);
});

it('renames a device and refetches the authoritative list', async () => {
  render(<ThemeProvider theme={theme}><DevicesPanel /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByText('Living room');
  await user.click(screen.getByRole('button', { name: 'Edit Living room' }));
  const name = screen.getByRole('textbox', { name: 'Custom device name' });
  await user.clear(name);
  await user.type(name, 'Bedroom');
  await user.click(screen.getByRole('button', { name: 'Save device name' }));

  expect(updateMock).toHaveBeenCalledWith('Phone', 'Bedroom');
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
});

it('confirms revocation by effective name and reloads after success', async () => {
  render(<ThemeProvider theme={theme}><DevicesPanel /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByText('Living room');
  await user.click(screen.getByRole('button', { name: 'Revoke Living room' }));
  expect(screen.getByRole('dialog')).toHaveTextContent('Living room');
  expect(screen.getByRole('dialog')).not.toHaveTextContent('Phone');
  await user.click(screen.getByRole('button', { name: 'Revoke device' }));

  expect(deleteMock).toHaveBeenCalledWith('Phone');
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
});

it('preserves editable state after a failed rename and aborts an unmounted load', async () => {
  updateMock.mockRejectedValue(new Error('failed'));
  let loadSignal: AbortSignal | undefined;
  listMock.mockImplementation((signal) => {
    loadSignal = signal;
    return Promise.resolve([device]);
  });
  const view = render(<ThemeProvider theme={theme}><DevicesPanel /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByText('Living room');
  await user.click(screen.getByRole('button', { name: 'Edit Living room' }));
  const name = screen.getByRole('textbox', { name: 'Custom device name' });
  await user.clear(name);
  await user.type(name, 'Bedroom');
  await user.click(screen.getByRole('button', { name: 'Save device name' }));
  await waitFor(() => { expect(updateMock).toHaveBeenCalled(); });
  expect(name).toHaveValue('Bedroom');
  expect(screen.getByRole('dialog')).toBeVisible();

  view.unmount();
  expect(loadSignal?.aborted).toBe(true);
});

it('disables rename controls while the mutation is pending', async () => {
  let finishRename: (() => void) | undefined;
  updateMock.mockReturnValue(new Promise((resolve) => { finishRename = resolve; }));
  render(<ThemeProvider theme={theme}><DevicesPanel /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByText('Living room');
  await user.click(screen.getByRole('button', { name: 'Edit Living room' }));
  await user.click(screen.getByRole('button', { name: 'Save device name' }));
  expect(screen.getByRole('button', { name: 'Save device name' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
  finishRename?.();
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
});

it('aborts the current reload on unmount and disables stale-row actions while loading', async () => {
  let reloadSignal: AbortSignal | undefined;
  let finishReload: ((records: DeviceInfo[]) => void) | undefined;
  listMock
    .mockResolvedValueOnce([device])
    .mockImplementationOnce((signal) => {
      reloadSignal = signal;
      return new Promise((resolve) => { finishReload = resolve; });
    });
  const view = render(<ThemeProvider theme={theme}><DevicesPanel /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByText('Living room');
  await user.click(screen.getByRole('button', { name: 'Reload devices' }));
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  expect(screen.getByRole('button', { name: 'Edit Living room' })).toBeDisabled();
  view.unmount();
  expect(reloadSignal?.aborted).toBe(true);
  finishReload?.([device]);
});

it('uses a fixed-width table and permits long device fields to wrap', async () => {
  const longName = 'device'.repeat(60);
  listMock.mockResolvedValue([{ ...device, customName: longName }]);
  render(<ThemeProvider theme={theme}><DevicesPanel /></ThemeProvider>);

  const name = await screen.findByText(longName);
  expect(screen.getByRole('table', { name: 'Devices' })).toHaveStyle({ tableLayout: 'fixed' });
  expect(name).toHaveStyle({ overflowWrap: 'anywhere' });
});

it('keeps the revoke dialog open and reports a nonsecret error after failure', async () => {
  deleteMock.mockRejectedValue(new Error('internal-device-id'));
  render(<ThemeProvider theme={theme}><DevicesPanel /></ThemeProvider>);
  const user = userEvent.setup();

  await screen.findByText('Living room');
  await user.click(screen.getByRole('button', { name: 'Revoke Living room' }));
  await user.click(screen.getByRole('button', { name: 'Revoke device' }));
  await waitFor(() => { expect(deleteMock).toHaveBeenCalled(); });
  expect(screen.getByRole('dialog')).toBeVisible();
  expect(notify).toHaveBeenCalledWith('Device access could not be revoked.', { type: 'error' });
  expect(screen.queryByText('internal-device-id')).not.toBeInTheDocument();
});
