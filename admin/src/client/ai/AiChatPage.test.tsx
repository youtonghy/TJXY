import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';

import { AiChatPage } from './AiChatPage';
import { getAiConversation, getAiModels, listAiConversations, streamAiChat } from './aiApi';

vi.mock('./aiApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('./aiApi')>();
  return { ...original, deleteAiConversation: vi.fn(), getAiConversation: vi.fn(), getAiModels: vi.fn(), listAiConversations: vi.fn(), streamAiChat: vi.fn() };
});

const modelsMock = vi.mocked(getAiModels);
const listMock = vi.mocked(listAiConversations);
const getMock = vi.mocked(getAiConversation);
const streamMock = vi.mocked(streamAiChat);
const modelId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';

beforeEach(() => {
  window.sessionStorage.clear();
  modelsMock.mockReset(); listMock.mockReset(); getMock.mockReset(); streamMock.mockReset();
  modelsMock.mockResolvedValue([{ id: modelId, displayName: 'Cinema Guide', isDefault: true }]);
  listMock.mockResolvedValue([]);
  getMock.mockRejectedValue(new Error('conversation not found'));
});

it('renders a focused media assistant empty state with model names and prompts', async () => {
  renderPage();
  expect(await screen.findByRole('heading', { name: 'AI assistant' })).toBeVisible();
  expect(screen.getByText('Cinema Guide')).toBeVisible();
  expect(screen.getByRole('button', { name: /Recommend something for tonight/ })).toBeVisible();
  expect(screen.getByRole('textbox', { name: 'Message' })).toBeVisible();
});

