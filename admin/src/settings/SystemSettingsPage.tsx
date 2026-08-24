/* eslint-disable react-hooks/set-state-in-effect */
import {
  Alert,
  Button,
  Card,
  FieldError,
  Input,
  Label,
  ListBox,
  Select,
  Spinner,
  Switch,
  TextField,
} from '@heroui/react';
import {
  KeyRound,
  FolderCog,
  Image as ImageIcon,
  Palette,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Server,
  Trash2,
  TriangleAlert,
  Upload,
} from 'lucide-react';
import { useCallback, useEffect, useState, type ChangeEvent } from 'react';
import { useNotify } from 'ra-core';

import { PageHeader } from '../ui/PageHeader';
import { interpolate, useTranslate } from './i18n';
import { useSystemLocale } from './SystemLocaleProvider';
import {
  getSystemSettings,
  restartSystem,
  saveSystemSettings,
  uploadBrandAsset,
  type SystemSettings,
} from './systemSettingsApi';

const initialSettings: SystemSettings = {
  locale: 'zh-CN',
  siteTitle: 'TJXY',
  siteSubtitle: 'Your media library',
  logoUrl: '/brand/tjxy-mark.webp',
  iconUrl: '/brand/favicon.svg',
  publicUrl: '',
  listenHost: '127.0.0.1',
  port: 8096,
  mediaBrowserRoots: [],
  invalidMediaBrowserRootIndexes: [],
  passkeyEnabled: false,
  revision: 0,
  restartRequired: false,
  environmentOverrides: {
    siteTitle: false,
    publicUrl: false,
    listenAddress: false,
    mediaBrowserRoots: false,
  },
  theme: { id: 'classic', schemaVersion: 1, options: {}, revision: 0 },
};

