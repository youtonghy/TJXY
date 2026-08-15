import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderWithAdmin } from '../test/renderWithAdmin';
import { AiSettingsPage } from './AiSettingsPage';
import { deleteAiSettings, discoverAiModels, getAiAnalytics, getAiSettings, saveAiSettings, testAiConnection } from './aiSettingsApi';

vi.mock('./aiSettingsApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('./aiSettingsApi')>();
  return { ...original, deleteAiSettings: vi.fn(), discoverAiModels: vi.fn(), getAiAnalytics: vi.fn(), getAiSettings: vi.fn(), saveAiSettings: vi.fn(), testAiConnection: vi.fn() };
});

const getMock = vi.mocked(getAiSettings);
const analyticsMock = vi.mocked(getAiAnalytics);
const deleteMock = vi.mocked(deleteAiSettings);
const discoverMock = vi.mocked(discoverAiModels);
const saveMock = vi.mocked(saveAiSettings);
const testMock = vi.mocked(testAiConnection);
const settings = {
  provider: 'OpenAiCompatible' as const, configured: true, enabled: true,
  baseUrl: 'https://ai.example/v1', systemPrompt: 'Movies only', revision: 2,
  dailyTotalTokenLimit: 500_000, dailyUserTokenLimit: 50_000,
  encryptionAvailable: true,
  models: [{ id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11', upstreamId: 'gpt-media', displayName: 'Cinema Guide', reasoningEffort: 'off' as const, isVisible: true, isDefault: true, sortOrder: 0 }],
};
const analytics = {
  window: { today: '2026-08-03', startsAt: '2026-08-02T14:00:00Z', endsAt: '2026-08-03T14:00:00Z', timeZone: 'server-local +10:00' },
  summary: { totalRequests: 0, activeUsers: 0, successfulRequests: 0, failedRequests: 0, promptTokens: 0, completionTokens: 0, totalTokens: 0, knownTokenRequests: 0 },
  daily: [], users: [], models: [], recentFailures: [],
};

beforeEach(() => {
  getMock.mockReset(); analyticsMock.mockReset(); deleteMock.mockReset(); discoverMock.mockReset(); saveMock.mockReset(); testMock.mockReset();
  getMock.mockResolvedValue(settings); analyticsMock.mockResolvedValue(analytics); deleteMock.mockResolvedValue(undefined); discoverMock.mockResolvedValue(['gpt-cinema', 'gpt-media']); saveMock.mockResolvedValue({ ...settings, revision: 3 }); testMock.mockResolvedValue(undefined);
});

it('loads redacted settings and exposes the configured model controls', async () => {
  renderWithAdmin(<AiSettingsPage />, { initialEntries: ['/admin/settings/ai'], strict: true });
  expect(await screen.findByRole('heading', { name: 'AI assistant' })).toBeVisible();
  expect(await screen.findByRole('heading', { name: 'AI 运行统计' })).toBeVisible();
  expect(screen.getByLabelText('API key')).toHaveValue('');
  expect(screen.getByLabelText('API key')).toHaveAttribute('type', 'password');
  expect(screen.getByRole('textbox', { name: 'Daily total limit' })).toHaveValue('500,000');
  expect(screen.getByRole('textbox', { name: 'Daily limit per user' })).toHaveValue('50,000');
  expect(screen.getByDisplayValue('Cinema Guide')).toBeVisible();
  expect(screen.getByDisplayValue('gpt-media')).toBeVisible();
  expect(screen.getByLabelText('思考强度')).toHaveTextContent('off');
  expect(screen.getByRole('checkbox', { name: '前端可见' })).toBeChecked();
  expect(screen.getByRole('checkbox', { name: '前端可见' })).toBeDisabled();
  expect(screen.getByRole('radio', { name: '默认模型' })).toBeChecked();
  expect(screen.getByRole('button', { name: 'Remove Cinema Guide' })).toBeDisabled();
});

it('saves the reasoning effort selected for a model', async () => {
  renderWithAdmin(<AiSettingsPage />, { initialEntries: ['/admin/settings/ai'], strict: true });
  const user = userEvent.setup();
  const effort = await screen.findByLabelText('思考强度');
  await user.click(effort);
  await user.click(screen.getByRole('option', { name: 'xhigh' }));
  await user.click(screen.getByRole('button', { name: 'Save settings' }));
  await waitFor(() => {
    expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({
      models: [expect.objectContaining({ reasoningEffort: 'xhigh' })],
    }));
  });
});

it('saves daily total and per-user token limits', async () => {
  renderWithAdmin(<AiSettingsPage />, { initialEntries: ['/admin/settings/ai'], strict: true });
  const user = userEvent.setup();
  const total = await screen.findByRole('textbox', { name: 'Daily total limit' });
  const perUser = screen.getByRole('textbox', { name: 'Daily limit per user' });
  await user.clear(total); await user.type(total, '750000');
  await user.clear(perUser); await user.type(perUser, '75000');
  await user.click(screen.getByRole('button', { name: 'Save settings' }));
  await waitFor(() => {
    expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({ dailyTotalTokenLimit: 750_000, dailyUserTokenLimit: 75_000 }));
  });
});

