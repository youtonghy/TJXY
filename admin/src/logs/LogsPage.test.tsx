import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderWithAdmin } from '../test/renderWithAdmin';
import { LogsPage } from './LogsPage';
import { getLoggingSettings, getLogPage, listLogFiles, saveLoggingSettings } from './logsApi';

vi.mock('./logsApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('./logsApi')>();
  return { ...original, getLoggingSettings: vi.fn(), getLogPage: vi.fn(), listLogFiles: vi.fn(), saveLoggingSettings: vi.fn() };
});

const getSettingsMock = vi.mocked(getLoggingSettings);
const listMock = vi.mocked(listLogFiles);
const pageMock = vi.mocked(getLogPage);
const saveMock = vi.mocked(saveLoggingSettings);

beforeEach(() => {
  getSettingsMock.mockReset().mockResolvedValue({ mode: 'Error', retentionDays: 30, revision: 1, directory: './data/logs' });
  listMock.mockReset().mockResolvedValue([{ date: '2026-08-13', sizeBytes: 2048, current: true }]);
  pageMock.mockReset().mockResolvedValue({ date: '2026-08-13', lines: ['{"level":"ERROR","message":"scan failed"}'], offset: 0, nextOffset: 48, sizeBytes: 48, hasOlder: false });
  saveMock.mockReset().mockImplementation((settings) => Promise.resolve({ ...settings, revision: 2 }));
});

it('loads daily logs and applies debug mode without a restart', async () => {
  renderWithAdmin(<LogsPage />, { initialEntries: ['/admin/logs'] });
  const user = userEvent.setup();

  expect(await screen.findByText('2026-08-13')).toBeVisible();
  expect(screen.getByText(/scan failed/)).toBeVisible();
  await user.click(screen.getByRole('radio', { name: 'Debug' }));
  await user.click(screen.getByRole('button', { name: 'Save settings' }));

  await waitFor(() => {
    expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({ mode: 'Debug', retentionDays: 30, revision: 1 }));
  });
  expect(screen.getByText('Record complete scanning, ingestion, and Lazy work flows.')).toBeVisible();
});

it('loads older bounded pages before current lines', async () => {
  pageMock.mockResolvedValueOnce({ date: '2026-08-13', lines: ['new'], offset: 256, nextOffset: 512, sizeBytes: 512, hasOlder: true })
    .mockResolvedValueOnce({ date: '2026-08-13', lines: ['old'], offset: 0, nextOffset: 256, sizeBytes: 512, hasOlder: false });
  renderWithAdmin(<LogsPage />, { initialEntries: ['/admin/logs'] });
  const user = userEvent.setup();
  await user.click(await screen.findByRole('button', { name: 'Load older lines' }));
  expect(await screen.findByText('old')).toBeVisible();
  expect(screen.getByText('new')).toBeVisible();
  expect(pageMock).toHaveBeenLastCalledWith('2026-08-13', 256);
});
