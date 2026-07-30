import {
  Alert,
  Button,
  Card,
  Input,
  Label,
  Skeleton,
  Switch,
  TextField,
  Tooltip,
} from '@heroui/react';
import {
  CheckCircle2,
  Eye,
  EyeOff,
  FlaskConical,
  KeyRound,
  RefreshCw,
  Save,
  Trash2,
  TriangleAlert,
} from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useCallback, useRef, useState } from 'react';

import { ConfirmDialog } from '../ui/ConfirmDialog';
import { PageHeader } from '../ui/PageHeader';
import { StatusChip } from '../ui/StatusChip';
import { PageError } from '../ui/SystemPages';
import { useAuthoritativeLoad } from '../ui/useAuthoritativeLoad';
import {
  deleteTmdbSettings,
  getTmdbSettings,
  saveTmdbSettings,
  testTmdbConnection,
  type TmdbSettings,
  type TmdbSettingsSource,
} from './metadataSettingsApi';

type LoadResult = { settings: TmdbSettings } | { error: unknown };
type Operation = 'save' | 'test' | 'delete' | null;

async function fetchSettings(signal: AbortSignal): Promise<LoadResult> {
  try {
    return { settings: await getTmdbSettings(signal) };
  } catch (error: unknown) {
    return { error };
  }
}

export function MetadataSettingsPage() {
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [settings, setSettings] = useState<TmdbSettings | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [language, setLanguage] = useState('zh-CN');
  const [accessToken, setAccessToken] = useState('');
  const [isAccessTokenVisible, setIsAccessTokenVisible] = useState(false);
  const [loadError, setLoadError] = useState<unknown>(null);
  const [authRedirecting, setAuthRedirecting] = useState(false);
  const [operation, setOperation] = useState<Operation>(null);
  const [hasConflict, setHasConflict] = useState(false);
  const operationRef = useRef<Operation>(null);

  const prepareLoadResult = useCallback(async (result: LoadResult) => {
    if ('settings' in result) {
      return () => {
        setSettings(result.settings);
        setEnabled(result.settings.enabled);
        setLanguage(result.settings.language);
        setAccessToken('');
        setIsAccessTokenVisible(false);
        setHasConflict(false);
        setLoadError(null);
        setAuthRedirecting(false);
      };
    }
    if (await logoutIfAccessDenied(result.error)) {
      return () => { setAuthRedirecting(true); };
    }
    return () => { setLoadError(result.error); };
  }, [logoutIfAccessDenied]);

  const { isMounted, loading, reload } = useAuthoritativeLoad(
    fetchSettings,
    prepareLoadResult,
  );

  const runOperation = async (
    nextOperation: Exclude<Operation, null>,
    action: () => Promise<void>,
  ) => {
    if (operationRef.current !== null) return;
    operationRef.current = nextOperation;
    setOperation(nextOperation);
    try {
      await action();
    } finally {
      operationRef.current = null;
      if (isMounted()) setOperation(null);
    }
  };

  const save = async () => {
    await runOperation('save', async () => {
      try {
        const next = await saveTmdbSettings({
          enabled,
          language,
          accessToken,
          revision: settings?.revision ?? null,
        });
        if (!isMounted()) return;
        setSettings(next);
        setEnabled(next.enabled);
        setLanguage(next.language);
        setAccessToken('');
        setIsAccessTokenVisible(false);
        setHasConflict(false);
        notify('TMDB metadata settings saved.', { type: 'success' });
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        if (isConflict(error)) {
          setHasConflict(true);
          return;
        }
        notify('TMDB metadata settings could not be saved.', { type: 'error' });
      }
    });
  };

  const testConnection = async () => {
    await runOperation('test', async () => {
      try {
        await testTmdbConnection(
          accessToken.length > 0
            ? { accessToken, language }
            : {},
        );
        if (isMounted()) notify('TMDB connection succeeded.', { type: 'success' });
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        notify('TMDB connection failed. Check the token and try again.', { type: 'error' });
      }
    });
  };

  const removeOverride = async () => {
    await runOperation('delete', async () => {
      try {
        await deleteTmdbSettings();
        if (!isMounted()) return;
        setAccessToken('');
        setIsAccessTokenVisible(false);
        notify('TMDB database override removed.', { type: 'success' });
        await reload();
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        throw new Error('TMDB override removal failed.', { cause: error });
      }
    });
  };

  if (authRedirecting) return null;

  return (
    <div className="space-y-5">
      <PageHeader
        actions={(
          <Tooltip>
            <Button
              aria-label="Reload metadata settings"
              isDisabled={operation !== null}
              isIconOnly
              isPending={loading}
              onPress={() => { void reload(); }}
              size="sm"
              variant="ghost"
            >
              <RefreshCw
                aria-hidden="true"
                className={`size-4${loading ? ' animate-spin' : ''}`}
              />
            </Button>
            <Tooltip.Content>Reload metadata settings</Tooltip.Content>
          </Tooltip>
        )}
        description="Configure external metadata enrichment used by library matching and imports."
        title="Metadata"
      />

      {loading && settings === null ? (
        <MetadataSettingsSkeleton />
      ) : loadError !== null && settings === null ? (
        <PageError error={loadError} headingLevel={2} onRetry={() => { void reload(); }} />
      ) : settings !== null ? (
        <>
          {loadError !== null && (
            <Alert role="alert" status="warning">
              <Alert.Indicator>
                <TriangleAlert aria-hidden="true" className="size-4" />
              </Alert.Indicator>
              <Alert.Content>
                <Alert.Title>Showing the last available settings</Alert.Title>
                <Alert.Description>
                  The latest refresh failed. The displayed revision has not been replaced.
                </Alert.Description>
              </Alert.Content>
              <Button onPress={() => { void reload(); }} size="sm" variant="tertiary">
                <RefreshCw aria-hidden="true" className="size-4" />
                Retry refresh
              </Button>
            </Alert>
          )}
          <TmdbSettingsForm
            accessToken={accessToken}
            enabled={enabled}
            hasConflict={hasConflict}
            isAccessTokenVisible={isAccessTokenVisible}
            language={language}
            loading={loading}
            operation={operation}
            onAccessTokenChange={setAccessToken}
            onEnabledChange={setEnabled}
            onLanguageChange={setLanguage}
            onReload={() => { void reload(); }}
            onRemove={removeOverride}
            onSave={() => { void save(); }}
            onTest={() => { void testConnection(); }}
            onVisibilityChange={() => {
              setIsAccessTokenVisible((visible) => !visible);
            }}
            settings={settings}
          />
        </>
      ) : null}
    </div>
  );
}