it('makes a selected default model visible before saving', async () => {
  const secondModel = { id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12', upstreamId: 'gpt-cinema', displayName: 'Film Expert', reasoningEffort: 'medium' as const, isVisible: false, isDefault: false, sortOrder: 1 };
  getMock.mockResolvedValueOnce({ ...settings, models: [...settings.models, secondModel] });
  renderWithAdmin(<AiSettingsPage />, { initialEntries: ['/admin/settings/ai'], strict: true });
  const user = userEvent.setup();
  const radios = await screen.findAllByRole('radio', { name: '默认模型' });
  const secondRadio = radios[1];
  if (secondRadio === undefined) throw new Error('Expected a second default-model control');
  await user.click(secondRadio);

  const visibilityControls = screen.getAllByRole('checkbox', { name: '前端可见' });
  expect(visibilityControls[1]).toBeChecked();
  expect(visibilityControls[1]).toBeDisabled();
  await user.click(screen.getByRole('button', { name: 'Save settings' }));
  await waitFor(() => {
    expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({
      models: [
        expect.objectContaining({ id: settings.models[0]?.id, isDefault: false, isVisible: true }),
        expect.objectContaining({ id: secondModel.id, isDefault: true, isVisible: true }),
      ],
    }));
  });
});

it('tests and saves a write-only draft while preserving the revision fence', async () => {
  renderWithAdmin(<AiSettingsPage />, { initialEntries: ['/admin/settings/ai'], strict: true });
  const user = userEvent.setup();
  const key = await screen.findByLabelText('API key');
  await user.type(key, 'draft-secret');
  await user.click(screen.getByRole('button', { name: 'Test connection' }));
  expect(testMock).toHaveBeenCalledWith({ baseUrl: 'https://ai.example/v1', apiKey: 'draft-secret', upstreamModel: 'gpt-media' });
  await user.click(screen.getByRole('button', { name: 'Save settings' }));
  await waitFor(() => { expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({ apiKey: 'draft-secret', revision: 2, models: settings.models })); });
  expect(key).toHaveValue('');
});

it('fetches provider models for selection while keeping manual entry editable', async () => {
  renderWithAdmin(<AiSettingsPage />, { initialEntries: ['/admin/settings/ai'], strict: true });
  const user = userEvent.setup();
  const input = await screen.findByDisplayValue('gpt-media');
  await user.click(screen.getByRole('button', { name: /^Fetch available models for Cinema Guide/u }));
  await waitFor(() => { expect(discoverMock).toHaveBeenCalledWith({ baseUrl: 'https://ai.example/v1', apiKey: '' }); });
  await user.click(await screen.findByRole('option', { name: 'gpt-cinema' }));
  expect(input).toHaveValue('gpt-cinema');
  await user.clear(input);
  await user.type(input, 'manual-model');
  expect(input).toHaveValue('manual-model');
});

it('disables persistence when credential encryption is unavailable', async () => {
  getMock.mockResolvedValueOnce({ ...settings, encryptionAvailable: false });
  renderWithAdmin(<AiSettingsPage />, { initialEntries: ['/admin/settings/ai'], strict: true });
  expect(await screen.findByText('Credential encryption is unavailable')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Save settings' })).toBeDisabled();
});

it('allows revision-fenced removal without encryption and requires confirmation', async () => {
  getMock.mockResolvedValueOnce({ ...settings, encryptionAvailable: false });
  renderWithAdmin(<AiSettingsPage />, { initialEntries: ['/admin/settings/ai'], strict: true });
  const user = userEvent.setup();
  const trigger = await screen.findByRole('button', { name: 'Remove settings' });
  expect(trigger).toBeEnabled();
  await user.click(trigger);
  expect(await screen.findByRole('heading', { name: 'Remove AI assistant settings?' })).toBeVisible();
  const buttons = screen.getAllByRole('button', { name: 'Remove settings' });
  const confirm = buttons.at(-1);
  expect(confirm).toBeDefined();
  if (confirm) await user.click(confirm);
  await waitFor(() => { expect(deleteMock).toHaveBeenCalledWith(2); });
});