it('sends a prompt, streams tool status and renders grounded sources', async () => {
  streamMock.mockImplementation((_request, handlers) => {
    handlers.onConversation?.('018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12');
    handlers.onTool?.('Searching the media library');
    handlers.onDelta?.('Try Arrival.');
    handlers.onSources?.([{ id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f13', name: 'Arrival', type: 'Movie', productionYear: 2016 }]);
    handlers.onDone?.('018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12');
    return Promise.resolve();
  });
  renderPage();
  const user = userEvent.setup();
  const input = await screen.findByRole('textbox', { name: 'Message' });
  await user.type(input, 'What should I watch?');
  await user.click(screen.getByRole('button', { name: 'Send message' }));

  expect(screen.getByText('What should I watch?')).toBeVisible();
  await waitFor(() => { expect(screen.getByText('Try Arrival.')).toBeVisible(); });
  const analysis = screen.getByRole('button', { name: 'View analysis · 1 step · 1 source' });
  expect(analysis).toHaveAttribute('aria-expanded', 'false');
  expect(screen.queryByRole('link', { name: /Arrival/ })).not.toBeInTheDocument();
  await user.click(analysis);
  expect(screen.getByText('Searching the media library')).toBeVisible();
  expect(screen.getByRole('link', { name: /Arrival/ })).toHaveAttribute('href', '/app/items/018f17ac-4e99-7ec5-b4fd-8f15ca9f4f13');
  expect(streamMock).toHaveBeenCalledWith(expect.objectContaining({
    conversationId: null,
    modelId,
    message: 'What should I watch?',
  }), expect.any(Object), expect.any(AbortSignal));
  expect(streamMock.mock.calls[0]?.[0].newConversationId).toMatch(/^[0-9a-f-]{36}$/);
});

it('keeps the stop action enabled after clearing the submitted prompt', async () => {
  streamMock.mockImplementation((_request, _handlers, signal) => new Promise<void>((_resolve, reject) => {
    signal?.addEventListener('abort', () => { reject(new DOMException('Aborted', 'AbortError')); }, { once: true });
  }));
  renderPage();
  const user = userEvent.setup();
  const input = await screen.findByRole('textbox', { name: 'Message' });
  await user.type(input, 'Recommend a film');
  await user.click(screen.getByRole('button', { name: 'Send message' }));
  const stop = await screen.findByRole('button', { name: 'Stop response' });
  expect(stop).toBeEnabled();
  await user.click(stop);
  await waitFor(() => { expect(screen.queryByRole('button', { name: 'Stop response' })).not.toBeInTheDocument(); });
  await waitFor(() => { expect(screen.queryByText('Recommend a film')).not.toBeInTheDocument(); });
  expect(screen.queryByRole('status', { name: 'Generating response' })).not.toBeInTheDocument();
  expect(window.sessionStorage.getItem('tjxy.ai.pending-conversation')).toContain(streamMock.mock.calls[0]?.[0].newConversationId);
});

it('rolls back an unpersisted turn when the server reports an error', async () => {
  streamMock.mockRejectedValue(new Error('assistant unavailable'));
  renderPage();
  const user = userEvent.setup();
  await user.type(await screen.findByRole('textbox', { name: 'Message' }), 'This should fail');
  await user.click(screen.getByRole('button', { name: 'Send message' }));
  expect(await screen.findByRole('alert')).toBeVisible();
  expect(screen.queryByText('This should fail')).not.toBeInTheDocument();
  expect(screen.queryByRole('status', { name: 'Generating response' })).not.toBeInTheDocument();
});

it('keeps the turn locked until the SSE stream has fully closed', async () => {
  let resolveStream!: () => void;
  streamMock.mockImplementation((_request, handlers) => {
    handlers.onDone?.('018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12');
    return new Promise<void>((resolve) => { resolveStream = resolve; });
  });
  renderPage();
  const user = userEvent.setup();
  await user.type(await screen.findByRole('textbox', { name: 'Message' }), 'Wait for EOF');
  await user.click(screen.getByRole('button', { name: 'Send message' }));
  expect(screen.getByRole('button', { name: 'Stop response' })).toBeEnabled();

  resolveStream();
  await waitFor(() => { expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled(); });
});

it('retries authoritative recovery after stopping a newly committed conversation', async () => {
  streamMock.mockImplementation((_request, _handlers, signal) => new Promise<void>((_resolve, reject) => {
    signal?.addEventListener('abort', () => { reject(new DOMException('Aborted', 'AbortError')); }, { once: true });
  }));
  getMock.mockRejectedValueOnce(new Error('not committed yet'));
  getMock.mockImplementation((id) => Promise.resolve({
    id, modelId, title: 'Recovered', createdAt: '2026-08-02T01:00:00Z', updatedAt: '2026-08-02T02:00:00Z',
    messages: [{ id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f14', role: 'assistant', content: 'Recovered answer', sources: [], createdAt: '2026-08-02T02:00:00Z' }],
  }));
  renderPage();
  const user = userEvent.setup();
  await user.type(await screen.findByRole('textbox', { name: 'Message' }), 'Recover this turn');
  await user.click(screen.getByRole('button', { name: 'Send message' }));
  await user.click(await screen.findByRole('button', { name: 'Stop response' }));

  expect(await screen.findByText('Recovered answer')).toBeVisible();
  expect(getMock.mock.calls.length).toBeGreaterThanOrEqual(2);
  expect(getMock).toHaveBeenCalledWith(streamMock.mock.calls[0]?.[0].newConversationId);
  expect(window.sessionStorage.getItem('tjxy.ai.pending-conversation')).toBeNull();
});

it('reconciles a pending conversation after the page is reloaded', async () => {
  const pendingId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';
  window.sessionStorage.setItem('tjxy.ai.pending-conversation', JSON.stringify([pendingId]));
  getMock.mockResolvedValue({
    id: pendingId, modelId, title: 'Recovered after reload',
    createdAt: '2026-08-02T01:00:00Z', updatedAt: '2026-08-02T02:00:00Z',
    messages: [{ id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f14', role: 'assistant', content: 'Reloaded answer', sources: [], createdAt: '2026-08-02T02:00:00Z' }],
  });
  renderPage();

  expect(await screen.findByText('Reloaded answer')).toBeVisible();
  expect(window.sessionStorage.getItem('tjxy.ai.pending-conversation')).toBeNull();
});

it('locks the prompt while a history conversation is loading', async () => {
  let resolveConversation!: (value: Awaited<ReturnType<typeof getAiConversation>>) => void;
  listMock.mockResolvedValue([{
    id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12', modelId, title: 'Slow history',
    createdAt: '2026-08-02T01:00:00Z', updatedAt: '2026-08-02T02:00:00Z',
  }]);
  getMock.mockReturnValue(new Promise((resolve) => { resolveConversation = resolve; }));
  renderPage();
  const user = userEvent.setup();
  await user.click(await screen.findByText('Slow history'));
  expect(screen.getByRole('textbox', { name: 'Message' })).toBeDisabled();

  resolveConversation({
    id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12', modelId, title: 'Slow history',
    createdAt: '2026-08-02T01:00:00Z', updatedAt: '2026-08-02T02:00:00Z', messages: [],
  });
  await waitFor(() => { expect(screen.getByRole('textbox', { name: 'Message' })).toBeEnabled(); });
});

function renderPage() {
  return render(<MemoryRouter initialEntries={['/app/ai']}><AiChatPage /></MemoryRouter>);
}
