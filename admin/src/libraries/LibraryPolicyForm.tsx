import {
  Alert,
  Button,
  Card,
  Chip,
  Description,
  Label,
  ListBox,
  Radio,
  RadioGroup,
  Select,
  Switch,
} from '@heroui/react';
import { LoaderCircle, RefreshCw, Save, TriangleAlert } from 'lucide-react';

import type {
  EffectiveLibraryPolicy,
  ExpansionPolicy,
  LibraryOption,
  LocalMetadataAccessMode,
  MetadataPolicy,
  MetadataSourceMode,
  ObjectSelectionScope,
  ProbePolicy,
  ScanProfile,
} from './libraryApi';
import {
  expansionPolicyOptions,
  metadataPolicyOptions,
  objectScopeOptions,
  optionLabel,
  probePolicyOptions,
  scanProfileOptions,
} from './libraryUi';
import { useTranslate } from '../settings/i18n';

export interface LibraryPolicyFormProps {
  advanced: boolean;
  enabled: boolean;
  hasConflict: boolean;
  isPending: boolean;
  library: LibraryOption;
  onAdvancedChange: (advanced: boolean) => void;
  onEnabledChange: (enabled: boolean) => void;
  onMetadataSourceModeChange: (mode: MetadataSourceMode) => void;
  onLocalMetadataAccessModeChange: (mode: LocalMetadataAccessMode) => void;
  onPolicyChange: (policy: EffectiveLibraryPolicy) => void;
  onProfileChange: (profile: ScanProfile) => void;
  onReloadLatest: () => void;
  onSave: () => void;
  policy: EffectiveLibraryPolicy;
  scanProfile: ScanProfile;
  metadataSourceMode: MetadataSourceMode;
  localMetadataAccessMode: LocalMetadataAccessMode;
}

const metadataSourceOptions: readonly {
  value: MetadataSourceMode;
  label: string;
  description: string;
}[] = [
  {
    value: 'automatic_scrape',
    label: 'Automatic scrape',
    description: 'Prefer local NFO and artwork, then use TMDB for missing fields.',
  },
  {
    value: 'local_only',
    label: 'Local metadata only',
    description: 'Index local NFO and artwork without remote provider requests.',
  },
];

