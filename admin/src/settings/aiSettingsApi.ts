import { ApiError, apiRequest } from '../api/httpClient';
import { hasControlCharacters, invalidResponse, isRecord, validDate, validMultilineText, validText, validUuid } from '../api/responseValidation';

const PATH = '/Admin/Ai/Settings';
export const MAX_AI_DAILY_TOKEN_LIMIT = 100_000_000;
const reasoningEfforts = ['off', 'low', 'medium', 'high', 'xhigh', 'max'] as const;
export type AiReasoningEffort = typeof reasoningEfforts[number];

export interface AiAdminModel {
  id: string;
  upstreamId: string;
  displayName: string;
  reasoningEffort: AiReasoningEffort;
  isVisible: boolean;
  isDefault: boolean;
  sortOrder: number;
}

export interface AiSettings {
  provider: 'OpenAiCompatible';
  configured: boolean;
  enabled: boolean;
  baseUrl: string | null;
  systemPrompt: string;
  dailyTotalTokenLimit: number;
  dailyUserTokenLimit: number;
  revision: number | null;
  encryptionAvailable: boolean;
  models: AiAdminModel[];
}

export type AiExecutionOutcome = 'upstream_rejected' | 'upstream_invalid' | 'upstream_timeout' | 'tool_failed' | 'persistence_failed' | 'internal_error';

export interface AiAnalytics {
  window: { today: string; startsAt: string; endsAt: string; timeZone: string };
  summary: AiUsageSummary;
  daily: AiUsageDaily[];
  users: AiUsageUser[];
  models: AiUsageModel[];
  recentFailures: AiUsageFailure[];
}

export interface AiUsageSummary {
  totalRequests: number;
  activeUsers: number;
  successfulRequests: number;
  failedRequests: number;
  promptTokens: number | null;
  completionTokens: number | null;
  totalTokens: number | null;
  knownTokenRequests: number;
}

export interface AiUsageDaily {
  day: string;
  totalRequests: number;
  successfulRequests: number;
  failedRequests: number;
  totalTokens: number | null;
}

export interface AiUsageUser {
  userId: string;
  username: string;
  totalRequests: number;
  successfulRequests: number;
  totalTokens: number | null;
  lastUsedAt: string;
}

export interface AiUsageModel {
  modelId: string;
  displayName: string;
  upstreamModelId: string;
  totalRequests: number;
  successfulRequests: number;
  totalTokens: number | null;
  lastUsedAt: string;
}

export interface AiUsageFailure {
  id: string;
  userId: string;
  username: string;
  modelId: string;
  modelDisplayName: string;
  outcome: AiExecutionOutcome;
  elapsedMs: number;
  startedAt: string;
}

export interface SaveAiSettingsRequest extends Omit<AiSettings, 'provider' | 'configured' | 'encryptionAvailable'> {
  apiKey: string;
}

export interface TestAiConnectionRequest {
  baseUrl?: string;
  apiKey?: string;
  upstreamModel?: string;
}

export interface DiscoverAiModelsRequest {
  baseUrl?: string;
  apiKey?: string;
}

export async function getAiSettings(signal?: AbortSignal): Promise<AiSettings> {
  return toSettings(await apiRequest<unknown>(PATH, signal === undefined ? {} : { signal }));
}

export async function getAiAnalytics(signal?: AbortSignal): Promise<AiAnalytics> {
  const value = await apiRequest<unknown>('/Admin/Ai/Analytics', signal === undefined ? {} : { signal });
  if (!isRecord(value) || !exactKeys(value, ['Window', 'Summary', 'Daily', 'Users', 'Models', 'RecentFailures']) || !isRecord(value.Window) || !isRecord(value.Summary) || !Array.isArray(value.Daily) || !Array.isArray(value.Users) || !Array.isArray(value.Models) || !Array.isArray(value.RecentFailures) || value.Daily.length > 31 || value.Users.length > 100 || value.Models.length > 100 || value.RecentFailures.length > 100) throw invalidResponse('AI analytics');
  return {
    window: toAnalyticsWindow(value.Window),
    summary: toUsageSummary(value.Summary),
    daily: value.Daily.map(toUsageDaily),
    users: value.Users.map(toUsageUser),
    models: value.Models.map(toUsageModel),
    recentFailures: value.RecentFailures.map(toUsageFailure),
  };
}

