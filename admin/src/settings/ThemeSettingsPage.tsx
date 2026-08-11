import {
  Alert,
  Button,
  Card,
  Description,
  Label,
  ListBox,
  Radio,
  RadioGroup,
  Select,
  Spinner,
  Switch,
} from '@heroui/react';
import { Palette, RefreshCw, RotateCcw, Save } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useNotify } from 'ra-core';
import { ApiError } from '../api/httpClient';
import {
  clientThemes,
  defaultClientTheme,
  findClientTheme,
} from '../client/themes/registry';
import type { ClientThemeDefinition, ThemeOptions } from '../client/themes/types';
import { PageHeader } from '../ui/PageHeader';
import { useTranslate } from './i18n';
import {
  getThemeSettings,
  saveThemeSettings,
  type ThemeSettings,
} from './themeSettingsApi';

type ThemeDrafts = Record<string, ThemeOptions>;

export function ThemeSettingsPage() {
  const tr = useTranslate();
  const notify = useNotify();
  const [settings, setSettings] = useState<ThemeSettings | null>(null);
  const [selectedThemeId, setSelectedThemeId] = useState(defaultClientTheme.id);
  const [drafts, setDrafts] = useState<ThemeDrafts>(() => defaultDrafts());
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [loadFailed, setLoadFailed] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [unavailableThemeId, setUnavailableThemeId] = useState<string | null>(null);

  const applySettings = useCallback((value: ThemeSettings) => {
    const activeDefinition = findClientTheme(value.activeThemeId);
    setSettings(value);
    setSelectedThemeId(activeDefinition?.id ?? defaultClientTheme.id);
    setUnavailableThemeId(activeDefinition === undefined ? value.activeThemeId : null);
    setDrafts(Object.fromEntries(clientThemes.map((definition) => {
      const configuration = value.configurations.find(({ themeId }) => themeId === definition.id);
      return [definition.id, configuration === undefined
        ? { ...definition.defaultOptions }
        : definition.normalizeOptions(configuration.schemaVersion, configuration.options)];
    })));
    setConflict(false);
  }, []);

  const requestSettings = useCallback((signal: AbortSignal) => getThemeSettings(signal)
      .then(applySettings)
      .catch(() => { if (!signal.aborted) setLoadFailed(true); })
      .finally(() => { if (!signal.aborted) setLoading(false); }), [applySettings]);

  const load = useCallback(() => {
    const controller = new AbortController();
    setLoading(true);
    setLoadFailed(false);
    void requestSettings(controller.signal);
  }, [requestSettings]);

  useEffect(() => {
    const controller = new AbortController();
    void requestSettings(controller.signal);
    return () => { controller.abort(); };
  }, [requestSettings]);

  const selectedDefinition = findClientTheme(selectedThemeId) ?? defaultClientTheme;
  const selectedOptions = drafts[selectedDefinition.id] ?? selectedDefinition.defaultOptions;
  const patchOption = (key: string, value: string | boolean) => {
    setDrafts((current) => ({
      ...current,
      [selectedDefinition.id]: { ...selectedOptions, [key]: value },
    }));
    setConflict(false);
  };
  const reset = () => {
    setDrafts((current) => ({
      ...current,
      [selectedDefinition.id]: { ...selectedDefinition.defaultOptions },
    }));
    setConflict(false);
  };
  const save = async () => {
    if (settings === null || conflict) return;
    setSaving(true);
    try {
      const saved = await saveThemeSettings({
        themeId: selectedDefinition.id,
        schemaVersion: selectedDefinition.schemaVersion,
        options: selectedDefinition.normalizeOptions(
          selectedDefinition.schemaVersion,
          selectedOptions,
        ),
      }, settings.revision);
      applySettings(saved);
      const active = saved.configurations.find(({ themeId }) => themeId === saved.activeThemeId);
      if (active !== undefined) {
        window.dispatchEvent(new CustomEvent('tjxy-site-theme', { detail: {
          id: active.themeId,
          schemaVersion: active.schemaVersion,
          options: active.options,
          revision: saved.revision,
        } }));
      }
      notify(tr('admin.theme.saved'), { type: 'success' });
    } catch (error) {
      if (error instanceof ApiError && error.category === 'conflict') setConflict(true);
      else notify(tr('admin.theme.saveFailed'), { type: 'error' });
    } finally {
      setSaving(false);
    }
  };

  if (loading && settings === null) {
    return <div className="flex min-h-64 items-center justify-center"><Spinner aria-label={tr('admin.theme.loading')} /></div>;
  }
  return (
    <div className="space-y-5">
      <PageHeader
        actions={<Button aria-label={tr('admin.theme.reload')} isIconOnly isPending={loading} onPress={load} size="sm" variant="ghost"><RefreshCw className="size-4" /></Button>}
        description={tr('admin.theme.subtitle')}
        title={tr('admin.theme.title')}
      />
      {loadFailed && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('admin.theme.loadFailed')}</Alert.Title><Alert.Description>{tr('admin.theme.loadFailedDescription')}</Alert.Description></Alert.Content><Button onPress={load} size="sm" variant="secondary">{tr('admin.theme.retry')}</Button></Alert>}
      {unavailableThemeId !== null && <Alert role="alert" status="warning"><Alert.Content><Alert.Title>{tr('admin.theme.unavailable')}</Alert.Title><Alert.Description>{tr('admin.theme.unavailableDescription')}</Alert.Description></Alert.Content></Alert>}
      {conflict && <Alert role="alert" status="warning"><Alert.Content><Alert.Title>{tr('admin.theme.conflict')}</Alert.Title><Alert.Description>{tr('admin.theme.conflictDescription')}</Alert.Description></Alert.Content><Button onPress={load} size="sm" variant="secondary">{tr('admin.theme.reloadLatest')}</Button></Alert>}
      {settings !== null && (
        <Card>
          <Card.Header className="items-start gap-3"><Palette className="mt-0.5 size-5 shrink-0 text-accent" /><div><Card.Title>{tr('admin.theme.choose')}</Card.Title><Card.Description>{tr('admin.theme.chooseDescription')}</Card.Description></div></Card.Header>
          <Card.Content className="space-y-7">
            <RadioGroup aria-label={tr('admin.theme.choose')} className="grid gap-4 lg:grid-cols-2" onChange={(value) => { if (findClientTheme(value) !== undefined) { setSelectedThemeId(value); setConflict(false); } }} value={selectedThemeId}>
              {clientThemes.map((definition) => (
                <ThemeChoice definition={definition} key={definition.id} options={drafts[definition.id] ?? definition.defaultOptions} />
              ))}
            </RadioGroup>
            <div className="border-t border-border pt-6">
              <div className="mb-5"><h2 className="text-lg font-semibold text-foreground">{tr('admin.theme.customize')}</h2><p className="mt-1 text-sm text-muted">{tr('admin.theme.customizeDescription')}</p></div>
              <div className="grid gap-5 md:grid-cols-2">
                {selectedDefinition.optionFields.map((field) => field.kind === 'select' ? (
                  <Select key={field.key} value={String(selectedOptions[field.key] ?? '')} onChange={(value) => { if (typeof value === 'string') patchOption(field.key, value); }}>
                    <Label>{tr(field.labelKey)}</Label>
                    <Select.Trigger><Select.Value /><Select.Indicator /></Select.Trigger>
                    <Select.Popover><ListBox>{field.choices.map((choice) => <ListBox.Item id={choice.value} key={choice.value} textValue={tr(choice.labelKey)}>{tr(choice.labelKey)}<ListBox.ItemIndicator /></ListBox.Item>)}</ListBox></Select.Popover>
                    <Description>{tr(field.descriptionKey)}</Description>
                  </Select>
                ) : (
                  <Switch isSelected={selectedOptions[field.key] === true} key={field.key} onChange={(value) => { patchOption(field.key, value); }}>
                    <Switch.Content><Switch.Control><Switch.Thumb /></Switch.Control><span><span className="block text-sm font-medium">{tr(field.labelKey)}</span><span className="block text-xs text-muted">{tr(field.descriptionKey)}</span></span></Switch.Content>
                  </Switch>
                ))}
              </div>
            </div>
          </Card.Content>
          <Card.Footer className="flex flex-wrap justify-between gap-3 border-t border-border p-5 sm:p-6">
            <Button onPress={reset} variant="tertiary"><RotateCcw className="size-4" />{tr('admin.theme.restoreDefaults')}</Button>
            <Button isDisabled={loadFailed || conflict} isPending={saving} onPress={() => { void save(); }}><Save className="size-4" />{tr('admin.theme.save')}</Button>
          </Card.Footer>
        </Card>
      )}
    </div>
  );
}

function ThemeChoice({ definition, options }: { definition: ClientThemeDefinition; options: ThemeOptions }) {
  const tr = useTranslate();
  const Preview = definition.Preview;
  return (
    <Radio value={definition.id}>
      <Radio.Content className="group flex w-full items-start gap-3 border border-border bg-surface-secondary p-3 data-[selected=true]:border-accent">
        <Radio.Control className="mt-1 shrink-0"><Radio.Indicator /></Radio.Control>
        <div className="min-w-0 flex-1"><Preview options={options} /><p className="mt-3 font-medium text-foreground">{tr(definition.labelKey)}</p><Description>{tr(definition.descriptionKey)}</Description></div>
      </Radio.Content>
    </Radio>
  );
}

function defaultDrafts(): ThemeDrafts {
  return Object.fromEntries(clientThemes.map((definition) => [
    definition.id, { ...definition.defaultOptions },
  ]));
}