export function LibraryPolicyForm({
  advanced,
  enabled,
  hasConflict,
  isPending,
  library,
  onAdvancedChange,
  onEnabledChange,
  onMetadataSourceModeChange,
  onLocalMetadataAccessModeChange,
  onPolicyChange,
  onProfileChange,
  onReloadLatest,
  onSave,
  policy,
  scanProfile,
  metadataSourceMode,
  localMetadataAccessMode,
}: LibraryPolicyFormProps) {
  const tr = useTranslate();
  return (
    <section aria-labelledby="scanning-policy-heading" className="border-t border-border py-7">
      <Card className="gap-0 overflow-hidden p-0" variant="secondary">
        <Card.Header className="border-b border-border p-5 sm:p-6">
          <div>
            <h2 className="text-base font-semibold text-foreground" id="scanning-policy-heading">{tr('Scanning policy', '扫描策略')}</h2>
            <p className="mt-1 text-sm text-muted">{tr('Choose how this library discovers and expands catalog content.', '选择此媒体库发现和扩展媒体内容的方式。')}</p>
          </div>
        </Card.Header>

        <Card.Content className="space-y-5 p-5 sm:p-6">
          {hasConflict && (
            <Alert role="alert" status="warning">
              <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
              <Alert.Content>
                <Alert.Title>{tr('Library settings changed elsewhere', '媒体库设置已在其他位置变更')}</Alert.Title>
                <Alert.Description>{tr('Your draft is intact. Reload the latest settings before saving again.', '草稿已保留，请加载最新设置后再保存。')}</Alert.Description>
              </Alert.Content>
              <Button isDisabled={isPending} onPress={onReloadLatest} size="sm" variant="tertiary">
                <RefreshCw aria-hidden="true" className="size-4" />
                {tr('Reload latest', '加载最新设置')}
              </Button>
            </Alert>
          )}

          <RadioGroup
            isDisabled={isPending}
            name="metadata-source-mode"
            onChange={(value) => {
              const mode = value as MetadataSourceMode;
              onMetadataSourceModeChange(mode);
              if (mode === 'automatic_scrape') onLocalMetadataAccessModeChange('import');
            }}
            value={metadataSourceMode}
          >
            <Label>{tr('Metadata source', '元数据来源')}</Label>
            <div className="grid gap-3">
              {metadataSourceOptions.map((option) => (
                <Radio
                  className="rounded-lg border border-border p-4 data-[selected=true]:border-accent data-[selected=true]:bg-accent/5"
                  key={option.value}
                  value={option.value}
                >
                  <Radio.Content className="w-full items-start gap-3">
                    <Radio.Control className="mt-0.5 shrink-0"><Radio.Indicator /></Radio.Control>
                    <span className="min-w-0">
                      <span className="block font-medium">{tr(option.label, option.value === 'automatic_scrape' ? '自动抓取' : '仅本地元数据')}</span>
                      <span className="mt-1 block text-sm text-muted">{tr(option.description, option.value === 'automatic_scrape' ? '优先使用本地 NFO 和图片，再使用 TMDB 补充缺失字段。' : '仅索引本地 NFO 和图片，不请求远程服务商。')}</span>
                    </span>
                  </Radio.Content>
                </Radio>
              ))}
            </div>
          </RadioGroup>
          {metadataSourceMode === 'local_only' && (
            <RadioGroup isDisabled={isPending} name="local-metadata-access-mode" onChange={(value) => { onLocalMetadataAccessModeChange(value as LocalMetadataAccessMode); }} value={localMetadataAccessMode}>
              <Label>{tr('Local metadata access', '本地元数据访问')}</Label>
              <div className="grid gap-3 sm:grid-cols-2">
                <Radio className="rounded-lg border border-border p-4 data-[selected=true]:border-accent data-[selected=true]:bg-accent/5" value="import"><Radio.Content className="w-full items-start gap-3"><Radio.Control className="mt-0.5"><Radio.Indicator /></Radio.Control><span><span className="block font-medium">{tr('Import', '导入')}</span><span className="mt-1 block text-sm text-muted">{tr('Persist parsed metadata and copied artwork.', '保存解析后的元数据与复制的图片。')}</span></span></Radio.Content></Radio>
                <Radio className="rounded-lg border border-border p-4 data-[selected=true]:border-accent data-[selected=true]:bg-accent/5" value="direct"><Radio.Content className="w-full items-start gap-3"><Radio.Control className="mt-0.5"><Radio.Indicator /></Radio.Control><span><span className="block font-medium">{tr('Direct', '直接读取')}</span><span className="mt-1 block text-sm text-muted">{tr('Use NFO and artwork in place without copying bytes.', '原位使用 NFO 和图片，不复制文件内容。')}</span></span></Radio.Content></Radio>
              </div>
            </RadioGroup>
          )}

          <Switch
            aria-label="Enabled"
            className="w-full"
            isDisabled={isPending}
            isSelected={enabled}
            onChange={onEnabledChange}
          >
            <Switch.Content className="flex w-full items-center justify-between gap-4 rounded-lg border border-border p-4">
              <span>
                <span className="block font-medium text-foreground">{tr('Enabled', '已启用')}</span>
                <span className="mt-1 block text-sm text-muted">{tr('Allow scheduled and manual scans for this library.', '允许对此媒体库执行计划和手动扫描。')}</span>
              </span>
              <Switch.Control className="shrink-0"><Switch.Thumb /></Switch.Control>
            </Switch.Content>
          </Switch>

          <div className="space-y-3 rounded-lg border border-border p-4">
            <PolicySelect
              description={tr('Select the default discovery and expansion behavior.', '选择默认的发现和扩展行为。')}
              isDisabled={isPending}
              label={tr('Scan profile', '扫描配置')}
              onChange={onProfileChange}
              options={scanProfileOptions}
              value={scanProfile}
            />
          </div>

          <Switch
            aria-label={tr('Override effective policy', '覆盖生效策略')}
            className="w-full"
            isDisabled={isPending}
            isSelected={advanced}
            onChange={onAdvancedChange}
          >
            <Switch.Content className="flex w-full items-center justify-between gap-4 rounded-lg border border-border p-4">
              <span>
                <span className="block font-medium text-foreground">{tr('Override effective policy', '覆盖生效策略')}</span>
                <span className="mt-1 block text-sm text-muted">{tr('Customize all effective policy values in one versioned update.', '在一次带版本的更新中自定义全部生效策略。')}</span>
              </span>
              <Switch.Control className="shrink-0"><Switch.Thumb /></Switch.Control>
            </Switch.Content>
          </Switch>

          {advanced ? (
            <section aria-labelledby="advanced-policy-heading" className="space-y-4 rounded-lg bg-surface-secondary p-4 sm:p-5">
              <div>
                <h3 className="font-semibold text-foreground" id="advanced-policy-heading">{tr('Advanced overrides', '高级覆盖设置')}</h3>
                <p className="mt-1 text-sm text-muted">{tr('These values replace the selected profile defaults.', '这些值会替换所选配置的默认值。')}</p>
              </div>
              <div className="grid gap-4">
                <PolicySelect
                  isDisabled={isPending}
                  label={tr('Object selection', '对象选择')}
                  onChange={(objectSelectionScope) => { onPolicyChange({ ...policy, objectSelectionScope }); }}
                  options={objectScopeOptions}
                  value={policy.objectSelectionScope}
                />
                <PolicySelect
                  isDisabled={isPending}
                  label={tr('Metadata', '元数据')}
                  onChange={(metadataPolicy) => { onPolicyChange({ ...policy, metadataPolicy }); }}
                  options={metadataPolicyOptions}
                  value={policy.metadataPolicy}
                />
                <PolicySelect
                  isDisabled={isPending}
                  label={tr('Expansion', '扩展')}
                  onChange={(expansionPolicy) => { onPolicyChange({ ...policy, expansionPolicy }); }}
                  options={expansionPolicyOptions}
                  value={policy.expansionPolicy}
                />
                <PolicySelect
                  isDisabled={isPending}
                  label={tr('Media probe', '媒体探测')}
                  onChange={(probePolicy) => { onPolicyChange({ ...policy, probePolicy }); }}
                  options={probePolicyOptions}
                  value={policy.probePolicy}
                />
              </div>
            </section>
          ) : (
            <div className="rounded-lg bg-surface-secondary p-4">
              <p className="font-medium text-foreground">{tr('Current effective policy', '当前生效策略')}</p>
              <dl aria-label={tr('Effective policy summary', '生效策略摘要')} className="mt-3 grid gap-3 sm:grid-cols-2">
                <PolicySummary label={tr('Object selection', '对象选择')} value={optionLabel(objectScopeOptions, library.objectSelectionScope)} />
                <PolicySummary label={tr('Metadata', '元数据')} value={optionLabel(metadataPolicyOptions, library.metadataPolicy)} />
                <PolicySummary label={tr('Expansion', '扩展')} value={optionLabel(expansionPolicyOptions, library.expansionPolicy)} />
                <PolicySummary label={tr('Media probe', '媒体探测')} value={optionLabel(probePolicyOptions, library.probePolicy)} />
              </dl>
            </div>
          )}
        </Card.Content>

        <Card.Footer className="justify-end border-t border-border p-5 sm:p-6">
          <Button className="min-w-40" isDisabled={hasConflict} isPending={isPending} onPress={onSave}>
            {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <Save aria-hidden="true" className="size-4" />}
            <span className="inline-flex min-h-5 items-center">{tr('Save scan policy', '保存扫描策略')}</span>
          </Button>
        </Card.Footer>
      </Card>
    </section>
  );
}

