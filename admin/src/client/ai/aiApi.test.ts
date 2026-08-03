import { clientFetch, clientRequest } from '../api/clientApi';
import {
  deleteAiConversation,
  getAiConversation,
  getAiModels,
  listAiConversations,
  streamAiChat,
} from './aiApi';

vi.mock('../api/clientApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/clientApi')>();
  return { ...original, clientFetch: vi.fn(), clientRequest: vi.fn() };
});

const fetchMock = vi.mocked(clientFetch);
const requestMock = vi.mocked(clientRequest);

beforeEach(() => {
  fetchMock.mockReset();
  requestMock.mockReset();
});

it('strictly maps visible models and user-scoped conversation data', async () => {
  requestMock
    .mockResolvedValueOnce({ Items: [{ Id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11', DisplayName: 'Cinema Guide', IsDefault: true }] })
    .mockResolvedValueOnce({ Items: [{ Id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12', ModelId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11', Title: 'Tonight', CreatedAt: '2026-08-02T01:00:00Z', UpdatedAt: '2026-08-02T02:00:00Z' }] })
    .mockResolvedValueOnce({
      Id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12', ModelId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11', Title: 'Tonight', CreatedAt: '2026-08-02T01:00:00Z', UpdatedAt: '2026-08-02T02:00:00Z',
      Messages: [{ Id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f13', Role: 'assistant', Content: 'Try\nArrival.', Sources: [], CreatedAt: '2026-08-02T02:00:00Z' }],
    })
    .mockResolvedValueOnce(undefined);

  await expect(getAiModels()).resolves.toEqual([{ id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11', displayName: 'Cinema Guide', isDefault: true }]);
  await expect(listAiConversations()).resolves.toHaveLength(1);
  await expect(getAiConversation('018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12')).resolves.toMatchObject({ messages: [{ role: 'assistant', content: 'Try\nArrival.' }] });
  await deleteAiConversation('018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12');

  expect(requestMock).toHaveBeenNthCalledWith(1, '/Ai/Models');
  expect(requestMock).toHaveBeenNthCalledWith(2, '/Ai/Conversations');
  expect(requestMock).toHaveBeenNthCalledWith(4, '/Ai/Conversations/018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12', { method: 'DELETE' });
});

it('parses SSE events across arbitrary chunks and requires one terminal event', async () => {
  fetchMock.mockResolvedValue(sseResponse([
    'event: conversation\ndata: {"Id":"018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12"}\n\nevent: del',
    'ta\ndata: {"Text":"Try "}\n\n: keep-alive\n\nevent: tool\ndata: {"Label":"Searching the media library"}\n\n',
    'event: sources\ndata: {"Items":[{"Id":"018f17ac-4e99-7ec5-b4fd-8f15ca9f4f14","Name":"Arrival","Type":"Movie","ProductionYear":2016}]}\n\nevent: delta\ndata: {"Text":"Arrival."}\n\nevent: done\ndata: {"ConversationId":"018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12"}\n\n',
  ]));
  const seen: string[] = [];

  await streamAiChat(
    {
      conversationId: null,
      newConversationId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12',
      modelId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11',
      message: 'Recommend a\nfilm',
    },
    {
      onConversation: (id) => { seen.push(`conversation:${id}`); },
      onDelta: (text) => { seen.push(`delta:${text}`); },
      onTool: (label) => { seen.push(`tool:${label}`); },
      onSources: (items) => { seen.push(`sources:${items[0]?.name ?? ''}`); },
      onDone: () => { seen.push('done'); },
    },
  );

  expect(seen).toEqual([
    'conversation:018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12',
    'delta:Try ',
    'tool:Searching the media library',
    'sources:Arrival',
    'delta:Arrival.',
    'done',
  ]);
  expect(fetchMock).toHaveBeenCalledWith('/Ai/Chat', expect.objectContaining({
    method: 'POST',
    headers: { Accept: 'text/event-stream' },
  }));
  expect(fetchMock.mock.calls[0]?.[1]?.body).toBe(JSON.stringify({
    ModelId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11',
    Message: 'Recommend a\nfilm',
    NewConversationId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12',
  }));
});

it('rejects malformed, unknown, or unterminated SSE without exposing server detail', async () => {
  for (const body of [
    'event: thought\ndata: {"Text":"private"}\n\n',
    'event: delta\ndata: {"Text":4}\n\n',
    'event: delta\ndata: {"Text":"unfinished"}',
  ]) {
    fetchMock.mockResolvedValueOnce(sseResponse([body]));
    await expect(streamAiChat(
      {
        conversationId: null,
        newConversationId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12',
        modelId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11',
        message: 'Film',
      },
      {},
    )).rejects.toMatchObject({ kind: 'invalid-response' });
  }
});

it('requires exactly one existing or new conversation id', async () => {
  await expect(streamAiChat({
    conversationId: null,
    newConversationId: null,
    modelId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11',
    message: 'Film',
  }, {})).rejects.toMatchObject({ kind: 'validation' });

  await expect(streamAiChat({
    conversationId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12',
    newConversationId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f13',
    modelId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11',
    message: 'Film',
  }, {})).rejects.toMatchObject({ kind: 'validation' });

  expect(fetchMock).not.toHaveBeenCalled();
});

function sseResponse(chunks: string[]): Response {
  const encoder = new TextEncoder();
  return new Response(new ReadableStream({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      controller.close();
    },
  }), { status: 200, headers: { 'Content-Type': 'text/event-stream' } });
}
