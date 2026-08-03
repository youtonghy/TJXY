import { Alert, Button, Card, Checkbox, ComboBox, Input, Label, ListBox, Radio, RadioGroup, Select, Skeleton, Switch, TextArea, TextField, Tooltip } from '@heroui/react';
import { ArrowDown, ArrowUp, Bot, Download, Eye, EyeOff, FlaskConical, Plus, RefreshCw, Save, Trash2 } from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useCallback, useContext, useRef, useState } from 'react';
import { ComboBoxStateContext } from 'react-aria-components';

import { PageHeader } from '../ui/PageHeader';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { PageError } from '../ui/SystemPages';
import { useAuthoritativeLoad } from '../ui/useAuthoritativeLoad';
import { AiAnalyticsPanel } from './AiAnalyticsPanel';
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
  isReasoningEffort,
} from './aiSettingsApi';

type Operation = 'save' | 'test' | 'delete' | null;
type LoadResult = { settings: AiSettings } | { error: unknown };
type AnalyticsLoadResult = { analytics: AiAnalytics } | { error: unknown };
const reasoningEfforts: AiReasoningEffort[] = ['off', 'low', 'medium', 'high', 'xhigh', 'max'];

export function AiSettingsPage() {
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
    setSystemPrompt(value.systemPrompt); setModels(value.models); setApiKey('');
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
      const value = await saveAiSettings({ enabled, baseUrl, apiKey, systemPrompt, revision: settings?.revision ?? null, models });
      if (!isMounted()) return;
      applySettings(value); notify('AI assistant settings saved.', { type: 'success' });
    } catch (failure: unknown) {
      if (!isMounted() || await logoutIfAccessDenied(failure)) return;
      if (isConflict(failure)) { setConflict(true); return; }
      notify('AI assistant settings could not be saved.', { type: 'error' });
    }
  });

  const test = () => run('test', async () => {
    try {
      await testAiConnection({ baseUrl, apiKey, upstreamModel: models[0]?.upstreamId });
      if (isMounted()) notify('AI connection succeeded.', { type: 'success' });
    } catch (failure: unknown) {
      if (!isMounted() || await logoutIfAccessDenied(failure)) return;
      notify('AI connection failed.', { type: 'error' });
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
      notify(items.length === 0 ? 'The provider returned no models.' : `Loaded ${String(items.length)} models.`, { type: items.length === 0 ? 'info' : 'success' });
      return true;
    } catch (failure: unknown) {
      if (!isMounted() || await logoutIfAccessDenied(failure)) return false;
      notify('Available models could not be loaded.', { type: 'error' });
      return false;
    } finally {
      discoveringRef.current = false;
      if (isMounted()) setDiscoveringModelId(null);
    }
  };

  const remove = () => run('delete', async () => {
    if (settings?.revision === null || settings === null) return;
    try { await deleteAiSettings(settings.revision); if (isMounted()) { notify('AI settings removed.', { type: 'success' }); await load(); } }
    catch (failure: unknown) {
      if (await logoutIfAccessDenied(failure)) throw failure;
      if (isConflict(failure)) setConflict(true);
      notify('AI settings could not be removed.', { type: 'error' });
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
        actions={<Tooltip><Button aria-label="Reload AI settings" isIconOnly isPending={loading || analyticsLoading} onPress={() => { void Promise.all([load(), loadAnalytics()]); }} size="sm" variant="ghost"><RefreshCw aria-hidden="true" className="size-4" /></Button><Tooltip.Content>Reload AI settings</Tooltip.Content></Tooltip>}
        description="Configure the server-side provider, media policy, and models visible in the client."
        title="AI assistant"
      />
      <AiAnalyticsPanel analytics={analytics} error={analyticsError} loading={analyticsLoading} onRetry={() => { void loadAnalytics(); }} />
      {loading && settings === null ? <SettingsSkeleton /> : error !== null && settings === null ? <PageError error={error} headingLevel={2} onRetry={() => { void load(); }} /> : settings !== null ? (
        <Card>
          <Card.Header className="flex items-start gap-3 p-5 sm:p-6"><span className="grid size-9 place-items-center rounded-lg bg-accent text-accent-foreground"><Bot aria-hidden="true" className="size-5" /></span><div><Card.Title>Provider and policy</Card.Title><Card.Description>The API key stays on the server and is never returned after saving.</Card.Description></div></Card.Header>
          <Card.Content className="px-5 pb-6 sm:px-6"><fieldset className="space-y-6" disabled={locked}>
            {conflict && <Alert role="alert" status="warning"><Alert.Content><Alert.Title>Settings changed elsewhere</Alert.Title><Alert.Description>Reload the latest revision before saving this draft.</Alert.Description></Alert.Content><Button onPress={() => { void load(); }} size="sm" variant="tertiary">Reload latest</Button></Alert>}
            {error !== null && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>Settings could not be reloaded</Alert.Title><Alert.Description>The current draft is still shown. Reload before saving changes.</Alert.Description></Alert.Content><Button onPress={() => { void load(); }} size="sm" variant="tertiary">Reload</Button></Alert>}
            {!settings.encryptionAvailable && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>Credential encryption is unavailable</Alert.Title><Alert.Description>Configure TJXY_CREDENTIAL_KEYRING before saving provider settings.</Alert.Description></Alert.Content></Alert>}
            <div className="grid gap-4 lg:grid-cols-2">
              <TextField fullWidth><Label>OpenAI-compatible base URL</Label><Input autoComplete="off" fullWidth value={baseUrl} onChange={(event) => { setBaseUrl(event.currentTarget.value); }} placeholder="https://api.example.com/v1" /></TextField>
              <TextField fullWidth><Label>API key</Label><div className="relative"><Input autoComplete="new-password" className="pr-11" fullWidth type={showKey ? 'text' : 'password'} value={apiKey} onChange={(event) => { setApiKey(event.currentTarget.value); }} placeholder={settings.configured ? 'Leave blank to keep the saved key' : 'Enter API key'} /><Button aria-label={showKey ? 'Hide API key' : 'Show API key'} className="absolute right-1 top-1/2 -translate-y-1/2" isIconOnly onPress={() => { setShowKey((value) => !value); }} size="sm" variant="ghost">{showKey ? <EyeOff className="size-4" /> : <Eye className="size-4" />}</Button></div></TextField>
            </div>
            <TextField fullWidth><Label>System prompt</Label><TextArea className="min-h-32" value={systemPrompt} onChange={(event) => { setSystemPrompt(event.currentTarget.value); }} /></TextField>
            <div className="border-t border-border pt-5">
              <div className="mb-4 flex flex-wrap items-center justify-between gap-3"><div><h2 className="text-base font-semibold">Client models</h2><p className="text-sm text-muted">Display names are shown to users; upstream IDs stay in the administrator workspace.</p></div><Button onPress={() => { setModels((current) => [...current, newModel(current.length)]); }} size="sm" variant="secondary"><Plus className="size-4" />Add model</Button></div>
              <RadioGroup aria-label="默认模型" onChange={selectDefault} value={models.find((model) => model.isDefault)?.id ?? ''}>
                <div className="space-y-3">{models.map((model, index) => (
                <div className="rounded-lg border border-border p-4" key={model.id}>
                  <div className="grid items-end gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_12rem]">
                    <TextField fullWidth><Label>Display name</Label><Input fullWidth maxLength={128} value={model.displayName} onChange={(event) => { updateModel(model.id, { displayName: event.currentTarget.value }); }} /></TextField>
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
                      <Label>Upstream model ID</Label>
                      <ComboBox.InputGroup>
                        <Input fullWidth maxLength={255} />
                        <Tooltip>
                          <ModelDiscoveryButton
                            isDisabled={discoveringModelId !== null && discoveringModelId !== model.id}
                            isPending={discoveringModelId === model.id}
                            label={`Fetch available models for ${model.displayName || `model ${String(index + 1)}`}`}
                            onDiscover={() => discover(model.id)}
                          />
                          <Tooltip.Content>Fetch available models</Tooltip.Content>
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
                      <Button aria-label={`Move ${model.displayName || `model ${String(index + 1)}`} up`} isDisabled={index === 0} isIconOnly onPress={() => { moveModel(index, -1); }} size="sm" variant="ghost"><ArrowUp className="size-4" /></Button>
                      <Button aria-label={`Move ${model.displayName || `model ${String(index + 1)}`} down`} isDisabled={index === models.length - 1} isIconOnly onPress={() => { moveModel(index, 1); }} size="sm" variant="ghost"><ArrowDown className="size-4" /></Button>
                      <Button aria-label={`Remove ${model.displayName || `model ${String(index + 1)}`}`} isDisabled={models.length <= 1} isIconOnly onPress={() => { removeModel(model.id); }} variant="danger-soft"><Trash2 className="size-4" /></Button>
                    </div>
                  </div>
                </div>
                ))}</div>
              </RadioGroup>
            </div>
            <Switch isSelected={enabled} onChange={setEnabled}><Switch.Content><Switch.Control><Switch.Thumb /></Switch.Control>Enable the AI assistant</Switch.Content></Switch>
          </fieldset></Card.Content>
          <Card.Footer className="flex flex-wrap justify-between gap-3 border-t border-border p-5 sm:p-6"><ConfirmDialog confirmLabel="Remove settings" description="Delete the encrypted provider key, model aliases, and assistant policy. Existing user conversations are retained, but the assistant will be unavailable until it is configured again." errorDescription="The AI settings remain active. Reload the latest revision and try again." isPending={operation === 'delete'} onConfirm={remove} title="Remove AI assistant settings?" trigger={<Button isDisabled={!settings.configured || locked} variant="danger-soft"><Trash2 className="size-4" />Remove settings</Button>} /><div className="flex gap-2"><Button isDisabled={locked} isPending={operation === 'test'} onPress={() => { void test(); }} variant="secondary"><FlaskConical className="size-4" />Test connection</Button><Button isDisabled={locked || !settings.encryptionAvailable || error !== null || conflict} isPending={operation === 'save'} onPress={() => { void save(); }}><Save className="size-4" />Save settings</Button></div></Card.Footer>
        </Card>
      ) : null}
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
