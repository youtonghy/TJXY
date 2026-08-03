export interface AiModel {
  id: string;
  displayName: string;
  isDefault: boolean;
}

export interface AiSource {
  id: string;
  name: string;
  type: string;
  productionYear: number | null;
}

export interface AiMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  sources: AiSource[];
  createdAt: string;
}

export interface AiConversationSummary {
  id: string;
  modelId: string;
  title: string;
  createdAt: string;
  updatedAt: string;
}

export interface AiConversation extends AiConversationSummary {
  messages: AiMessage[];
}

export interface AiChatRequest {
  conversationId: string | null;
  newConversationId: string | null;
  modelId: string;
  message: string;
}

export interface AiChatHandlers {
  onConversation?: (conversationId: string) => void;
  onTool?: (label: string) => void;
  onDelta?: (text: string) => void;
  onSources?: (items: AiSource[]) => void;
  onDone?: (conversationId: string) => void;
  onError?: () => void;
}