export function SystemSettingsPage() {
  const notify = useNotify();
  const tr = useTranslate();
  const { setLocale } = useSystemLocale();
  const [settings, setSettings] = useState<SystemSettings>(initialSettings);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [uploading, setUploading] = useState<'logo' | 'icon' | null>(null);

  const load = useCallback(() => {
    setLoading(true);
    void getSystemSettings()
      .then((loaded) => {
        setSettings(loaded);
        if (loaded.invalidMediaBrowserRootIndexes.length > 0) {
          notify(tr('admin.system.mediaBrowserRootsUnavailable'), { type: 'warning' });
        }
      })
      .catch(() => { notify(tr('admin.system.saveFailed'), { type: 'error' }); })
      .finally(() => { setLoading(false); });
  }, [notify, tr]);

  useEffect(load, [load]);

  const patch = <Key extends keyof SystemSettings>(key: Key, value: SystemSettings[Key]) => {
    setSettings((current) => ({ ...current, [key]: value }));
  };

  const save = () => {
    setSaving(true);
    void saveSystemSettings({
      ...settings,
      mediaBrowserRoots: settings.mediaBrowserRoots.map((root) => root.trim()).filter(Boolean),
    })
      .then((saved) => {
        setSettings(saved);
        setLocale(saved.locale);
        window.dispatchEvent(new CustomEvent('tjxy-system-settings', { detail: saved }));
        if (saved.restartRequired) {
          notify(tr('admin.system.restartReminder'), {
            type: 'warning',
            autoHideDuration: 8000,
          });
        } else {
          notify(tr('admin.system.saved'), { type: 'success' });
        }
      })
      .catch(() => { notify(tr('admin.system.saveFailed'), { type: 'error' }); })
      .finally(() => { setSaving(false); });
  };

  const upload = (kind: 'logo' | 'icon', event: ChangeEvent<HTMLInputElement>) => {
    const input = event.currentTarget;
    const file = input.files?.[0];
    if (file === undefined) return;
    setUploading(kind);
    void uploadBrandAsset(kind, file)
      .then(({ url }) => { patch(kind === 'logo' ? 'logoUrl' : 'iconUrl', url); })
      .catch(() => { notify(tr('admin.system.uploadFailed'), { type: 'error' }); })
      .finally(() => { setUploading(null); input.value = ''; });
  };

  const restart = () => {
    setRestarting(true);
    void restartSystem()
      .catch(() => { notify(tr('admin.system.restartFailed'), { type: 'error' }); })
      .finally(() => { setRestarting(false); });
  };

  return (
    <div className="space-y-5">
      <PageHeader
        actions={(
          <Button
            aria-label={tr('admin.system.reload')}
            isIconOnly
            isPending={loading}
            onPress={load}
            size="sm"
            variant="ghost"
          >
            <RefreshCw className="size-4" />
          </Button>
        )}
        description={tr('admin.system.subtitle')}
        title={tr('admin.system.title')}
      />

      {settings.restartRequired && (
        <Alert status="warning">
          <Alert.Indicator><RotateCcw className="size-4" /></Alert.Indicator>
          <Alert.Content>
            <Alert.Title>{tr('admin.system.restartRequired')}</Alert.Title>
            <Alert.Description>{tr('admin.system.restartRequiredDescription')}</Alert.Description>
          </Alert.Content>
        </Alert>
      )}

      <Card>
        <Card.Header className="items-start gap-3">
          <Palette className="mt-0.5 size-5 shrink-0 text-accent" />
          <div><Card.Title>{tr('admin.system.branding')}</Card.Title><Card.Description>{tr('admin.system.brandingDescription')}</Card.Description></div>
        </Card.Header>
        <Card.Content className="grid gap-5 md:grid-cols-2">
          <TextField fullWidth isDisabled={settings.environmentOverrides.siteTitle}>
            <Label>{tr('admin.system.siteTitle')}</Label>
            <Input value={settings.siteTitle} onChange={(event) => { patch('siteTitle', event.currentTarget.value); }} />
          </TextField>
          <TextField fullWidth>
            <Label>{tr('admin.system.siteSubtitle')}</Label>
            <Input value={settings.siteSubtitle} onChange={(event) => { patch('siteSubtitle', event.currentTarget.value); }} />
          </TextField>
          <Select value={settings.locale} onChange={(value) => {
            if (value === 'zh-CN' || value === 'en-US') patch('locale', value);
          }}>
            <Label>{tr('admin.system.language')}</Label>
            <Select.Trigger><Select.Value /><Select.Indicator /></Select.Trigger>
            <Select.Popover><ListBox>
              <ListBox.Item id="zh-CN" textValue="中文">中文<ListBox.ItemIndicator /></ListBox.Item>
              <ListBox.Item id="en-US" textValue="English">English<ListBox.ItemIndicator /></ListBox.Item>
            </ListBox></Select.Popover>
          </Select>
          {settings.environmentOverrides.siteTitle && <p className="self-end text-xs text-warning">{tr('admin.system.environmentOverride')}</p>}
          <BrandAssetField
            alt={tr('admin.system.logo')}
            inputLabel={tr('admin.system.logoUrl')}
            isUploading={uploading === 'logo'}
            onChange={(value) => { patch('logoUrl', value); }}
            onUpload={(event) => { upload('logo', event); }}
            uploadLabel={tr('admin.system.uploadLogo')}
            value={settings.logoUrl}
          />
          <BrandAssetField
            alt={tr('admin.system.icon')}
            inputLabel={tr('admin.system.iconUrl')}
            isUploading={uploading === 'icon'}
            onChange={(value) => { patch('iconUrl', value); }}
            onUpload={(event) => { upload('icon', event); }}
            uploadLabel={tr('admin.system.uploadIcon')}
            value={settings.iconUrl}
          />
        </Card.Content>
      </Card>

      <Card>
        <Card.Header className="items-start gap-3">
          <KeyRound className="mt-0.5 size-5 shrink-0 text-accent" />
          <div>
            <Card.Title>{tr('admin.system.passkeyTitle')}</Card.Title>
            <Card.Description>{tr('admin.system.passkeyDescription')}</Card.Description>
          </div>
        </Card.Header>
        <Card.Content className="pt-0">
          <Switch
            aria-label={tr('admin.system.passkeyEnabled')}
            className="w-full"
            isSelected={settings.passkeyEnabled}
            onChange={(selected) => { patch('passkeyEnabled', selected); }}
          >
            <Switch.Content className="flex w-full items-center justify-between gap-4 rounded-lg border border-border bg-surface-secondary p-4 transition-colors hover:bg-default sm:p-5">
              <span className="min-w-0">
                <span className="block font-medium text-foreground">
                  {settings.passkeyEnabled ? tr('admin.system.passkeyStatusEnabled') : tr('admin.system.passkeyStatusDisabled')}
                </span>
                <span className="mt-1 block text-sm leading-5 text-muted">{tr('admin.system.passkeyHint')}</span>
              </span>
              <Switch.Control className="shrink-0"><Switch.Thumb /></Switch.Control>
            </Switch.Content>
          </Switch>
        </Card.Content>
      </Card>

      <Card>
        <Card.Header className="items-start gap-3">
          <Server className="mt-0.5 size-5 shrink-0 text-accent" />
          <div><Card.Title>{tr('admin.system.network')}</Card.Title><Card.Description>{tr('admin.system.networkDescription')}</Card.Description></div>
        </Card.Header>
        <Card.Content className="grid gap-5 md:grid-cols-2">
          <TextField fullWidth isDisabled={settings.environmentOverrides.listenAddress}>
            <Label>{tr('admin.system.listenHost')}</Label>
            <Input value={settings.listenHost} onChange={(event) => { patch('listenHost', event.currentTarget.value); }} />
          </TextField>
          <TextField fullWidth isDisabled={settings.environmentOverrides.listenAddress}>
            <Label>{tr('admin.system.port')}</Label>
            <Input inputMode="numeric" min={1} max={65535} type="number" value={String(settings.port)} onChange={(event) => {
              patch('port', Number(event.currentTarget.value));
            }} />
          </TextField>
          <TextField className="md:col-span-2" fullWidth isDisabled={settings.environmentOverrides.publicUrl}>
            <Label>{tr('admin.system.publicUrl')}</Label>
            <Input placeholder="https://media.example.com" type="url" value={settings.publicUrl} onChange={(event) => { patch('publicUrl', event.currentTarget.value); }} />
          </TextField>
          {(settings.environmentOverrides.listenAddress || settings.environmentOverrides.publicUrl) && (
            <p className="text-xs text-warning md:col-span-2">{tr('admin.system.environmentOverride')}</p>
          )}
        </Card.Content>
      </Card>

      <Card>
        <Card.Header className="items-start gap-3">
          <FolderCog className="mt-0.5 size-5 shrink-0 text-accent" />
          <div><Card.Title>{tr('admin.system.mediaBrowser')}</Card.Title><Card.Description>{tr('admin.system.mediaBrowserDescription')}</Card.Description></div>
        </Card.Header>
        <Card.Content className="space-y-3">
          {settings.invalidMediaBrowserRootIndexes.length > 0 && (
            <Alert role="alert" status="warning"><Alert.Indicator><TriangleAlert className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{tr('admin.system.mediaBrowserRootsUnavailable')}</Alert.Title><Alert.Description>{tr('admin.system.mediaBrowserRootsUnavailableDescription')}</Alert.Description></Alert.Content></Alert>
          )}
          {settings.mediaBrowserRoots.map((root, index) => (
            <div className="flex items-end gap-2" key={`media-browser-root-${String(index)}`}>
              <TextField className="min-w-0 flex-1" fullWidth isDisabled={settings.environmentOverrides.mediaBrowserRoots} isInvalid={settings.invalidMediaBrowserRootIndexes.includes(index)}>
                <Label>{interpolate(tr('admin.system.mediaBrowserRoot'), { index: String(index + 1) })}</Label>
                <Input placeholder="/srv/media" value={root} onChange={(event) => { const next = [...settings.mediaBrowserRoots]; next[index] = event.currentTarget.value; patch('mediaBrowserRoots', next); }} />
                {settings.invalidMediaBrowserRootIndexes.includes(index) && <FieldError>{tr('admin.system.mediaBrowserRootUnavailable')}</FieldError>}
              </TextField>
              <Button aria-label={interpolate(tr('admin.system.removeMediaBrowserRoot'), { index: String(index + 1) })} isDisabled={settings.environmentOverrides.mediaBrowserRoots} isIconOnly onPress={() => { patch('mediaBrowserRoots', settings.mediaBrowserRoots.filter((_, itemIndex) => itemIndex !== index)); }} variant="ghost"><Trash2 className="size-4" /></Button>
            </div>
          ))}
          <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border pt-4">
            <p className="text-xs text-muted">{tr('admin.system.mediaBrowserHint')}</p>
            <Button isDisabled={settings.environmentOverrides.mediaBrowserRoots} onPress={() => { patch('mediaBrowserRoots', [...settings.mediaBrowserRoots, '']); }} size="sm" variant="secondary"><Plus className="size-4" />{tr('admin.system.addMediaBrowserRoot')}</Button>
          </div>
        </Card.Content>
      </Card>

      <div className="sticky bottom-0 z-10 flex flex-wrap justify-end gap-3 border-t border-border bg-background/95 py-4 backdrop-blur">
        <Button isDisabled={loading || saving || restarting} isPending={saving} onPress={save}>
          <Save className="size-4" />{tr('admin.system.save')}
        </Button>
        <Button isDisabled={loading || saving || restarting} isPending={restarting} onPress={restart} variant="secondary">
          <RotateCcw className="size-4" />{tr('admin.system.restart')}
        </Button>
      </div>
      {loading && <Spinner aria-label={tr('admin.system.loading')} />}
    </div>
  );
}