export async function saveAiSettings(request: SaveAiSettingsRequest): Promise<AiSettings> {
  const body: Record<string, unknown> = {
    Enabled: request.enabled,
    BaseUrl: requireUrl(request.baseUrl),
  };
  const apiKey = optionalApiKey(request.apiKey);
  if (apiKey !== undefined) body.ApiKey = apiKey;
  body.SystemPrompt = requireMultilineText(request.systemPrompt, 16_000, 'system prompt');
  body.DailyTotalTokenLimit = requireTokenLimit(request.dailyTotalTokenLimit, 'daily total token limit');
  body.DailyUserTokenLimit = requireTokenLimit(request.dailyUserTokenLimit, 'daily user token limit');
  if (request.revision !== null) body.Revision = requireRevision(request.revision);
  body.Models = request.models.map((model, index) => toRequestModel(model, index));
  return toSettings(await apiRequest<unknown>(PATH, { method: 'PUT', body: JSON.stringify(body) }));
}

export async function testAiConnection(request: TestAiConnectionRequest): Promise<void> {
  const body: Record<string, string> = {};
  if (request.baseUrl !== undefined) body.BaseUrl = requireUrl(request.baseUrl);
  const apiKey = optionalApiKey(request.apiKey);
  if (apiKey !== undefined) body.ApiKey = apiKey;
  if (request.upstreamModel !== undefined) body.UpstreamModel = requireText(request.upstreamModel, 512, 'upstream model');
  const value = await apiRequest<unknown>(`${PATH}/Test`, { method: 'POST', body: JSON.stringify(body) });
  if (!isRecord(value) || !exactKeys(value, ['Status']) || value.Status !== 'Success') throw invalidResponse('AI connection-test result');
}

export async function discoverAiModels(request: DiscoverAiModelsRequest): Promise<string[]> {
  const body: Record<string, string> = {};
  if (request.baseUrl !== undefined) body.BaseUrl = requireUrl(request.baseUrl);
  const apiKey = optionalApiKey(request.apiKey);
  if (apiKey !== undefined) body.ApiKey = apiKey;
  const value = await apiRequest<unknown>(`${PATH}/Models`, { method: 'POST', body: JSON.stringify(body) });
  if (!isRecord(value) || !exactKeys(value, ['Items']) || !Array.isArray(value.Items) || value.Items.length > 1_000) throw invalidResponse('AI model-discovery result');
  return value.Items.map((item) => {
    if (!isRecord(item) || !exactKeys(item, ['Id']) || !validText(item.Id, 255)) throw invalidResponse('AI discovered model');
    return item.Id;
  });
}

export async function deleteAiSettings(revision: number): Promise<void> {
  await apiRequest(PATH, { method: 'DELETE', body: JSON.stringify({ Revision: requireRevision(revision) }) });
}

function toSettings(value: unknown): AiSettings {
  if (!isRecord(value) || !exactKeys(value, ['Provider', 'Configured', 'Enabled', 'BaseUrl', 'SystemPrompt', 'DailyTotalTokenLimit', 'DailyUserTokenLimit', 'Revision', 'EncryptionAvailable', 'Models']) || value.Provider !== 'OpenAiCompatible' || typeof value.Configured !== 'boolean' || typeof value.Enabled !== 'boolean' || (value.BaseUrl !== null && !validText(value.BaseUrl, 2_048)) || !validMultilineText(value.SystemPrompt, 16_000) || (value.Revision !== null && !validRevision(value.Revision)) || typeof value.EncryptionAvailable !== 'boolean' || !Array.isArray(value.Models)) throw invalidResponse('AI settings');
  return { provider: value.Provider, configured: value.Configured, enabled: value.Enabled, baseUrl: value.BaseUrl, systemPrompt: value.SystemPrompt, dailyTotalTokenLimit: tokenLimit(value.DailyTotalTokenLimit), dailyUserTokenLimit: tokenLimit(value.DailyUserTokenLimit), revision: value.Revision, encryptionAvailable: value.EncryptionAvailable, models: value.Models.map(toModel) };
}

function toAnalyticsWindow(value: Record<string, unknown>): AiAnalytics['window'] {
  if (!exactKeys(value, ['Today', 'StartsAt', 'EndsAt', 'TimeZone']) || !validDay(value.Today) || !validDate(value.StartsAt) || !validDate(value.EndsAt) || !validText(value.TimeZone, 64)) throw invalidResponse('AI analytics window');
  return { today: value.Today, startsAt: value.StartsAt, endsAt: value.EndsAt, timeZone: value.TimeZone };
}

