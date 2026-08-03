import { isRecord, validDate, validMultilineText, validText, validUuid } from '../../api/responseValidation';
import { ClientApiError, clientFetch, clientRequest } from '../api/clientApi';
import type {
  AiChatHandlers,
  AiChatRequest,
  AiConversation,
  AiConversationSummary,
  AiMessage,
  AiModel,
  AiSource,
} from './aiTypes';

const MAX_STREAM_BYTES = 1024 * 1024;
const MAX_FRAME_BYTES = 64 * 1024;

export async function getAiModels(): Promise<AiModel[]> {
  const value = await clientRequest<unknown>('/Ai/Models');
  if (!isRecord(value) || !exactKeys(value, ['Items']) || !Array.isArray(value.Items)) {
    throw invalidResponse();
  }
  return value.Items.map(toModel);
}

export async function listAiConversations(): Promise<AiConversationSummary[]> {
  const value = await clientRequest<unknown>('/Ai/Conversations');
  if (!isRecord(value) || !exactKeys(value, ['Items']) || !Array.isArray(value.Items)) {
    throw invalidResponse();
  }
  return value.Items.map(toConversationSummary);
}

export async function getAiConversation(id: string): Promise<AiConversation> {
  requireUuid(id);
  const value = await clientRequest<unknown>(`/Ai/Conversations/${id}`);
  if (!isRecord(value) || !exactKeys(value, [
    'Id', 'ModelId', 'Title', 'CreatedAt', 'UpdatedAt', 'Messages',
  ]) || !Array.isArray(value.Messages)) {
    throw invalidResponse();
  }
  return {
    ...toConversationFields(value),
    messages: value.Messages.map(toMessage),
  };
}

export async function deleteAiConversation(id: string): Promise<void> {
  requireUuid(id);
  await clientRequest(`/Ai/Conversations/${id}`, { method: 'DELETE' });
}

export async function streamAiChat(
  request: AiChatRequest,
  handlers: AiChatHandlers,
  signal?: AbortSignal,
): Promise<void> {
  const modelId = requireUuid(request.modelId);
  const conversationId = request.conversationId === null
    ? null
    : requireUuid(request.conversationId);
  const newConversationId = request.newConversationId === null
    ? null
    : requireUuid(request.newConversationId);
  if ((conversationId === null) === (newConversationId === null)) {
    throw new ClientApiError(400, 'validation');
  }
  const message = request.message.trim();
  if (!validMultilineText(message, 16_000)) throw new ClientApiError(400, 'validation');
  const body: Record<string, unknown> = { ModelId: modelId, Message: message };
  if (conversationId !== null) body.ConversationId = conversationId;
  if (newConversationId !== null) body.NewConversationId = newConversationId;
  const response = await clientFetch('/Ai/Chat', {
    method: 'POST',
    headers: { Accept: 'text/event-stream' },
    body: JSON.stringify(body),
    signal,
  });
  if (!response.ok) throw new ClientApiError(response.status, responseKind(response.status));
  if (response.headers.get('content-type')?.split(';')[0]?.trim().toLowerCase() !== 'text/event-stream') {
    throw invalidResponse();
  }
  if (response.body === null) throw invalidResponse();

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let bytesRead = 0;
  let terminal = false;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    bytesRead += value.byteLength;
    if (bytesRead > MAX_STREAM_BYTES) throw invalidResponse();
    buffer += decoder.decode(value, { stream: true });
    buffer = buffer.replaceAll('\r\n', '\n');
    let boundary = buffer.indexOf('\n\n');
    while (boundary >= 0) {
      const frame = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      if (new TextEncoder().encode(frame).byteLength > MAX_FRAME_BYTES) throw invalidResponse();
      if (frame.length > 0) terminal = dispatchFrame(frame, terminal, handlers);
      boundary = buffer.indexOf('\n\n');
    }
  }
  buffer += decoder.decode();
  if (buffer.trim().length > 0 || !terminal) throw invalidResponse();
}