function BrandAssetField({
  alt,
  inputLabel,
  isUploading,
  onChange,
  onUpload,
  uploadLabel,
  value,
}: {
  alt: string;
  inputLabel: string;
  isUploading: boolean;
  onChange: (value: string) => void;
  onUpload: (event: ChangeEvent<HTMLInputElement>) => void;
  uploadLabel: string;
  value: string;
}) {
  return (
    <div className="flex min-w-0 items-start gap-4">
      <div className="flex size-16 shrink-0 items-center justify-center overflow-hidden rounded-md border border-border bg-default">
        {value ? <img alt={alt} className="size-full object-contain" src={value} /> : <ImageIcon className="size-5 text-muted" />}
      </div>
      <div className="min-w-0 flex-1 space-y-3">
        <Button isPending={isUploading} size="sm" variant="secondary">
          <Upload className="size-4" />
          <label className="cursor-pointer">
            {uploadLabel}
            <input accept="image/png,image/jpeg,image/webp,image/x-icon" aria-label={uploadLabel} className="sr-only" onChange={onUpload} type="file" />
          </label>
        </Button>
        <details>
          <summary className="cursor-pointer text-xs font-medium text-muted">{inputLabel}</summary>
          <TextField className="mt-2" fullWidth>
            <Label>{inputLabel}</Label>
            <Input value={value} onChange={(event) => { onChange(event.currentTarget.value); }} />
          </TextField>
        </details>
      </div>
    </div>
  );
}