function toUsageSummary(value: Record<string, unknown>): AiUsageSummary {
  if (!exactKeys(value, ['TotalRequests', 'ActiveUsers', 'SuccessfulRequests', 'FailedRequests', 'PromptTokens', 'CompletionTokens', 'TotalTokens', 'KnownTokenRequests'])) throw invalidResponse('AI analytics summary');
  const result = {
    totalRequests: count(value.TotalRequests), activeUsers: count(value.ActiveUsers), successfulRequests: count(value.SuccessfulRequests), failedRequests: count(value.FailedRequests),
    promptTokens: nullableCount(value.PromptTokens), completionTokens: nullableCount(value.CompletionTokens), totalTokens: nullableCount(value.TotalTokens), knownTokenRequests: count(value.KnownTokenRequests),
  };
  if (result.successfulRequests + result.failedRequests !== result.totalRequests || result.knownTokenRequests > result.totalRequests) throw invalidResponse('AI analytics summary');
  return result;
}

function toUsageDaily(value: unknown): AiUsageDaily {
  if (!isRecord(value) || !exactKeys(value, ['Day', 'TotalRequests', 'SuccessfulRequests', 'FailedRequests', 'TotalTokens']) || !validDay(value.Day)) throw invalidResponse('AI analytics daily row');
  const result = { day: value.Day, totalRequests: count(value.TotalRequests), successfulRequests: count(value.SuccessfulRequests), failedRequests: count(value.FailedRequests), totalTokens: nullableCount(value.TotalTokens) };
  if (result.successfulRequests + result.failedRequests !== result.totalRequests) throw invalidResponse('AI analytics daily row');
  return result;
}

function toUsageUser(value: unknown): AiUsageUser {
  if (!isRecord(value) || !exactKeys(value, ['UserId', 'Username', 'TotalRequests', 'SuccessfulRequests', 'TotalTokens', 'LastUsedAt']) || !validUuid(value.UserId) || !validText(value.Username, 256) || !validDate(value.LastUsedAt)) throw invalidResponse('AI analytics user row');
  const totalRequests = count(value.TotalRequests); const successfulRequests = count(value.SuccessfulRequests);
  if (successfulRequests > totalRequests) throw invalidResponse('AI analytics user row');
  return { userId: value.UserId, username: value.Username, totalRequests, successfulRequests, totalTokens: nullableCount(value.TotalTokens), lastUsedAt: value.LastUsedAt };
}

function toUsageModel(value: unknown): AiUsageModel {
  if (!isRecord(value) || !exactKeys(value, ['ModelId', 'DisplayName', 'UpstreamModelId', 'TotalRequests', 'SuccessfulRequests', 'TotalTokens', 'LastUsedAt']) || !validUuid(value.ModelId) || !validText(value.DisplayName, 128) || !validText(value.UpstreamModelId, 255) || !validDate(value.LastUsedAt)) throw invalidResponse('AI analytics model row');
  const totalRequests = count(value.TotalRequests); const successfulRequests = count(value.SuccessfulRequests);
  if (successfulRequests > totalRequests) throw invalidResponse('AI analytics model row');
  return { modelId: value.ModelId, displayName: value.DisplayName, upstreamModelId: value.UpstreamModelId, totalRequests, successfulRequests, totalTokens: nullableCount(value.TotalTokens), lastUsedAt: value.LastUsedAt };
}

function toUsageFailure(value: unknown): AiUsageFailure {
  if (!isRecord(value) || !exactKeys(value, ['Id', 'UserId', 'Username', 'ModelId', 'ModelDisplayName', 'Outcome', 'ElapsedMs', 'StartedAt']) || !validUuid(value.Id) || !validUuid(value.UserId) || !validText(value.Username, 256) || !validUuid(value.ModelId) || !validText(value.ModelDisplayName, 128) || !isExecutionOutcome(value.Outcome) || !validDate(value.StartedAt)) throw invalidResponse('AI analytics failure row');
  return { id: value.Id, userId: value.UserId, username: value.Username, modelId: value.ModelId, modelDisplayName: value.ModelDisplayName, outcome: value.Outcome, elapsedMs: count(value.ElapsedMs), startedAt: value.StartedAt };
}