function dispatchFrame(frame: string, terminal: boolean, handlers: AiChatHandlers): boolean {
  const lines = frame.split('\n');
  if (lines.every((line) => line.length === 0 || line.startsWith(':'))) return terminal;
  if (terminal) throw invalidResponse();
  const eventLines = lines.filter((line) => line.startsWith('event:'));
  const dataLines = lines.filter((line) => line.startsWith('data:'));
  const other = lines.filter((line) => line.length > 0 && !line.startsWith(':') && !line.startsWith('event:') && !line.startsWith('data:'));
  if (eventLines.length !== 1 || dataLines.length !== 1 || other.length > 0) throw invalidResponse();
  const name = eventLines[0]?.slice(6).trim();
  let payload: unknown;
  try { payload = JSON.parse(dataLines[0]?.slice(5).trim() ?? ''); } catch { throw invalidResponse(); }
  if (!isRecord(payload)) throw invalidResponse();
  switch (name) {
    case 'conversation': {
      if (!exactKeys(payload, ['Id']) || !validUuid(payload.Id)) throw invalidResponse();
      handlers.onConversation?.(payload.Id);
      return false;
    }
    case 'tool': {
      if (!exactKeys(payload, ['Label']) || !validText(payload.Label, 160)) throw invalidResponse();
      handlers.onTool?.(payload.Label);
      return false;
    }
    case 'delta': {
      if (!exactKeys(payload, ['Text']) || !validMultilineText(payload.Text, 16_000, true)) throw invalidResponse();
      handlers.onDelta?.(payload.Text);
      return false;
    }
    case 'sources': {
      if (!exactKeys(payload, ['Items']) || !Array.isArray(payload.Items)) throw invalidResponse();
      handlers.onSources?.(payload.Items.map(toSource));
      return false;
    }
    case 'done': {
      if (!exactKeys(payload, ['ConversationId']) || !validUuid(payload.ConversationId)) throw invalidResponse();
      handlers.onDone?.(payload.ConversationId);
      return true;
    }
    case 'error': {
      if (!exactKeys(payload, ['Code', 'Message']) || !validText(payload.Code, 64) || !validText(payload.Message, 256)) throw invalidResponse();
      handlers.onError?.();
      throw new ClientApiError(503, 'unavailable');
    }
    default: throw invalidResponse();
  }
}

function toModel(value: unknown): AiModel {
  if (!isRecord(value) || !exactKeys(value, ['Id', 'DisplayName', 'IsDefault']) || !validUuid(value.Id) || !validText(value.DisplayName, 160) || typeof value.IsDefault !== 'boolean') throw invalidResponse();
  return { id: value.Id, displayName: value.DisplayName, isDefault: value.IsDefault };
}

function toConversationSummary(value: unknown): AiConversationSummary {
  if (!isRecord(value) || !exactKeys(value, ['Id', 'ModelId', 'Title', 'CreatedAt', 'UpdatedAt'])) throw invalidResponse();
  return toConversationFields(value);
}

function toConversationFields(value: Record<string, unknown>): AiConversationSummary {
  if (!validUuid(value.Id) || !validUuid(value.ModelId) || !validText(value.Title, 160) || !validDate(value.CreatedAt) || !validDate(value.UpdatedAt)) throw invalidResponse();
  return { id: value.Id, modelId: value.ModelId, title: value.Title, createdAt: value.CreatedAt, updatedAt: value.UpdatedAt };
}

function toMessage(value: unknown): AiMessage {
  if (!isRecord(value) || !exactKeys(value, ['Id', 'Role', 'Content', 'Sources', 'CreatedAt']) || !validUuid(value.Id) || (value.Role !== 'user' && value.Role !== 'assistant') || !validMultilineText(value.Content, 32_000) || !Array.isArray(value.Sources) || !validDate(value.CreatedAt)) throw invalidResponse();
  return { id: value.Id, role: value.Role, content: value.Content, sources: value.Sources.map(toSource), createdAt: value.CreatedAt };
}

function toSource(value: unknown): AiSource {
  if (!isRecord(value) || !exactKeys(value, ['Id', 'Name', 'Type', 'ProductionYear']) || !validUuid(value.Id) || !validText(value.Name, 512) || !validText(value.Type, 64) || (value.ProductionYear !== null && (typeof value.ProductionYear !== 'number' || !Number.isSafeInteger(value.ProductionYear)))) throw invalidResponse();
  return { id: value.Id, name: value.Name, type: value.Type, productionYear: value.ProductionYear };
}

function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function requireUuid(value: string): string {
  if (!validUuid(value)) throw new ClientApiError(400, 'validation');
  return value;
}

function invalidResponse(): ClientApiError {
  return new ClientApiError(200, 'invalid-response');
}

function responseKind(status: number): 'authentication' | 'authorization' | 'not-found' | 'validation' | 'unavailable' | 'unexpected' {
  if (status === 401) return 'authentication';
  if (status === 403) return 'authorization';
  if (status === 404) return 'not-found';
  if (status === 400 || status === 422) return 'validation';
  if (status >= 500) return 'unavailable';
  return 'unexpected';
}
