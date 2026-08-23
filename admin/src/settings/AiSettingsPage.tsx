import { Alert, Button, Card, Checkbox, ComboBox, Description, Disclosure, Input, Label, ListBox, NumberField, Radio, RadioGroup, Select, Skeleton, Switch, TextArea, TextField, Tooltip } from '@heroui/react';
import { ArrowDown, ArrowUp, Bot, Download, Eye, EyeOff, FlaskConical, Plus, RefreshCw, Save, Trash2 } from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useCallback, useContext, useRef, useState } from 'react';
import { ComboBoxStateContext } from 'react-aria-components';

import { PageHeader } from '../ui/PageHeader';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { PageError } from '../ui/SystemPages';
import { useAuthoritativeLoad } from '../ui/useAuthoritativeLoad';
import { AiAnalyticsPanel } from './AiAnalyticsPanel';
import { useTranslate } from './i18n';
import {
  deleteAiSettings,
  discoverAiModels,
  getAiAnalytics,
  getAiSettings,
  saveAiSettings,
  testAiConnection,
  type AiAdminModel,
  type AiAnalytics,
  type AiReasoningEffort,
  type AiSettings,
  MAX_AI_DAILY_TOKEN_LIMIT,
  isReasoningEffort,
} from './aiSettingsApi';

type Operation = 'save' | 'test' | 'delete' | null;
type LoadResult = { settings: AiSettings } | { error: unknown };
type AnalyticsLoadResult = { analytics: AiAnalytics } | { error: unknown };
const reasoningEfforts: AiReasoningEffort[] = ['off', 'low', 'medium', 'high', 'xhigh', 'max'];