function toModel(value: unknown): AiAdminModel {
  if (!isRecord(value) || !exactKeys(value, ['Id', 'UpstreamId', 'DisplayName', 'ReasoningEffort', 'IsVisible', 'IsDefault', 'SortOrder']) || !validUuid(value.Id) || !validText(value.UpstreamId, 255) || !validText(value.DisplayName, 128) || !isReasoningEffort(value.ReasoningEffort) || typeof value.IsVisible !== 'boolean' || typeof value.IsDefault !== 'boolean' || typeof value.SortOrder !== 'number' || !Number.isSafeInteger(value.SortOrder)) throw invalidResponse('AI model');
  return { id: value.Id, upstreamId: value.UpstreamId, displayName: value.DisplayName, reasoningEffort: value.ReasoningEffort, isVisible: value.IsVisible, isDefault: value.IsDefault, sortOrder: value.SortOrder };
}

function toRequestModel(model: AiAdminModel, sortOrder: number): Record<string, unknown> {
  if (!validUuid(model.id)) throw new ApiError(400, 'validation', 'A valid model ID is required.');
  if (!isReasoningEffort(model.reasoningEffort)) throw new ApiError(400, 'validation', 'A valid reasoning effort is required.');
  return { Id: model.id, UpstreamId: requireText(model.upstreamId, 255, 'upstream model'), DisplayName: requireText(model.displayName, 128, 'model display name'), ReasoningEffort: model.reasoningEffort, IsVisible: model.isVisible, IsDefault: model.isDefault, SortOrder: sortOrder };
}

export function isReasoningEffort(value: unknown): value is AiReasoningEffort {
  return typeof value === 'string' && reasoningEfforts.some((effort) => effort === value);
}

function isExecutionOutcome(value: unknown): value is AiExecutionOutcome {
  return typeof value === 'string' && ['upstream_rejected', 'upstream_invalid', 'upstream_timeout', 'tool_failed', 'persistence_failed', 'internal_error'].includes(value);
}

function count(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) throw invalidResponse('AI analytics count');
  return value;
}

function tokenLimit(value: unknown): number {
  const limit = count(value);
  if (limit > MAX_AI_DAILY_TOKEN_LIMIT) throw invalidResponse('AI token limit');
  return limit;
}

function requireTokenLimit(value: number, subject: string): number {
  if (!Number.isSafeInteger(value) || value < 0 || value > MAX_AI_DAILY_TOKEN_LIMIT) throw new ApiError(400, 'validation', `A valid ${subject} is required.`);
  return value;
}

function nullableCount(value: unknown): number | null { return value === null ? null : count(value); }
function validDay(value: unknown): value is string { return typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/u.test(value) && !Number.isNaN(Date.parse(`${value}T00:00:00Z`)); }

function requireUrl(value: string | null): string {
  if (value === null) throw new ApiError(400, 'validation', 'A provider URL is required.');
  try {
    const url = new URL(value.trim());
    if (!['http:', 'https:'].includes(url.protocol) || url.username.length > 0 || url.password.length > 0 || url.search.length > 0 || url.hash.length > 0 || value.length > 2_048) throw new Error('invalid');
    return value.trim().replace(/\/$/u, '');
  } catch { throw new ApiError(400, 'validation', 'A valid provider URL is required.'); }
}

function optionalApiKey(value: string | undefined): string | undefined {
  if (value === undefined || value.length === 0) return undefined;
  const key = value.trim();
  if (key.length === 0 || key.length > 8_192 || hasControlCharacters(key) || /\s/u.test(key)) throw new ApiError(400, 'validation', 'A valid API key is required.');
  return key;
}

function requireText(value: string, max: number, subject: string): string {
  const text = value.trim();
  if (!validText(text, max)) throw new ApiError(400, 'validation', `A valid ${subject} is required.`);
  return text;
}

function requireMultilineText(value: string, max: number, subject: string): string {
  const text = value.trim();
  if (!validMultilineText(text, max)) throw new ApiError(400, 'validation', `A valid ${subject} is required.`);
  return text;
}

function requireRevision(value: number): number {
  if (!validRevision(value)) throw new ApiError(400, 'validation', 'A valid settings revision is required.');
  return value;
}

function validRevision(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 1;
}

function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort(); const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}
