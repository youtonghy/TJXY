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

export interface LibraryPolicyFormProps {
  advanced: boolean;
  enabled: boolean;
  hasConflict: boolean;
  isPending: boolean;
  library: LibraryOption;
  onAdvancedChange: (advanced: boolean) => void;
  onEnabledChange: (enabled: boolean) => void;
  onMetadataSourceModeChange: (mode: MetadataSourceMode) => void;
  onPolicyChange: (policy: EffectiveLibraryPolicy) => void;
  onProfileChange: (profile: ScanProfile) => void;
  onReloadLatest: () => void;
  onSave: () => void;
  policy: EffectiveLibraryPolicy;
  scanProfile: ScanProfile;
  metadataSourceMode: MetadataSourceMode;
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
  onPolicyChange,
  onProfileChange,
  onReloadLatest,
  onSave,
  policy,
  scanProfile,
  metadataSourceMode,
}: LibraryPolicyFormProps) {
  return (
    <section aria-labelledby="scanning-policy-heading" className="border-t border-border py-7">
      <Card className="gap-0 overflow-hidden p-0" variant="secondary">
        <Card.Header className="border-b border-border p-5 sm:p-6">
          <div>
            <h2 className="text-base font-semibold text-foreground" id="scanning-policy-heading">Scanning policy</h2>
            <p className="mt-1 text-sm text-muted">Choose how this library discovers and expands catalog content.</p>
          </div>
        </Card.Header>

        <Card.Content className="space-y-5 p-5 sm:p-6">
          {hasConflict && (
            <Alert role="alert" status="warning">
              <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
              <Alert.Content>
                <Alert.Title>Library settings changed elsewhere</Alert.Title>
                <Alert.Description>Your draft is intact. Reload the latest settings before saving again.</Alert.Description>
              </Alert.Content>
              <Button isDisabled={isPending} onPress={onReloadLatest} size="sm" variant="tertiary">
                <RefreshCw aria-hidden="true" className="size-4" />
                Reload latest
              </Button>
            </Alert>
          )}

          <RadioGroup
            isDisabled={isPending}
            name="metadata-source-mode"
            onChange={(value) => { onMetadataSourceModeChange(value as MetadataSourceMode); }}
            value={metadataSourceMode}
          >
            <Label>Metadata source</Label>
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
                      <span className="block font-medium">{option.label}</span>
                      <span className="mt-1 block text-sm text-muted">{option.description}</span>
                    </span>
                  </Radio.Content>
                </Radio>
              ))}
            </div>
          </RadioGroup>

          <Switch
            aria-label="Enabled"
            className="w-full"
            isDisabled={isPending}
            isSelected={enabled}
            onChange={onEnabledChange}
          >
            <Switch.Content className="flex w-full items-center justify-between gap-4 rounded-lg border border-border p-4">
              <span>
                <span className="block font-medium text-foreground">Enabled</span>
                <span className="mt-1 block text-sm text-muted">Allow scheduled and manual scans for this library.</span>
              </span>
              <Switch.Control className="shrink-0"><Switch.Thumb /></Switch.Control>
            </Switch.Content>
          </Switch>

          <div className="space-y-3 rounded-lg border border-border p-4">
            <PolicySelect
              description="Select the default discovery and expansion behavior."
              isDisabled={isPending}
              label="Scan profile"
              onChange={onProfileChange}
              options={scanProfileOptions}
              value={scanProfile}
            />
          </div>

          <Switch
            aria-label="Override effective policy"
            className="w-full"
            isDisabled={isPending}
            isSelected={advanced}
            onChange={onAdvancedChange}
          >
            <Switch.Content className="flex w-full items-center justify-between gap-4 rounded-lg border border-border p-4">
              <span>
                <span className="block font-medium text-foreground">Override effective policy</span>
                <span className="mt-1 block text-sm text-muted">Customize all effective policy values in one versioned update.</span>
              </span>
              <Switch.Control className="shrink-0"><Switch.Thumb /></Switch.Control>
            </Switch.Content>
          </Switch>

          {advanced ? (
            <section aria-labelledby="advanced-policy-heading" className="space-y-4 rounded-lg bg-surface-secondary p-4 sm:p-5">
              <div>
                <h3 className="font-semibold text-foreground" id="advanced-policy-heading">Advanced overrides</h3>
                <p className="mt-1 text-sm text-muted">These values replace the selected profile defaults.</p>
              </div>
              <div className="grid gap-4">
                <PolicySelect
                  isDisabled={isPending}
                  label="Object selection"
                  onChange={(objectSelectionScope) => { onPolicyChange({ ...policy, objectSelectionScope }); }}
                  options={objectScopeOptions}
                  value={policy.objectSelectionScope}
                />
                <PolicySelect
                  isDisabled={isPending}
                  label="Metadata"
                  onChange={(metadataPolicy) => { onPolicyChange({ ...policy, metadataPolicy }); }}
                  options={metadataPolicyOptions}
                  value={policy.metadataPolicy}
                />
                <PolicySelect
                  isDisabled={isPending}
                  label="Expansion"
                  onChange={(expansionPolicy) => { onPolicyChange({ ...policy, expansionPolicy }); }}
                  options={expansionPolicyOptions}
                  value={policy.expansionPolicy}
                />
                <PolicySelect
                  isDisabled={isPending}
                  label="Media probe"
                  onChange={(probePolicy) => { onPolicyChange({ ...policy, probePolicy }); }}
                  options={probePolicyOptions}
                  value={policy.probePolicy}
                />
              </div>
            </section>
          ) : (
            <div className="rounded-lg bg-surface-secondary p-4">
              <p className="font-medium text-foreground">Current effective policy</p>
              <dl aria-label="Effective policy summary" className="mt-3 grid gap-3 sm:grid-cols-2">
                <PolicySummary label="Object selection" value={optionLabel(objectScopeOptions, library.objectSelectionScope)} />
                <PolicySummary label="Metadata" value={optionLabel(metadataPolicyOptions, library.metadataPolicy)} />
                <PolicySummary label="Expansion" value={optionLabel(expansionPolicyOptions, library.expansionPolicy)} />
                <PolicySummary label="Media probe" value={optionLabel(probePolicyOptions, library.probePolicy)} />
              </dl>
            </div>
          )}
        </Card.Content>

        <Card.Footer className="justify-end border-t border-border p-5 sm:p-6">
          <Button className="min-w-40" isDisabled={hasConflict} isPending={isPending} onPress={onSave}>
            {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <Save aria-hidden="true" className="size-4" />}
            <span className="inline-flex min-h-5 items-center">Save scan policy</span>
          </Button>
        </Card.Footer>
      </Card>
    </section>
  );
}

function PolicySummary({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex min-w-0 items-center justify-between gap-3">
      <dt className="text-sm text-muted">{label}</dt>
      <dd><Chip size="sm" variant="secondary">{value}</Chip></dd>
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
            <ListBox.Item id={option.value} key={option.value} textValue={option.label}>
              {option.label}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}
