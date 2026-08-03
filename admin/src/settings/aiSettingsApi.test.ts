import { apiRequest } from '../api/httpClient';
import {
  deleteAiSettings,
  discoverAiModels,
  getAiAnalytics,
  getAiSettings,
  saveAiSettings,
  testAiConnection,
} from './aiSettingsApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);
const model = { id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11', upstreamId: 'gpt-media', displayName: 'Cinema Guide', reasoningEffort: 'high' as const, isVisible: true, isDefault: true, sortOrder: 0 };

beforeEach(() => { requestMock.mockReset(); });

it('loads a strict redacted provider contract', async () => {
  requestMock.mockResolvedValue({ Provider: 'OpenAiCompatible', Configured: true, Enabled: true, BaseUrl: 'https://ai.example/v1', SystemPrompt: 'Movies only', Revision: 3, EncryptionAvailable: true, Models: [{ Id: model.id, UpstreamId: model.upstreamId, DisplayName: model.displayName, ReasoningEffort: 'high', IsVisible: true, IsDefault: true, SortOrder: 0 }] });
  await expect(getAiSettings()).resolves.toEqual({ provider: 'OpenAiCompatible', configured: true, enabled: true, baseUrl: 'https://ai.example/v1', systemPrompt: 'Movies only', revision: 3, encryptionAvailable: true, models: [model] });
});

it('loads strict AI usage analytics without message content', async () => {
  requestMock.mockResolvedValue({
    Window: { Today: '2026-08-03', StartsAt: '2026-08-02T14:00:00Z', EndsAt: '2026-08-03T14:00:00Z', TimeZone: 'server-local +10:00' },
    Summary: { TotalRequests: 3, ActiveUsers: 2, SuccessfulRequests: 2, FailedRequests: 1, PromptTokens: null, CompletionTokens: null, TotalTokens: null, KnownTokenRequests: 2 },
    Daily: [{ Day: '2026-08-03', TotalRequests: 3, SuccessfulRequests: 2, FailedRequests: 1, TotalTokens: null }],
    Users: [{ UserId: model.id, Username: 'admin', TotalRequests: 2, SuccessfulRequests: 2, TotalTokens: 140, LastUsedAt: '2026-08-03T01:00:00Z' }],
    Models: [{ ModelId: model.id, DisplayName: 'Cinema Guide', UpstreamModelId: 'gpt-media', TotalRequests: 3, SuccessfulRequests: 2, TotalTokens: null, LastUsedAt: '2026-08-03T01:00:00Z' }],
    RecentFailures: [{ Id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12', UserId: model.id, Username: 'admin', ModelId: model.id, ModelDisplayName: 'Cinema Guide', Outcome: 'upstream_timeout', ElapsedMs: 30000, StartedAt: '2026-08-03T00:30:00Z' }],
  });
  await expect(getAiAnalytics()).resolves.toMatchObject({
    summary: { totalRequests: 3, totalTokens: null, knownTokenRequests: 2 },
    users: [{ username: 'admin', totalTokens: 140 }],
    recentFailures: [{ outcome: 'upstream_timeout' }],
  });
  expect(requestMock).toHaveBeenCalledWith('/Admin/Ai/Analytics', {});
});

it('rejects AI analytics response key drift', async () => {
  requestMock.mockResolvedValue({ Window: {}, Summary: {}, Daily: [], Users: [], Models: [], RecentFailures: [], Prompt: 'leaked' });
  await expect(getAiAnalytics()).rejects.toMatchObject({ category: 'invalid-response' });
});

it('saves only explicit settings fields and keeps the API key write-only', async () => {
  requestMock.mockResolvedValue({ Provider: 'OpenAiCompatible', Configured: true, Enabled: true, BaseUrl: 'https://ai.example/v1', SystemPrompt: 'Movies only\nUse library context', Revision: 4, EncryptionAvailable: true, Models: [{ Id: model.id, UpstreamId: model.upstreamId, DisplayName: model.displayName, ReasoningEffort: 'high', IsVisible: true, IsDefault: true, SortOrder: 0 }] });
  await saveAiSettings({ enabled: true, baseUrl: 'https://ai.example/v1', apiKey: 'draft-secret', systemPrompt: 'Movies only\nUse library context', revision: 3, models: [model] });
  expect(requestMock).toHaveBeenCalledWith('/Admin/Ai/Settings', {
    method: 'PUT',
    body: JSON.stringify({ Enabled: true, BaseUrl: 'https://ai.example/v1', ApiKey: 'draft-secret', SystemPrompt: 'Movies only\nUse library context', Revision: 3, Models: [{ Id: model.id, UpstreamId: 'gpt-media', DisplayName: 'Cinema Guide', ReasoningEffort: 'high', IsVisible: true, IsDefault: true, SortOrder: 0 }] }),
  });
});

it('tests drafts and removes the database override', async () => {
  requestMock.mockResolvedValueOnce({ Status: 'Success' }).mockResolvedValueOnce(undefined);
  await testAiConnection({ baseUrl: 'https://ai.example/v1', apiKey: '', upstreamModel: 'gpt-media' });
  await deleteAiSettings(3);
  expect(requestMock).toHaveBeenNthCalledWith(1, '/Admin/Ai/Settings/Test', { method: 'POST', body: JSON.stringify({ BaseUrl: 'https://ai.example/v1', UpstreamModel: 'gpt-media' }) });
  expect(requestMock).toHaveBeenNthCalledWith(2, '/Admin/Ai/Settings', { method: 'DELETE', body: JSON.stringify({ Revision: 3 }) });
});

it('discovers provider models without sending a blank write-only key', async () => {
  requestMock.mockResolvedValue({ Items: [{ Id: 'alpha-model' }, { Id: 'zeta-model' }] });
  await expect(discoverAiModels({ baseUrl: 'https://ai.example/v1', apiKey: '' })).resolves.toEqual(['alpha-model', 'zeta-model']);
  expect(requestMock).toHaveBeenCalledWith('/Admin/Ai/Settings/Models', {
    method: 'POST',
    body: JSON.stringify({ BaseUrl: 'https://ai.example/v1' }),
  });
});

it('rejects malformed discovered model contracts', async () => {
  requestMock.mockResolvedValue({ Items: [{ Id: 'model-a', Name: 'unexpected' }] });
  await expect(discoverAiModels({ baseUrl: 'https://ai.example/v1' })).rejects.toMatchObject({ category: 'invalid-response' });
});

it('rejects response key drift and secret leakage', async () => {
  requestMock.mockResolvedValue({ Provider: 'OpenAiCompatible', Configured: true, Enabled: true, BaseUrl: 'https://ai.example/v1', SystemPrompt: 'Movies only', Revision: 3, EncryptionAvailable: true, Models: [], ApiKey: 'leaked' });
  await expect(getAiSettings()).rejects.toMatchObject({ category: 'invalid-response' });
});

it('matches the database model length limits before sending', async () => {
  await expect(saveAiSettings({ enabled: true, baseUrl: 'https://ai.example/v1', apiKey: '', systemPrompt: 'Movies only', revision: 3, models: [{ ...model, upstreamId: 'x'.repeat(256) }] })).rejects.toMatchObject({ category: 'validation' });
  expect(requestMock).not.toHaveBeenCalled();
});