export function AiSettingsPage() {
  const tr = useTranslate();
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [settings, setSettings] = useState<AiSettings | null>(null);
  const [analytics, setAnalytics] = useState<AiAnalytics | null>(null);
  const [analyticsError, setAnalyticsError] = useState<unknown>(null);
  const [enabled, setEnabled] = useState(false);
  const [baseUrl, setBaseUrl] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [showKey, setShowKey] = useState(false);
  const [systemPrompt, setSystemPrompt] = useState('');
  const [dailyTotalTokenLimit, setDailyTotalTokenLimit] = useState(0);
  const [dailyUserTokenLimit, setDailyUserTokenLimit] = useState(0);
  const [models, setModels] = useState<AiAdminModel[]>([]);
  const [error, setError] = useState<unknown>(null);
  const [operation, setOperation] = useState<Operation>(null);
  const [discoveringModelId, setDiscoveringModelId] = useState<string | null>(null);
  const [discoveredModels, setDiscoveredModels] = useState<string[]>([]);
  const [conflict, setConflict] = useState(false);
  const operationRef = useRef<Operation>(null);
  const discoveringRef = useRef(false);
  const locked = operation !== null;

  const applySettings = useCallback((value: AiSettings) => {
    setSettings(value); setEnabled(value.enabled); setBaseUrl(value.baseUrl ?? '');
    setSystemPrompt(value.systemPrompt); setDailyTotalTokenLimit(value.dailyTotalTokenLimit);
    setDailyUserTokenLimit(value.dailyUserTokenLimit); setModels(value.models); setApiKey('');
    setShowKey(false); setConflict(false); setError(null);
  }, []);

  const prepareLoadResult = useCallback(async (result: LoadResult) => {
    if ('settings' in result) return () => { applySettings(result.settings); };
    if (await logoutIfAccessDenied(result.error)) return () => undefined;
    return () => { setError(result.error); };
  }, [applySettings, logoutIfAccessDenied]);

  const { isMounted, loading, reload: load } = useAuthoritativeLoad(
    fetchSettings,
    prepareLoadResult,
  );

  const prepareAnalyticsResult = useCallback(async (result: AnalyticsLoadResult) => {
    if ('analytics' in result) return () => { setAnalytics(result.analytics); setAnalyticsError(null); };
    if (await logoutIfAccessDenied(result.error)) return () => undefined;
    return () => { setAnalyticsError(result.error); };
  }, [logoutIfAccessDenied]);

  const { loading: analyticsLoading, reload: loadAnalytics } = useAuthoritativeLoad(
    fetchAnalytics,
    prepareAnalyticsResult,
  );

  const run = async (next: Exclude<Operation, null>, action: () => Promise<void>) => {
    if (operationRef.current !== null) return;
    operationRef.current = next;
    setOperation(next);
    try { await action(); } finally {
      operationRef.current = null;
      if (isMounted()) setOperation(null);
    }
  };

  const save = () => run('save', async () => {
    try {
      const value = await saveAiSettings({ enabled, baseUrl, apiKey, systemPrompt, dailyTotalTokenLimit, dailyUserTokenLimit, revision: settings?.revision ?? null, models });
      if (!isMounted()) return;
      applySettings(value); notify(tr('AI assistant settings saved.', 'AI 助手设置已保存。'), { type: 'success' });
    } catch (failure: unknown) {
      if (!isMounted() || await logoutIfAccessDenied(failure)) return;
      if (isConflict(failure)) { setConflict(true); return; }
      notify(tr('AI assistant settings could not be saved.', '无法保存 AI 助手设置。'), { type: 'error' });
    }
  });

  const test = () => run('test', async () => {
    try {
      await testAiConnection({ baseUrl, apiKey, upstreamModel: models[0]?.upstreamId });
      if (isMounted()) notify(tr('AI connection succeeded.', 'AI 连接成功。'), { type: 'success' });
    } catch (failure: unknown) {
      if (!isMounted() || await logoutIfAccessDenied(failure)) return;
      notify(tr('AI connection failed.', 'AI 连接失败。'), { type: 'error' });
    }
  });

  const discover = async (modelId: string): Promise<boolean> => {
    if (discoveringRef.current) return false;
    discoveringRef.current = true;
    setDiscoveringModelId(modelId);
    try {
      const items = await discoverAiModels({ baseUrl, apiKey });
      if (!isMounted()) return false;
      setDiscoveredModels(items);
      notify(items.length === 0 ? tr('The provider returned no models.', '服务商未返回任何模型。') : `${tr('Loaded', '已加载')} ${String(items.length)} ${tr('models.', '个模型。')}`, { type: items.length === 0 ? 'info' : 'success' });
      return true;
    } catch (failure: unknown) {
      if (!isMounted() || await logoutIfAccessDenied(failure)) return false;
      notify(tr('Available models could not be loaded.', '无法加载可用模型。'), { type: 'error' });
      return false;
    } finally {
      discoveringRef.current = false;
      if (isMounted()) setDiscoveringModelId(null);
    }
  };

  const remove = () => run('delete', async () => {
    if (settings?.revision === null || settings === null) return;
    try { await deleteAiSettings(settings.revision); if (isMounted()) { notify(tr('AI settings removed.', 'AI 设置已移除。'), { type: 'success' }); await load(); } }
    catch (failure: unknown) {
      if (await logoutIfAccessDenied(failure)) throw failure;
      if (isConflict(failure)) setConflict(true);
      notify(tr('AI settings could not be removed.', '无法移除 AI 设置。'), { type: 'error' });
      throw failure;
    }
  });

  const updateModel = (id: string, patch: Partial<AiAdminModel>) => {
    setModels((current) => current.map((model) => model.id === id ? { ...model, ...patch } : model));
  };
  const selectDefault = (id: string) => {
    setModels((current) => current.map((model) => ({ ...model, isDefault: model.id === id, isVisible: model.id === id ? true : model.isVisible })));
  };
  const removeModel = (id: string) => {
    setModels((current) => {
      if (current.length <= 1) return current;
      const removed = current.find((model) => model.id === id);
      const remaining = current.filter((model) => model.id !== id);
      if (!removed?.isDefault) return remaining;
      return remaining.map((model, index) => ({ ...model, isDefault: index === 0, isVisible: index === 0 ? true : model.isVisible }));
    });
  };
  const moveModel = (index: number, offset: -1 | 1) => {
    setModels((current) => {
      const target = index + offset;
      if (target < 0 || target >= current.length) return current;
      const next = [...current];
      const item = next[index];
      const targetItem = next[target];
      if (item === undefined || targetItem === undefined) return current;
      next[index] = targetItem;
      next[target] = item;
      return next;
    });
  };

  return (
    <div className="space-y-5">
      <PageHeader
        actions={<Tooltip><Button aria-label={tr('Reload AI settings', '重新加载 AI 设置')} isIconOnly isPending={loading || analyticsLoading} onPress={() => { void Promise.all([load(), loadAnalytics()]); }} size="sm" variant="ghost"><RefreshCw aria-hidden="true" className="size-4" /></Button><Tooltip.Content>{tr('Reload AI settings', '重新加载 AI 设置')}</Tooltip.Content></Tooltip>}
        description={tr('Configure the server-side provider, media policy, and models visible in the client.', '配置服务端 AI 提供商、媒体策略和前台可见模型。')}
        title={tr('AI assistant', 'AI 助手')}
      />
      {loading && settings === null ? <SettingsSkeleton /> : error !== null && settings === null ? <PageError error={error} headingLevel={2} onRetry={() => { void load(); }} /> : settings !== null ? (
        <Disclosure className="overflow-hidden rounded-lg border border-border bg-surface" defaultExpanded={false}>
          <Disclosure.Heading>
            <Disclosure.Trigger className="flex w-full items-start justify-between gap-4 p-5 text-left hover:bg-default/50 sm:p-6">
              <span className="flex items-start gap-3"><span className="grid size-9 place-items-center rounded-lg bg-accent text-accent-foreground"><Bot aria-hidden="true" className="size-5" /></span><span><span className="block text-base font-semibold text-foreground">{tr('Provider and policy', '提供商与策略')}</span><span className="mt-1 block text-sm text-muted">{tr('The API key stays on the server and is never returned after saving.', 'API 密钥仅保存在服务器上，保存后不会再次返回。')}</span></span></span>
              <Disclosure.Indicator className="mt-1 size-5 shrink-0 text-muted" />
            </Disclosure.Trigger>
          </Disclosure.Heading>
          <Disclosure.Content>
            <Disclosure.Body className="border-t border-border px-5 pb-6 pt-5 sm:px-6">
              <fieldset className="space-y-6" disabled={locked}>
            {conflict && <Alert role="alert" status="warning"><Alert.Content><Alert.Title>{tr('Settings changed elsewhere', '设置已在其他位置变更')}</Alert.Title><Alert.Description>{tr('Reload the latest revision before saving this draft.', '请加载最新版本后再保存当前草稿。')}</Alert.Description></Alert.Content><Button onPress={() => { void load(); }} size="sm" variant="tertiary">{tr('Reload latest', '加载最新设置')}</Button></Alert>}
            {error !== null && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('Settings could not be reloaded', '无法重新加载设置')}</Alert.Title><Alert.Description>{tr('The current draft is still shown. Reload before saving changes.', '当前草稿仍然保留，请重新加载后再保存修改。')}</Alert.Description></Alert.Content><Button onPress={() => { void load(); }} size="sm" variant="tertiary">{tr('Reload', '重新加载')}</Button></Alert>}
            {!settings.encryptionAvailable && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('Credential encryption is unavailable', '凭据加密不可用')}</Alert.Title><Alert.Description>{tr('Configure TJXY_CREDENTIAL_KEYRING before saving provider settings.', '保存提供商设置前，请先配置 TJXY_CREDENTIAL_KEYRING。')}</Alert.Description></Alert.Content></Alert>}
            <div className="grid gap-4 lg:grid-cols-2">
              <TextField fullWidth><Label>{tr('OpenAI-compatible base URL', 'OpenAI 兼容服务地址')}</Label><Input autoComplete="off" fullWidth value={baseUrl} onChange={(event) => { setBaseUrl(event.currentTarget.value); }} placeholder="https://api.example.com/v1" /></TextField>
              <TextField fullWidth><Label>{tr('API key', 'API 密钥')}</Label><div className="relative"><Input autoComplete="new-password" className="pr-11" fullWidth type={showKey ? 'text' : 'password'} value={apiKey} onChange={(event) => { setApiKey(event.currentTarget.value); }} placeholder={settings.configured ? tr('Leave blank to keep the saved key', '留空以保留已保存的密钥') : tr('Enter API key', '输入 API 密钥')} /><Button aria-label={showKey ? tr('Hide API key', '隐藏 API 密钥') : tr('Show API key', '显示 API 密钥')} className="absolute right-1 top-1/2 -translate-y-1/2" isIconOnly onPress={() => { setShowKey((value) => !value); }} size="sm" variant="ghost">{showKey ? <EyeOff className="size-4" /> : <Eye className="size-4" />}</Button></div></TextField>
            </div>
            <TextField fullWidth><Label>{tr('System prompt', '系统提示词')}</Label><TextArea className="min-h-32" value={systemPrompt} onChange={(event) => { setSystemPrompt(event.currentTarget.value); }} /></TextField>
            <div className="border-t border-border pt-5">
              <div className="mb-4"><h2 className="text-base font-semibold">{tr('Usage limits', '用量上限')}</h2><p className="text-sm text-muted">{tr('Limits reset at server-local midnight and use the provider\'s reported token usage.', '上限在服务器本地时间午夜重置，并采用上游返回的真实 Token 用量。')}</p></div>
              <div className="grid gap-4 lg:grid-cols-2">
                <NumberField fullWidth maxValue={MAX_AI_DAILY_TOKEN_LIMIT} minValue={0} name="daily-total-token-limit" onChange={setDailyTotalTokenLimit} step={1_000} value={dailyTotalTokenLimit}>
                  <Label>{tr('Daily total limit', '每日总上限')}</Label>
                  <NumberField.Group><NumberField.DecrementButton /><NumberField.Input /><NumberField.IncrementButton /></NumberField.Group>
                  <Description>{tr('Tokens across all users. Set to 0 for unlimited.', '所有用户合计 Token；设为 0 表示不限制。')}</Description>
                </NumberField>
                <NumberField fullWidth maxValue={MAX_AI_DAILY_TOKEN_LIMIT} minValue={0} name="daily-user-token-limit" onChange={setDailyUserTokenLimit} step={1_000} value={dailyUserTokenLimit}>
                  <Label>{tr('Daily limit per user', '每位用户每日上限')}</Label>
                  <NumberField.Group><NumberField.DecrementButton /><NumberField.Input /><NumberField.IncrementButton /></NumberField.Group>
                  <Description>{tr('Tokens available to each user. Set to 0 for unlimited.', '每位用户可用 Token；设为 0 表示不限制。')}</Description>
                </NumberField>
              </div>
            </div>
            <div className="border-t border-border pt-5">
              <div className="mb-4 flex flex-wrap items-center justify-between gap-3"><div><h2 className="text-base font-semibold">{tr('Client models', '前台模型')}</h2><p className="text-sm text-muted">{tr('Display names are shown to users; upstream IDs stay in the administrator workspace.', '显示名称会展示给用户，上游模型 ID 仅在管理后台可见。')}</p></div><Button onPress={() => { setModels((current) => [...current, newModel(current.length)]); }} size="sm" variant="secondary"><Plus className="size-4" />{tr('Add model', '添加模型')}</Button></div>
              <RadioGroup aria-label="默认模型" onChange={selectDefault} value={models.find((model) => model.isDefault)?.id ?? ''}>
                <div className="space-y-3">{models.map((model, index) => (
                <div className="rounded-lg border border-border p-4" key={model.id}>
                  <div className="grid items-end gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_12rem]">
                    <TextField fullWidth><Label>{tr('Display name', '显示名称')}</Label><Input fullWidth maxLength={128} value={model.displayName} onChange={(event) => { updateModel(model.id, { displayName: event.currentTarget.value }); }} /></TextField>
                    <ComboBox
                      allowsEmptyCollection
                      allowsCustomValue
                      defaultFilter={() => true}
                      fullWidth
                      inputValue={model.upstreamId}
                      menuTrigger="focus"
                      onChange={(key) => { if (key !== null && !Array.isArray(key)) updateModel(model.id, { upstreamId: String(key) }); }}
                      onInputChange={(value) => { updateModel(model.id, { upstreamId: value }); }}
                      value={discoveredModels.includes(model.upstreamId) ? model.upstreamId : null}
                    >
                      <Label>{tr('Upstream model ID', '上游模型 ID')}</Label>
                      <ComboBox.InputGroup>
                        <Input fullWidth maxLength={255} />
                        <Tooltip>
                          <ModelDiscoveryButton
                            isDisabled={discoveringModelId !== null && discoveringModelId !== model.id}
                            isPending={discoveringModelId === model.id}
                            label={`${tr('Fetch available models for', '获取可用模型：')}${tr(' ', '')}${model.displayName || `${tr('model', '模型')} ${String(index + 1)}`}`}
                            onDiscover={() => discover(model.id)}
                          />
                          <Tooltip.Content>{tr('Fetch available models', '获取可用模型')}</Tooltip.Content>
                        </Tooltip>
                      </ComboBox.InputGroup>
                      <ComboBox.Popover>
                        <ListBox>
                          {discoveredModels.map((id) => <ListBox.Item id={id} key={id} textValue={id}>{id}<ListBox.ItemIndicator /></ListBox.Item>)}
                        </ListBox>
                      </ComboBox.Popover>
                    </ComboBox>
                    <Select fullWidth onChange={(value) => { if (isReasoningEffort(value)) updateModel(model.id, { reasoningEffort: value }); }} value={model.reasoningEffort}>
                      <Label>思考强度</Label>
                      <Select.Trigger><Select.Value /><Select.Indicator /></Select.Trigger>
                      <Select.Popover><ListBox>{reasoningEfforts.map((effort) => <ListBox.Item id={effort} key={effort} textValue={effort}>{effort}<ListBox.ItemIndicator /></ListBox.Item>)}</ListBox></Select.Popover>
                    </Select>
                  </div>
                  <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-border pt-3">
                    <div className="flex flex-wrap items-center gap-5">
                      <Checkbox isDisabled={model.isDefault} isSelected={model.isVisible} onChange={(value) => { updateModel(model.id, { isVisible: value }); }}>
                        <Checkbox.Content><Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>前端可见</Checkbox.Content>
                      </Checkbox>
                      <Radio value={model.id}>
                        <Radio.Control><Radio.Indicator /></Radio.Control>
                        <Radio.Content>默认模型</Radio.Content>
                      </Radio>
                    </div>
                    <div className="flex items-center gap-1">
                      <Button aria-label={`${tr('Move', '上移')} ${model.displayName || `${tr('model', '模型')} ${String(index + 1)}`}${tr(' up', '')}`} isDisabled={index === 0} isIconOnly onPress={() => { moveModel(index, -1); }} size="sm" variant="ghost"><ArrowUp className="size-4" /></Button>
                      <Button aria-label={`${tr('Move', '下移')} ${model.displayName || `${tr('model', '模型')} ${String(index + 1)}`}${tr(' down', '')}`} isDisabled={index === models.length - 1} isIconOnly onPress={() => { moveModel(index, 1); }} size="sm" variant="ghost"><ArrowDown className="size-4" /></Button>
                      <Button aria-label={`${tr('Remove', '移除')} ${model.displayName || `${tr('model', '模型')} ${String(index + 1)}`}`} isDisabled={models.length <= 1} isIconOnly onPress={() => { removeModel(model.id); }} variant="danger-soft"><Trash2 className="size-4" /></Button>
                    </div>
                  </div>
                </div>
                ))}</div>
              </RadioGroup>
            </div>
            <Switch isSelected={enabled} onChange={setEnabled}><Switch.Content><Switch.Control><Switch.Thumb /></Switch.Control>{tr('Enable the AI assistant', '启用 AI 助手')}</Switch.Content></Switch>
              </fieldset>
              <div className="mt-6 flex flex-wrap justify-between gap-3 border-t border-border pt-5"><ConfirmDialog confirmLabel={tr('Remove settings', '移除设置')} description={tr('Delete the encrypted provider key, model aliases, and assistant policy. Existing user conversations are retained, but the assistant will be unavailable until it is configured again.', '删除加密的提供商密钥、模型别名和助手策略。已有用户对话会保留，但重新配置前助手将不可用。')} errorDescription={tr('The AI settings remain active. Reload the latest revision and try again.', 'AI 设置仍然有效，请加载最新版本后重试。')} isPending={operation === 'delete'} onConfirm={remove} title={tr('Remove AI assistant settings?', '移除 AI 助手设置？')} trigger={<Button isDisabled={!settings.configured || locked} variant="danger-soft"><Trash2 className="size-4" />{tr('Remove settings', '移除设置')}</Button>} /><div className="flex gap-2"><Button isDisabled={locked} isPending={operation === 'test'} onPress={() => { void test(); }} variant="secondary"><FlaskConical className="size-4" />{tr('Test connection', '测试连接')}</Button><Button isDisabled={locked || !settings.encryptionAvailable || error !== null || conflict} isPending={operation === 'save'} onPress={() => { void save(); }}><Save className="size-4" />{tr('Save settings', '保存设置')}</Button></div></div>
            </Disclosure.Body>
          </Disclosure.Content>
        </Disclosure>
      ) : null}
      <AiAnalyticsPanel analytics={analytics} error={analyticsError} loading={analyticsLoading} onRetry={() => { void loadAnalytics(); }} />
    </div>
  );
}

