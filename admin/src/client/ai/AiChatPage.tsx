import { Alert, Button, Disclosure, Drawer, ListBox, Select, Spinner, Tooltip } from '@heroui/react';
import { ChatConversation } from '@heroui-pro/react/chat-conversation';
import { ChatListView } from '@heroui-pro/react/chat-list-view';
import { ChatLoader } from '@heroui-pro/react/chat-loader';
import { ChatMessage } from '@heroui-pro/react/chat-message';
import { StreamMarkdown } from '@heroui-pro/react/markdown';
import { PromptInput, type ChatStatus } from '@heroui-pro/react/prompt-input';
import { PromptSuggestion } from '@heroui-pro/react/prompt-suggestion';
import { ChatSource } from '@heroui-pro/react/chat-source';
import { ChatTool } from '@heroui-pro/react/chat-tool';
import { Globe2, MessageCircle, MessageSquarePlus, PanelLeft, PanelLeftClose, PanelLeftOpen, Sparkles, Trash2 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { validUuid } from '../../api/responseValidation';
import { useTranslate } from '../../settings/i18n';
import { ClientApiError } from '../api/clientApi';
import { deleteAiConversation, getAiConversation, getAiModels, listAiConversations, streamAiChat } from './aiApi';
import type { AiConversationSummary, AiMessage, AiModel, AiSource } from './aiTypes';

const suggestions = [
  ['Recommend something for tonight', '根据我的观影偏好，推荐一部适合今晚看的电影。'],
  ['Find a hidden gem in my library', '从我的媒体库里找一部容易错过的佳作。'],
  ['What should I continue watching?', '结合我的观看记录，我接下来应该继续看什么？'],
] as const;

const markdownComponents = { img: () => null } as const;
const recoveryDelaysMs = [0, 50, 150, 300] as const;
const pendingConversationKey = 'tjxy.ai.pending-conversation';

export function AiChatPage() {
  const tr = useTranslate();
  const [models, setModels] = useState<AiModel[]>([]);
  const [conversations, setConversations] = useState<AiConversationSummary[]>([]);
  const [selectedModel, setSelectedModel] = useState('');
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [messages, setMessages] = useState<AiMessage[]>([]);
  const [prompt, setPrompt] = useState('');
  const [status, setStatus] = useState<ChatStatus>('ready');
  const [toolLabels, setToolLabels] = useState<string[]>([]);
  const [sources, setSources] = useState<AiSource[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [desktopHistoryOpen, setDesktopHistoryOpen] = useState(true);
  const [recovering, setRecovering] = useState(false);
  const controller = useRef<AbortController | null>(null);
  const activeTurn = useRef<{ abort: AbortController; rollback: AiMessage[]; conversationId: string } | null>(null);
  const recoveryGeneration = useRef(0);

  const refreshConversations = async () => { setConversations(await listAiConversations()); };
  useEffect(() => {
    const abort = new AbortController();
    const pendingConversations = Promise.all(readPendingConversations().map((id) => getAiConversation(id).catch(() => null)));
    void Promise.all([getAiModels(), listAiConversations(), pendingConversations]).then(([nextModels, nextConversations, reconciledItems]) => {
      if (abort.signal.aborted) return;
      const reconciled = reconciledItems.filter((item) => item !== null);
      const mergedConversations = [...reconciled, ...nextConversations.filter((item) => !reconciled.some((pending) => pending.id === item.id))];
      setModels(nextModels); setConversations(mergedConversations);
      setSelectedModel(nextModels.find((model) => model.isDefault)?.id ?? nextModels[0]?.id ?? '');
      for (const conversation of reconciled) clearPendingConversation(conversation.id);
      const latest = reconciled.at(-1);
      if (latest !== undefined) {
        setConversationId(latest.id); setSelectedModel(latest.modelId); setMessages(latest.messages);
      }
    }).catch(() => { if (!abort.signal.aborted) setError(true); }).finally(() => { if (!abort.signal.aborted) setLoading(false); });
    let reconciliationRunning = false;
    const reconcilePending = () => {
      const ids = readPendingConversations();
      if (ids.length === 0 || reconciliationRunning) return;
      reconciliationRunning = true;
      void Promise.all(ids.map((id) => getAiConversation(id).catch(() => null))).then(async (items) => {
        const reconciled = items.filter((item) => item !== null);
        if (reconciled.length === 0) return;
        for (const conversation of reconciled) clearPendingConversation(conversation.id);
        const nextConversations = await listAiConversations();
        if (!abort.signal.aborted) setConversations(nextConversations);
      }).catch(() => undefined).finally(() => { reconciliationRunning = false; });
    };
    window.addEventListener('focus', reconcilePending);
    window.addEventListener('online', reconcilePending);
    return () => { abort.abort(); controller.current?.abort(); window.removeEventListener('focus', reconcilePending); window.removeEventListener('online', reconcilePending); };
  }, []);

  const openConversation = async (id: string) => {
    if (status === 'streaming' || status === 'submitted') return;
    const generation = recoveryGeneration.current + 1;
    recoveryGeneration.current = generation; setRecovering(true);
    try {
      const conversation = await getAiConversation(id);
      if (recoveryGeneration.current !== generation) return;
      setConversationId(conversation.id); setSelectedModel(conversation.modelId);
      setMessages(conversation.messages); setSources([]); setToolLabels([]); setHistoryOpen(false); setError(false);
    } catch { if (recoveryGeneration.current === generation) setError(true); }
    finally { if (recoveryGeneration.current === generation) setRecovering(false); }
  };

  const startNew = () => { recoveryGeneration.current += 1; setRecovering(false); const turn = activeTurn.current; activeTurn.current = null; controller.current = null; turn?.abort.abort(); setConversationId(null); setMessages([]); setSources([]); setToolLabels([]); setPrompt(''); setStatus('ready'); setHistoryOpen(false); setError(false); };
  const recoverConversation = (id: string, rollback: AiMessage[]) => {
    const generation = recoveryGeneration.current + 1;
    recoveryGeneration.current = generation; setRecovering(true);
    void (async () => {
      for (const delay of recoveryDelaysMs) {
        if (delay > 0) await new Promise((resolve) => { window.setTimeout(resolve, delay); });
        if (recoveryGeneration.current !== generation) return;
        try {
          const conversation = await getAiConversation(id);
          if (recoveryGeneration.current !== generation) return;
          clearPendingConversation(conversation.id);
          setConversationId(conversation.id); setSelectedModel(conversation.modelId); setMessages(conversation.messages);
          return;
        } catch (failure: unknown) {
          if (!isRecoverableConversationError(failure)) break;
        }
      }
      if (recoveryGeneration.current === generation) setMessages(rollback);
    })().finally(() => {
      if (recoveryGeneration.current === generation) setRecovering(false);
    });
  };
  const stop = () => {
    const turn = activeTurn.current;
    activeTurn.current = null;
    if (controller.current === turn?.abort) controller.current = null;
    turn?.abort.abort();
    setStatus('ready');
    setToolLabels([]); setSources([]);
    void refreshConversations().catch(() => undefined);
    if (!turn) return;
    recoverConversation(turn.conversationId, turn.rollback);
  };
  const send = async () => {
    const message = prompt.trim();
    if (message.length === 0 || selectedModel.length === 0 || recovering || status === 'submitted' || status === 'streaming') return;
    const userMessage: AiMessage = { id: crypto.randomUUID(), role: 'user', content: message, sources: [], createdAt: new Date().toISOString() };
    const assistantId = crypto.randomUUID();
    const turnConversationId = conversationId ?? crypto.randomUUID();
    const newConversationId = conversationId === null ? turnConversationId : null;
    const rollback = messages;
    const abort = new AbortController();
    const turn = { abort, rollback, conversationId: turnConversationId };
    activeTurn.current = turn;
    if (newConversationId !== null) storePendingConversation(newConversationId);
    setMessages((current) => [...current, userMessage, { id: assistantId, role: 'assistant', content: '', sources: [], createdAt: new Date().toISOString() }]);
    setPrompt(''); setStatus('submitted'); setToolLabels([]); setSources([]); setError(false);
    controller.current = abort;
    const streamState = { terminalServerError: false };
    try {
      await streamAiChat({ conversationId, newConversationId, modelId: selectedModel, message }, {
        onConversation: (id) => { if (activeTurn.current !== turn) return; turn.conversationId = id; setConversationId(id); },
        onTool: (label) => { if (activeTurn.current !== turn) return; setStatus('streaming'); setToolLabels((current) => [...current, label]); },
        onDelta: (text) => { if (activeTurn.current !== turn) return; setStatus('streaming'); setMessages((current) => current.map((item) => item.id === assistantId ? { ...item, content: item.content + text } : item)); },
        onSources: (items) => { if (activeTurn.current !== turn) return; setSources(items); setMessages((current) => current.map((item) => item.id === assistantId ? { ...item, sources: items } : item)); },
        onError: () => { streamState.terminalServerError = true; if (newConversationId !== null) clearPendingConversation(newConversationId); },
      }, abort.signal);
      if (activeTurn.current !== turn) return;
      if (newConversationId !== null) clearPendingConversation(newConversationId);
      setStatus('ready');
      void refreshConversations().catch(() => undefined);
    } catch (failure: unknown) {
      if (activeTurn.current !== turn) return;
      if (!(failure instanceof DOMException && failure.name === 'AbortError')) {
        setStatus('error'); setError(true); setMessages(rollback); setToolLabels([]); setSources([]);
        const shouldReconcile = !streamState.terminalServerError && isRecoverableConversationError(failure);
        if (shouldReconcile) recoverConversation(turn.conversationId, rollback);
        else if (newConversationId !== null) clearPendingConversation(newConversationId);
      }
    } finally { if (controller.current === abort) controller.current = null; if (activeTurn.current === turn) activeTurn.current = null; }
  };

  const removeConversation = async (id: string) => {
    try { await deleteAiConversation(id); if (conversationId === id) startNew(); await refreshConversations(); }
    catch { setError(true); }
  };

  if (loading) return <div className="grid min-h-[60vh] place-items-center"><Spinner aria-label={tr('Loading AI assistant', '正在加载 AI 助手')} /></div>;
  if (error && models.length === 0) return <Alert status="danger"><Alert.Content><Alert.Title>{tr('The AI assistant could not be loaded', '无法加载 AI 助手')}</Alert.Title><Alert.Description>{tr('The server could not provide the assistant configuration.', '服务器暂时无法提供 AI 助手配置。')}</Alert.Description></Alert.Content></Alert>;
  if (models.length === 0) return <Alert status="warning"><Alert.Content><Alert.Title>{tr('AI assistant is not configured', 'AI 助手尚未配置')}</Alert.Title><Alert.Description>{tr('Ask an administrator to enable at least one model.', '请联系管理员启用至少一个模型。')}</Alert.Description></Alert.Content></Alert>;
  const activeConversationTitle = conversations.find((item) => item.id === conversationId)?.title;

  return (
    <div className={`-my-6 h-[calc(100svh-4rem)] overflow-hidden lg:grid ${desktopHistoryOpen ? 'lg:grid-cols-[15.5rem_minmax(0,1fr)]' : 'lg:grid-cols-[minmax(0,1fr)]'}`}>
      {desktopHistoryOpen ? <aside className="hidden min-h-0 border-r border-border bg-surface-secondary/35 p-3 lg:block" aria-label={tr('Conversation history', '对话历史')}>
        <ConversationHistory activeConversationId={conversationId} actions={{ onDelete: removeConversation, onNew: startNew, onOpen: openConversation }} conversations={conversations} onCollapse={() => { setDesktopHistoryOpen(false); }} tr={tr} />
      </aside> : null}
      <section className="flex h-full min-h-0 min-w-0 flex-col" aria-labelledby="ai-heading">
        <header className="flex min-h-16 shrink-0 items-center gap-3 border-b border-border px-1 py-2 sm:px-4">
          {!desktopHistoryOpen ? <Tooltip delay={0}><Tooltip.Trigger><Button aria-label={tr('Show conversation history', '展开对话历史')} className="hidden lg:inline-flex" isIconOnly onPress={() => { setDesktopHistoryOpen(true); }} size="sm" variant="ghost"><PanelLeftOpen className="size-4" /></Button></Tooltip.Trigger><Tooltip.Content>{tr('Show conversation history', '展开对话历史')}</Tooltip.Content></Tooltip> : null}
          <Drawer isOpen={historyOpen} onOpenChange={setHistoryOpen}>
            <Drawer.Trigger aria-label={tr('Open conversation history', '打开对话历史')} className="inline-flex size-10 items-center justify-center rounded-md hover:bg-default lg:hidden">
              <PanelLeft className="size-4" />
            </Drawer.Trigger>
            <Drawer.Backdrop>
              <Drawer.Content className="w-80 max-w-[calc(100vw-3rem)]" placement="left">
                <Drawer.Dialog>
                  <Drawer.Header><Drawer.Heading>{tr('Conversations', '对话')}</Drawer.Heading><Drawer.CloseTrigger aria-label={tr('Close conversation history', '关闭对话历史')} /></Drawer.Header>
                  <Drawer.Body><ConversationHistory activeConversationId={conversationId} actions={{ onDelete: removeConversation, onNew: startNew, onOpen: openConversation }} conversations={conversations} hideHeading tr={tr} /></Drawer.Body>
                </Drawer.Dialog>
              </Drawer.Content>
            </Drawer.Backdrop>
          </Drawer>
          <div className="min-w-0 flex-1"><h1 className="truncate text-sm font-semibold sm:text-base" id="ai-heading">{activeConversationTitle ?? tr('AI assistant', 'AI 助手')}</h1><p className="truncate text-xs text-muted">{activeConversationTitle === undefined ? tr('Movies, television, and your viewing context', '影视内容与个性化观影建议') : tr('Conversation grounded in your media library', '基于你的媒体库进行对话')}</p></div>
        </header>
        {error && <Alert className="m-4" role="alert" status="danger"><Alert.Content><Alert.Title>{tr('The response could not be completed', '无法完成本次回复')}</Alert.Title><Alert.Description>{tr('Try again or start a new conversation.', '请重试或新建对话。')}</Alert.Description></Alert.Content></Alert>}
        <ChatConversation className="min-h-0 flex-1"><ChatConversation.Content className="mx-auto w-full max-w-[714px] px-4 py-8 sm:py-10">
          {messages.length === 0 ? <EmptyPrompts onSelect={setPrompt} tr={tr} /> : messages.map((message) => message.role === 'user' ? <ChatMessage.User key={message.id}><ChatMessage.Bubble><ChatMessage.Content>{message.content}</ChatMessage.Content></ChatMessage.Bubble></ChatMessage.User> : <ChatMessage.Assistant key={message.id}><ChatMessage.Body><ChatMessage.Content>{message.content.length > 0 ? <StreamMarkdown components={markdownComponents} isStreaming={status === 'streaming'}>{message.content}</StreamMarkdown> : (message === messages.at(-1) && (status === 'submitted' || status === 'streaming')) ? <ChatLoader.Dots label={tr('Generating response', '正在生成回复')} /> : null}<AssistantContextDisclosure isStreaming={message === messages.at(-1) && status === 'streaming'} sources={message.sources} toolLabels={message === messages.at(-1) ? toolLabels : []} tr={tr} /></ChatMessage.Content></ChatMessage.Body></ChatMessage.Assistant>)}
          <ChatConversation.ScrollAnchor /></ChatConversation.Content><ChatConversation.ScrollButton aria-label={tr('Scroll to latest message', '滚动到最新消息')} /></ChatConversation>
        <div className="shrink-0 bg-background px-2 pb-4 pt-3 sm:px-4"><PromptInput className="mx-auto max-w-[714px]" isDisabled={recovering} lockInputOnRun onStop={stop} onSubmit={() => { void send(); }} onValueChange={setPrompt} status={status} value={prompt}><PromptInput.Shell className="rounded-lg"><PromptInput.Content><PromptInput.TextArea aria-label={tr('Message', '消息')} placeholder={tr('Ask about a film, series, or recommendation', '询问影视信息或个性化推荐')} /></PromptInput.Content><PromptInput.Toolbar><PromptInput.ToolbarStart><Select aria-label={tr('Model', '模型')} className="w-44" isDisabled={conversationId !== null} onChange={(value) => { if (typeof value === 'string') setSelectedModel(value); }} value={selectedModel} variant="secondary"><Select.Trigger className="h-8 min-h-8 gap-2 rounded-lg px-2"><Globe2 aria-hidden="true" className="size-4 shrink-0" /><Select.Value /><Select.Indicator /></Select.Trigger><Select.Popover><ListBox>{models.map((model) => <ListBox.Item id={model.id} key={model.id} textValue={model.displayName}>{model.displayName}<ListBox.ItemIndicator /></ListBox.Item>)}</ListBox></Select.Popover></Select></PromptInput.ToolbarStart><PromptInput.ToolbarEnd><PromptInput.Send aria-label={status === 'submitted' || status === 'streaming' ? tr('Stop response', '停止回复') : tr('Send message', '发送消息')} isDisabled={recovering || ((status === 'ready' || status === 'error') && prompt.trim().length === 0)} status={status} /></PromptInput.ToolbarEnd></PromptInput.Toolbar></PromptInput.Shell></PromptInput><p className="mx-auto mt-2 max-w-[714px] text-center text-xs text-muted">{sources.length > 0 ? tr(`${String(sources.length)} library sources`, `${String(sources.length)} 个媒体库来源`) : tr('AI can make mistakes. Check important information.', 'AI 可能会出错，请核实重要信息。')}</p></div>
      </section>
    </div>
  );
}

function AssistantContextDisclosure({ isStreaming, sources, toolLabels, tr }: {
  isStreaming: boolean;
  sources: AiSource[];
  toolLabels: string[];
  tr: ReturnType<typeof useTranslate>;
}) {
  if (toolLabels.length === 0 && sources.length === 0) return null;
  const counts = [
    toolLabels.length > 0 ? tr(`${String(toolLabels.length)} ${toolLabels.length === 1 ? 'step' : 'steps'}`, `${String(toolLabels.length)} 个步骤`) : null,
    sources.length > 0 ? tr(`${String(sources.length)} ${sources.length === 1 ? 'source' : 'sources'}`, `${String(sources.length)} 个来源`) : null,
  ].filter((value): value is string => value !== null);
  const summary = [
    isStreaming ? tr('Analyzing', '正在分析') : tr('View analysis', '查看分析过程'),
    ...counts,
  ].join(' · ');

  return (
    <Disclosure className="mt-4 overflow-hidden rounded-md border border-border">
      <Disclosure.Heading>
        <Disclosure.Trigger className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm font-medium text-muted hover:bg-default hover:text-foreground">
          <span>{summary}</span>
          <Disclosure.Indicator className="size-4 shrink-0" />
        </Disclosure.Trigger>
      </Disclosure.Heading>
      <Disclosure.Content>
        <Disclosure.Body className="space-y-3 border-t border-border px-3 py-3">
          {toolLabels.map((label, index) => <ChatTool isExpandable={false} key={`${label}-${String(index)}`} state={isStreaming ? 'input-streaming' : 'output-available'} toolName={label} />)}
          {sources.length > 0 && <div className="flex flex-wrap gap-2">{sources.map((source) => <ChatSource description={[source.type, source.productionYear].filter(Boolean).join(' · ')} enablePreview={false} href={`/app/items/${source.id}`} key={source.id} sourceType="url" title={source.name} />)}</div>}
        </Disclosure.Body>
      </Disclosure.Content>
    </Disclosure>
  );
}

function readPendingConversations(): string[] {
  try {
    const value = window.sessionStorage.getItem(pendingConversationKey);
    if (value === null) return [];
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed) ? [...new Set(parsed.filter((item): item is string => typeof item === 'string' && validUuid(item)))].slice(-20) : [];
  } catch { return []; }
}

function storePendingConversation(id: string): void {
  try {
    const values = [...readPendingConversations().filter((value) => value !== id), id].slice(-20);
    window.sessionStorage.setItem(pendingConversationKey, JSON.stringify(values));
  } catch { /* Storage can be unavailable in privacy-restricted contexts. */ }
}

function clearPendingConversation(id: string): void {
  try {
    const values = readPendingConversations().filter((value) => value !== id);
    if (values.length === 0) window.sessionStorage.removeItem(pendingConversationKey);
    else window.sessionStorage.setItem(pendingConversationKey, JSON.stringify(values));
  }
  catch { /* Storage can be unavailable in privacy-restricted contexts. */ }
}

function isRecoverableConversationError(failure: unknown): boolean {
  return !(failure instanceof ClientApiError) || ['network', 'not-found', 'unavailable', 'invalid-response'].includes(failure.kind);
}

function EmptyPrompts({ onSelect, tr }: { onSelect: (value: string) => void; tr: ReturnType<typeof useTranslate> }) { return <div className="mx-auto flex min-h-[28rem] w-full flex-col justify-center py-8"><div className="mb-7"><span className="mb-4 grid size-11 place-items-center rounded-lg bg-accent text-accent-foreground"><Sparkles className="size-5" /></span><h2 className="text-2xl font-semibold">{tr('What would you like to watch?', '想看点什么？')}</h2><p className="mt-2 text-sm leading-6 text-muted">{tr('Start from your library and viewing history to find the right title for now.', '从你的媒体库与观看记录出发，找一部真正适合现在的影片。')}</p></div><PromptSuggestion><PromptSuggestion.Items className="grid grid-cols-1 gap-2 sm:grid-cols-2">{suggestions.map(([english, chinese]) => <PromptSuggestion.Item className="rounded-lg" key={english} onPress={() => { onSelect(tr(english, chinese)); }}>{tr(english, chinese)}</PromptSuggestion.Item>)}</PromptSuggestion.Items></PromptSuggestion></div>; }

function ConversationHistory({ activeConversationId, actions, conversations, hideHeading = false, onCollapse, tr }: {
  activeConversationId: string | null;
  actions: {
    onDelete: (id: string) => Promise<void>;
    onNew: () => void;
    onOpen: (id: string) => Promise<void>;
  };
  conversations: AiConversationSummary[];
  hideHeading?: boolean;
  onCollapse?: () => void;
  tr: ReturnType<typeof useTranslate>;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="mb-3 flex items-center justify-between px-1">
        <h2 className={hideHeading ? 'sr-only' : 'text-sm font-semibold'}>{tr('Conversations', '对话')}</h2>
        <div className="flex items-center gap-1">{onCollapse ? <Tooltip delay={0}><Tooltip.Trigger><Button aria-label={tr('Collapse conversation history', '收起对话历史')} isIconOnly onPress={onCollapse} size="sm" variant="ghost"><PanelLeftClose className="size-4" /></Button></Tooltip.Trigger><Tooltip.Content>{tr('Collapse conversation history', '收起对话历史')}</Tooltip.Content></Tooltip> : null}<Tooltip delay={0}><Tooltip.Trigger><Button aria-label={tr('New conversation', '新建对话')} isIconOnly onPress={actions.onNew} size="sm" variant="ghost"><MessageSquarePlus className="size-4" /></Button></Tooltip.Trigger><Tooltip.Content>{tr('New conversation', '新建对话')}</Tooltip.Content></Tooltip></div>
      </div>
      <ChatListView aria-label={tr('Saved conversations', '已保存对话')} className="min-h-0 flex-1 overflow-y-auto" items={conversations} onAction={(key) => { void actions.onOpen(String(key)); }} renderEmptyState={() => <p className="px-3 py-8 text-center text-sm text-muted">{tr('No conversations yet', '还没有对话')}</p>}>
        {(item) => <ChatListView.Item className={`group rounded-lg border-b-0 px-2 py-2 ${item.id === activeConversationId ? 'bg-default' : ''}`} id={item.id} textValue={item.title}><ChatListView.ItemContent className="gap-2"><ChatListView.Icon><MessageCircle className="size-4" /></ChatListView.Icon><ChatListView.Text><ChatListView.Title>{item.title}</ChatListView.Title><ChatListView.Preview>{new Date(item.updatedAt).toLocaleDateString()}</ChatListView.Preview></ChatListView.Text></ChatListView.ItemContent><ChatListView.Meta className="opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100"><Button aria-label={tr(`Delete ${item.title}`, `删除 ${item.title}`)} isIconOnly onPress={() => { void actions.onDelete(item.id); }} size="sm" variant="ghost"><Trash2 className="size-4" /></Button></ChatListView.Meta></ChatListView.Item>}
      </ChatListView>
    </div>
  );
}
