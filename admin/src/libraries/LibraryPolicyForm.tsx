import {
  Alert,
  Button,
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
    <section aria-labelledby="scanning-policy-heading" className="space-y-5 border-t border-border py-7">
      <div>
        <h2 className="text-base font-semibold text-foreground" id="scanning-policy-heading">Scanning policy</h2>
        <p className="mt-1 text-sm text-muted">Choose how this library discovers and expands catalog content.</p>
      </div>

      <RadioGroup
        isDisabled={isPending}
        onChange={(value) => { onMetadataSourceModeChange(value as MetadataSourceMode); }}
        value={metadataSourceMode}
      >
        <Label>Metadata source</Label>
        <div className="grid gap-3 md:grid-cols-2">
          <Radio value="automatic_scrape">
            <Radio.Control><Radio.Indicator /></Radio.Control>
            <Radio.Content>
              <span className="font-medium">Automatic scrape</span>
              <span className="text-sm text-muted">Prefer local NFO and artwork, then use TMDB for missing fields.</span>
            </Radio.Content>
          </Radio>
          <Radio value="local_only">
            <Radio.Control><Radio.Indicator /></Radio.Control>
            <Radio.Content>
              <span className="font-medium">Local metadata only</span>
              <span className="text-sm text-muted">Index local NFO and artwork without remote provider requests.</span>
            </Radio.Content>
          </Radio>
        </div>
      </RadioGroup>

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

      <div className="grid gap-5 md:grid-cols-2">
        <Switch isDisabled={isPending} isSelected={enabled} onChange={onEnabledChange}>
          <Switch.Content>
            <Switch.Control><Switch.Thumb /></Switch.Control>
            Enabled
          </Switch.Content>
        </Switch>
        <PolicySelect
          isDisabled={isPending}
          label="Scan profile"
          onChange={onProfileChange}
          options={scanProfileOptions}
          value={scanProfile}
        />
      </div>

      <Switch isDisabled={isPending} isSelected={advanced} onChange={onAdvancedChange}>
        <Switch.Content>
          <Switch.Control><Switch.Thumb /></Switch.Control>
          Override effective policy
        </Switch.Content>
      </Switch>

      {advanced ? (
        <section aria-labelledby="advanced-policy-heading" className="space-y-4 border-l-2 border-accent/35 pl-4">
          <div>
            <h3 className="font-semibold text-foreground" id="advanced-policy-heading">Advanced overrides</h3>
            <p className="mt-1 text-sm text-muted">Override all effective policy values as one versioned update.</p>
          </div>
          <div className="grid gap-4 sm:grid-cols-2">
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
        <div className="border-y border-border py-4 text-sm">
          <p className="font-medium text-foreground">Current effective policy</p>
          <p className="mt-1 text-muted">
            {optionLabel(objectScopeOptions, library.objectSelectionScope)} /{' '}
            {optionLabel(metadataPolicyOptions, library.metadataPolicy)} /{' '}
            {optionLabel(expansionPolicyOptions, library.expansionPolicy)} /{' '}
            {optionLabel(probePolicyOptions, library.probePolicy)}
          </p>
        </div>
      )}

      <div className="flex justify-end">
        <Button className="min-w-40" isDisabled={hasConflict} isPending={isPending} onPress={onSave}>
          {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <Save aria-hidden="true" className="size-4" />}
          <span className="inline-flex min-h-5 items-center">Save scan policy</span>
        </Button>
      </div>
    </section>
  );
}

function PolicySelect<T extends ScanProfile | ObjectSelectionScope | MetadataPolicy | ExpansionPolicy | ProbePolicy>({
  isDisabled,
  label,
  onChange,
  options,
  value,
}: {
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