interface TmdbSettingsFormProps {
  accessToken: string;
  enabled: boolean;
  hasConflict: boolean;
  isAccessTokenVisible: boolean;
  language: string;
  loading: boolean;
  operation: Operation;
  settings: TmdbSettings;
  onAccessTokenChange: (value: string) => void;
  onEnabledChange: (value: boolean) => void;
  onLanguageChange: (value: string) => void;
  onReload: () => void;
  onRemove: () => Promise<void>;
  onSave: () => void;
  onTest: () => void;
  onVisibilityChange: () => void;
}

function TmdbSettingsForm({
  accessToken,
  enabled,
  hasConflict,
  isAccessTokenVisible,
  language,
  loading,
  operation,
  settings,
  onAccessTokenChange,
  onEnabledChange,
  onLanguageChange,
  onReload,
  onRemove,
  onSave,
  onTest,
  onVisibilityChange,
}: TmdbSettingsFormProps) {
  const locked = loading || operation !== null;
  const canTest = settings.configured || accessToken.length > 0;
  const canSave = settings.encryptionAvailable
    && (settings.source === 'Database' || accessToken.length > 0);

  return (
    <div className="space-y-5">
      {hasConflict && (
        <Alert role="alert" status="warning">
          <Alert.Indicator>
            <TriangleAlert aria-hidden="true" className="size-4" />
          </Alert.Indicator>
          <Alert.Content>
            <Alert.Title>Settings changed elsewhere</Alert.Title>
            <Alert.Description>
              Your draft is intact. Reload the latest revision before saving again.
            </Alert.Description>
          </Alert.Content>
          <Button isDisabled={locked} onPress={onReload} size="sm" variant="tertiary">
            <RefreshCw aria-hidden="true" className="size-4" />
            Reload latest
          </Button>
        </Alert>
      )}

      {!settings.encryptionAvailable && (
        <Alert role="alert" status="warning">
          <Alert.Indicator>
            <KeyRound aria-hidden="true" className="size-4" />
          </Alert.Indicator>
          <Alert.Content>
            <Alert.Title>Credential encryption is unavailable</Alert.Title>
            <Alert.Description>
              Configure TJXY_CREDENTIAL_KEYRING before saving a database token.
            </Alert.Description>
          </Alert.Content>
        </Alert>
      )}

      <Card className="gap-0 p-0">
        <Card.Header className="flex-col items-start justify-between gap-4 border-b border-border p-5 sm:flex-row sm:p-6">
          <div className="min-w-0">
            <Card.Title>TMDB</Card.Title>
            <Card.Description className="mt-1">
              The Movie Database metadata provider.
            </Card.Description>
          </div>
          <div className="flex w-full shrink-0 flex-wrap justify-start gap-2 sm:w-auto sm:justify-end">
            <StatusChip tone={sourceTone(settings.source)}>
              {sourceLabel(settings.source)}
            </StatusChip>
            <StatusChip tone={enabled ? 'success' : 'neutral'}>
              {enabled ? 'Enabled' : 'Disabled'}
            </StatusChip>
          </div>
        </Card.Header>

        <Card.Content className="space-y-6 p-5 sm:p-6">
          <div className="grid gap-5 md:grid-cols-[minmax(0,1fr)_14rem]">
            <TextField fullWidth isDisabled={locked} name="tmdb-access-token">
              <Label>TMDB API Read Access Token</Label>
              <div className="relative">
                <Input
                  autoComplete="new-password"
                  className="pr-11"
                  fullWidth
                  onChange={(event) => {
                    onAccessTokenChange(event.currentTarget.value);
                  }}
                  placeholder={settings.configured
                    ? 'Leave empty to keep the configured token'
                    : 'Paste the TMDB API Read Access Token'}
                  type={isAccessTokenVisible ? 'text' : 'password'}
                  value={accessToken}
                />
                <Tooltip>
                  <Button
                    aria-label={isAccessTokenVisible
                      ? 'Hide access token'
                      : 'Show access token'}
                    className="absolute right-1 top-1/2 -translate-y-1/2"
                    isIconOnly
                    onPress={onVisibilityChange}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    {isAccessTokenVisible
                      ? <EyeOff aria-hidden="true" className="size-4" />
                      : <Eye aria-hidden="true" className="size-4" />}
                  </Button>
                  <Tooltip.Content>
                    {isAccessTokenVisible ? 'Hide access token' : 'Show access token'}
                  </Tooltip.Content>
                </Tooltip>
              </div>
              <p className="mt-1.5 text-xs leading-5 text-muted">
                Write-only. TJXY never returns the configured token to this browser.
              </p>
            </TextField>

            <TextField fullWidth isDisabled={locked} name="tmdb-language">
              <Label>Metadata language</Label>
              <Input
                fullWidth
                onChange={(event) => { onLanguageChange(event.currentTarget.value); }}
                placeholder="zh-CN"
                value={language}
              />
            </TextField>
          </div>

          <div className="flex flex-col gap-4 border-t border-border pt-5 sm:flex-row sm:items-center sm:justify-between">
            <Switch
              isDisabled={locked}
              isSelected={enabled}
              onChange={onEnabledChange}
            >
              <Switch.Content>
                <Switch.Control><Switch.Thumb /></Switch.Control>
                Enable TMDB metadata matching
              </Switch.Content>
            </Switch>
            <p className="text-xs text-muted">
              {settings.revision === null
                ? 'No database revision'
                : `Database revision ${String(settings.revision)}`}
            </p>
          </div>
        </Card.Content>

        <Card.Footer className="flex flex-col-reverse gap-3 border-t border-border p-5 sm:flex-row sm:items-center sm:justify-between sm:p-6">
          <div>
            {settings.source === 'Database' && (
              <ConfirmDialog
                confirmLabel="Remove override"
                description="Remove the encrypted database token and return to any environment configuration. This does not change the environment."
                errorDescription="The database override remains active. Try again."
                isPending={locked}
                onConfirm={onRemove}
                title="Remove TMDB database override?"
                trigger={(
                  <Button
                    aria-label="Remove database override"
                    isDisabled={locked}
                    size="sm"
                    variant="danger-soft"
                  >
                    <Trash2 aria-hidden="true" className="size-4" />
                    Remove override
                  </Button>
                )}
              />
            )}
          </div>
          <div className="flex w-full flex-col gap-2 sm:w-auto sm:flex-row">
            <Button
              isDisabled={locked || !canTest}
              isPending={operation === 'test'}
              onPress={onTest}
              variant="secondary"
            >
              {operation === 'test'
                ? <RefreshCw aria-hidden="true" className="size-4 animate-spin" />
                : <FlaskConical aria-hidden="true" className="size-4" />}
              Test connection
            </Button>
            <Button
              isDisabled={locked || hasConflict || !canSave}
              isPending={operation === 'save'}
              onPress={onSave}
            >
              {operation === 'save'
                ? <RefreshCw aria-hidden="true" className="size-4 animate-spin" />
                : <Save aria-hidden="true" className="size-4" />}
              Save settings
            </Button>
          </div>
        </Card.Footer>
      </Card>

      {settings.source !== 'None' && (
        <div className="flex items-start gap-2 text-sm text-muted">
          <CheckCircle2 aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-success" />
          {settings.source === 'Database'
            ? 'Database settings apply immediately without a server restart.'
            : 'The environment fallback remains active until a database override is saved.'}
        </div>
      )}
    </div>
  );
}

function MetadataSettingsSkeleton() {
  return (
    <div aria-label="Loading metadata settings" className="space-y-4" role="status">
      <Skeleton className="h-7 w-48 rounded-md" />
      <Skeleton className="h-64 w-full rounded-lg" />
    </div>
  );
}

function sourceLabel(source: TmdbSettingsSource): string {
  switch (source) {
    case 'Database':
      return 'Database override';
    case 'Environment':
      return 'Environment fallback';
    case 'None':
      return 'Not configured';
  }
}

function sourceTone(source: TmdbSettingsSource): 'accent' | 'success' | 'neutral' {
  switch (source) {
    case 'Database':
      return 'accent';
    case 'Environment':
      return 'success';
    case 'None':
      return 'neutral';
  }
}

function isConflict(error: unknown): boolean {
  return typeof error === 'object'
    && error !== null
    && 'category' in error
    && error.category === 'conflict';
}