function ModelDiscoveryButton({
  isDisabled,
  isPending,
  label,
  onDiscover,
}: {
  isDisabled: boolean;
  isPending: boolean;
  label: string;
  onDiscover: () => Promise<boolean>;
}) {
  const comboState = useContext(ComboBoxStateContext);
  return (
    <Button
      aria-label={label}
      isDisabled={isDisabled}
      isIconOnly
      isPending={isPending}
      onPress={() => {
        void onDiscover().then((discovered) => {
          if (discovered) setTimeout(() => { comboState?.open(); }, 0);
        });
      }}
      size="sm"
      variant="ghost"
    >
      <Download aria-hidden="true" className="size-4" />
    </Button>
  );
}

function newModel(sortOrder: number): AiAdminModel { return { id: crypto.randomUUID(), upstreamId: '', displayName: '', reasoningEffort: 'off', isVisible: true, isDefault: sortOrder === 0, sortOrder }; }
function isConflict(value: unknown): boolean { return typeof value === 'object' && value !== null && 'category' in value && value.category === 'conflict'; }
function SettingsSkeleton() { return <Card><Card.Content className="space-y-4 p-6"><Skeleton className="h-10 w-2/3 rounded-lg" /><Skeleton className="h-28 w-full rounded-lg" /><Skeleton className="h-36 w-full rounded-lg" /></Card.Content></Card>; }

async function fetchSettings(signal: AbortSignal): Promise<LoadResult> {
  try { return { settings: await getAiSettings(signal) }; }
  catch (error: unknown) { return { error }; }
}

async function fetchAnalytics(signal: AbortSignal): Promise<AnalyticsLoadResult> {
  try { return { analytics: await getAiAnalytics(signal) }; }
  catch (error: unknown) { return { error }; }
}