function PolicySummary({ label, value }: { label: string; value: string }) {
  const tr = useTranslate();
  return (
    <div className="flex min-w-0 items-center justify-between gap-3">
      <dt className="text-sm text-muted">{label}</dt>
      <dd><Chip size="sm" variant="secondary">{translatePolicyValue(tr, value)}</Chip></dd>
    </div>
  );
}

function PolicySelect<T extends ScanProfile | ObjectSelectionScope | MetadataPolicy | ExpansionPolicy | ProbePolicy>({
  description,
  isDisabled,
  label,
  onChange,
  options,
  value,
}: {
  description?: string;
  isDisabled: boolean;
  label: string;
  onChange: (value: T) => void;
  options: readonly { value: T; label: string }[];
  value: T;
}) {
  const tr = useTranslate();
  return (
    <Select
      fullWidth
      isDisabled={isDisabled}
      onChange={(key) => { if (typeof key === 'string') onChange(key as T); }}
      value={value}
    >
      <Label>{label}</Label>
      {description !== undefined && <Description>{description}</Description>}
      <Select.Trigger>
        <Select.Value />
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox>
          {options.map((option) => (
            <ListBox.Item id={option.value} key={option.value} textValue={tr(option.label, translatePolicyValue(tr, option.label))}>
              {tr(option.label, translatePolicyValue(tr, option.label))}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}

function translatePolicyValue(tr: ReturnType<typeof useTranslate>, value: string): string {
  const labels: Record<string, string> = {
    Full: '完整', Lazy: '懒加载', Manual: '手动', All: '全部',
    AllSyncedObjects: '全部已同步对象', TitleLayer: '标题层', LibraryRoots: '媒体库根目录',
    FullMetadata: '完整元数据', BasicMetadata: '基础元数据', NoMetadata: '无元数据',
    Eager: '积极', OnBrowse: '浏览时', OnPlayback: '播放时',
    'All synced objects': '全部已同步对象', 'Title layer': '标题层', 'Library roots': '媒体库根目录',
    'Full metadata': '完整元数据', 'Basic metadata': '基础元数据', 'No metadata': '无元数据',
    'On browse': '浏览时', 'On playback': '播放时',
  };
  return tr(value, labels[value] ?? value);
}
