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
import { interpolate, useTranslate } from './i18n';
import {
  deleteMusicBrainzSettings,
  deleteTheAudioDbSettings,
  deleteTmdbSettings,
  getMusicBrainzSettings,
  getTheAudioDbSettings,
  getTmdbSettings,
  saveMusicBrainzSettings,
  saveTheAudioDbSettings,
  saveTmdbSettings,
  testMusicBrainzConnection,
  testTheAudioDbConnection,
  testTmdbConnection,
  type MetadataSettingsSource,
  type MusicBrainzSettings,
  type TheAudioDbSettings,
  type TmdbSettings,
} from './metadataSettingsApi';

type LoadResult = {
  settings: TmdbSettings;
  theAudioDb: TheAudioDbSettings;
  musicBrainz: MusicBrainzSettings;
} | { error: unknown };
type Operation = 'save' | 'test' | 'delete' | null;

async function fetchSettings(signal: AbortSignal): Promise<LoadResult> {
  try {
    const [settings, theAudioDb, musicBrainz] = await Promise.all([
      getTmdbSettings(signal),
      getTheAudioDbSettings(signal),
      getMusicBrainzSettings(signal),
    ]);
    return { settings, theAudioDb, musicBrainz };
  } catch (error: unknown) {
    return { error };
  }
}

export function MetadataSettingsPage() {
  const t = useTranslate();
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
  const [theAudioDb, setTheAudioDb] = useState<TheAudioDbSettings | null>(null);
  const [theAudioDbEnabled, setTheAudioDbEnabled] = useState(false);
  const [theAudioDbApiKey, setTheAudioDbApiKey] = useState('');
  const [isTheAudioDbApiKeyVisible, setIsTheAudioDbApiKeyVisible] = useState(false);
  const [theAudioDbOperation, setTheAudioDbOperation] = useState<Operation>(null);
  const [theAudioDbConflict, setTheAudioDbConflict] = useState(false);
  const theAudioDbOperationRef = useRef<Operation>(null);
  const [musicBrainz, setMusicBrainz] = useState<MusicBrainzSettings | null>(null);
  const [musicBrainzEnabled, setMusicBrainzEnabled] = useState(false);
  const [musicBrainzUserAgent, setMusicBrainzUserAgent] = useState('');
  const [musicBrainzOperation, setMusicBrainzOperation] = useState<Operation>(null);
  const [musicBrainzConflict, setMusicBrainzConflict] = useState(false);
  const musicBrainzOperationRef = useRef<Operation>(null);

  const prepareLoadResult = useCallback(async (result: LoadResult) => {
    if ('settings' in result) {
      return () => {
        setSettings(result.settings);
        setEnabled(result.settings.enabled);
        setLanguage(result.settings.language);
        setAccessToken('');
        setIsAccessTokenVisible(false);
        setHasConflict(false);
        setTheAudioDb(result.theAudioDb);
        setTheAudioDbEnabled(result.theAudioDb.enabled);
        setTheAudioDbApiKey('');
        setIsTheAudioDbApiKeyVisible(false);
        setTheAudioDbConflict(false);
        setMusicBrainz(result.musicBrainz);
        setMusicBrainzEnabled(result.musicBrainz.enabled);
        setMusicBrainzUserAgent(result.musicBrainz.userAgent);
        setMusicBrainzConflict(false);
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
        notify(t('admin.metadata.tmdbSaved'), { type: 'success' });
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        if (isConflict(error)) {
          setHasConflict(true);
          return;
        }
        notify(t('admin.metadata.tmdbSaveFailed'), { type: 'error' });
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
        if (isMounted()) notify(t('admin.metadata.tmdbTestSucceeded'), { type: 'success' });
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        notify(t('admin.metadata.tmdbTestFailed'), { type: 'error' });
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
        notify(t('admin.metadata.tmdbRemoved'), { type: 'success' });
        await reload();
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        throw new Error('TMDB override removal failed.', { cause: error });
      }
    });
  };

  const runTheAudioDbOperation = async (
    nextOperation: Exclude<Operation, null>,
    action: () => Promise<void>,
  ) => {
    if (theAudioDbOperationRef.current !== null) return;
    theAudioDbOperationRef.current = nextOperation;
    setTheAudioDbOperation(nextOperation);
    try {
      await action();
    } finally {
      theAudioDbOperationRef.current = null;
      if (isMounted()) setTheAudioDbOperation(null);
    }
  };

  const saveTheAudioDb = async () => {
    await runTheAudioDbOperation('save', async () => {
      try {
        const next = await saveTheAudioDbSettings({
          enabled: theAudioDbEnabled,
          apiKey: theAudioDbApiKey,
          revision: theAudioDb?.revision ?? null,
        });
        if (!isMounted()) return;
        setTheAudioDb(next);
        setTheAudioDbEnabled(next.enabled);
        setTheAudioDbApiKey('');
        setIsTheAudioDbApiKeyVisible(false);
        setTheAudioDbConflict(false);
        notify(t('admin.metadata.audioDbSaved'), { type: 'success' });
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        if (isConflict(error)) {
          setTheAudioDbConflict(true);
          return;
        }
        notify(t('admin.metadata.audioDbSaveFailed'), { type: 'error' });
      }
    });
  };

  const testTheAudioDb = async () => {
    await runTheAudioDbOperation('test', async () => {
      try {
        await testTheAudioDbConnection(
          theAudioDbApiKey.length > 0 ? { apiKey: theAudioDbApiKey } : {},
        );
        if (isMounted()) notify(t('admin.metadata.audioDbTestSucceeded'), { type: 'success' });
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        notify(t('admin.metadata.audioDbTestFailed'), { type: 'error' });
      }
    });
  };

  const removeTheAudioDb = async () => {
    await runTheAudioDbOperation('delete', async () => {
      try {
        await deleteTheAudioDbSettings();
        if (!isMounted()) return;
        setTheAudioDbApiKey('');
        setIsTheAudioDbApiKeyVisible(false);
        notify(t('admin.metadata.audioDbRemoved'), { type: 'success' });
        await reload();
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        notify(t('admin.metadata.audioDbRemoveFailed'), { type: 'error' });
      }
    });
  };

  const runMusicBrainzOperation = async (
    nextOperation: Exclude<Operation, null>,
    action: () => Promise<void>,
  ) => {
    if (musicBrainzOperationRef.current !== null) return;
    musicBrainzOperationRef.current = nextOperation;
    setMusicBrainzOperation(nextOperation);
    try {
      await action();
    } finally {
      musicBrainzOperationRef.current = null;
      if (isMounted()) setMusicBrainzOperation(null);
    }
  };

  const saveMusicBrainz = async () => {
    await runMusicBrainzOperation('save', async () => {
      try {
        const next = await saveMusicBrainzSettings({
          enabled: musicBrainzEnabled,
          userAgent: musicBrainzUserAgent,
          revision: musicBrainz?.revision ?? null,
        });
        if (!isMounted()) return;
        setMusicBrainz(next);
        setMusicBrainzEnabled(next.enabled);
        setMusicBrainzUserAgent(next.userAgent);
        setMusicBrainzConflict(false);
        notify(t('admin.metadata.musicBrainzSaved'), { type: 'success' });
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        if (isConflict(error)) {
          setMusicBrainzConflict(true);
          return;
        }
        notify(t('admin.metadata.musicBrainzSaveFailed'), { type: 'error' });
      }
    });
  };

  const testMusicBrainz = async () => {
    await runMusicBrainzOperation('test', async () => {
      try {
        await testMusicBrainzConnection(
          musicBrainzUserAgent.length > 0 ? { userAgent: musicBrainzUserAgent } : {},
        );
        if (isMounted()) notify(t('admin.metadata.musicBrainzTestSucceeded'), { type: 'success' });
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        notify(t('admin.metadata.musicBrainzTestFailed'), { type: 'error' });
      }
    });
  };

  const removeMusicBrainz = async () => {
    await runMusicBrainzOperation('delete', async () => {
      try {
        await deleteMusicBrainzSettings();
        if (!isMounted()) return;
        notify(t('admin.metadata.musicBrainzRemoved'), { type: 'success' });
        await reload();
      } catch (error: unknown) {
        if (!isMounted() || await logoutIfAccessDenied(error)) return;
        notify(t('admin.metadata.musicBrainzRemoveFailed'), { type: 'error' });
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
              aria-label={t('admin.metadata.reload')}
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
            <Tooltip.Content>{t('admin.metadata.reload')}</Tooltip.Content>
          </Tooltip>
        )}
        description={t('admin.metadata.subtitle')}
        title={t('admin.metadata.title')}
      />

      {loading && settings === null ? (
        <MetadataSettingsSkeleton />
      ) : loadError !== null && settings === null ? (
        <PageError error={loadError} headingLevel={2} onRetry={() => { void reload(); }} />
      ) : settings !== null && theAudioDb !== null && musicBrainz !== null ? (
        <>
          {loadError !== null && (
            <Alert role="alert" status="warning">
              <Alert.Indicator>
                <TriangleAlert aria-hidden="true" className="size-4" />
              </Alert.Indicator>
              <Alert.Content>
                <Alert.Title>{t('admin.metadata.staleTitle')}</Alert.Title>
                <Alert.Description>
                  {t('admin.metadata.staleDescription')}
                </Alert.Description>
              </Alert.Content>
              <Button onPress={() => { void reload(); }} size="sm" variant="tertiary">
                <RefreshCw aria-hidden="true" className="size-4" />
                {t('admin.metadata.retryRefresh')}
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
          <MusicProviderSettingsForm
            description={t('admin.metadata.audioDbDescription')}
            enabled={theAudioDbEnabled}
            fieldLabel={t('admin.metadata.audioDbApiKey')}
            fieldValue={theAudioDbApiKey}
            hasConflict={theAudioDbConflict}
            isSecret
            isSecretVisible={isTheAudioDbApiKeyVisible}
            loading={loading}
            operation={theAudioDbOperation}
            onEnabledChange={setTheAudioDbEnabled}
            onFieldChange={setTheAudioDbApiKey}
            onReload={() => { void reload(); }}
            onRemove={removeTheAudioDb}
            onSave={() => { void saveTheAudioDb(); }}
            onTest={() => { void testTheAudioDb(); }}
            onVisibilityChange={() => {
              setIsTheAudioDbApiKeyVisible((visible) => !visible);
            }}
            provider="TheAudioDB"
            settings={theAudioDb}
          />
          <MusicProviderSettingsForm
            description={t('admin.metadata.musicBrainzDescription')}
            enabled={musicBrainzEnabled}
            fieldLabel={t('admin.metadata.musicBrainzUserAgent')}
            fieldValue={musicBrainzUserAgent}
            hasConflict={musicBrainzConflict}
            isSecret={false}
            isSecretVisible
            loading={loading}
            operation={musicBrainzOperation}
            onEnabledChange={setMusicBrainzEnabled}
            onFieldChange={setMusicBrainzUserAgent}
            onReload={() => { void reload(); }}
            onRemove={removeMusicBrainz}
            onSave={() => { void saveMusicBrainz(); }}
            onTest={() => { void testMusicBrainz(); }}
            provider="MusicBrainz"
            settings={musicBrainz}
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
  const t = useTranslate();
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
            <Alert.Title>{t('admin.metadata.conflictTitle')}</Alert.Title>
            <Alert.Description>
              {t('admin.metadata.conflictDescription')}
            </Alert.Description>
          </Alert.Content>
          <Button isDisabled={locked} onPress={onReload} size="sm" variant="tertiary">
            <RefreshCw aria-hidden="true" className="size-4" />
            {t('admin.metadata.reloadLatest')}
          </Button>
        </Alert>
      )}

      {!settings.encryptionAvailable && (
        <Alert role="alert" status="warning">
          <Alert.Indicator>
            <KeyRound aria-hidden="true" className="size-4" />
          </Alert.Indicator>
          <Alert.Content>
            <Alert.Title>{t('admin.metadata.encryptionUnavailable')}</Alert.Title>
            <Alert.Description>
              {t('admin.metadata.encryptionDescription')}
            </Alert.Description>
          </Alert.Content>
        </Alert>
      )}

      <Card className="gap-0 p-0">
        <Card.Header className="flex-col items-start justify-between gap-4 border-b border-border p-5 sm:flex-row sm:p-6">
          <div className="min-w-0">
            <Card.Title>TMDB</Card.Title>
            <Card.Description className="mt-1">
              {t('admin.metadata.tmdbDescription')}
            </Card.Description>
          </div>
          <div className="flex w-full shrink-0 flex-wrap justify-start gap-2 sm:w-auto sm:justify-end">
            <StatusChip tone={sourceTone(settings.source)}>
              {t(sourceLabelKey(settings.source))}
            </StatusChip>
            <StatusChip tone={enabled ? 'success' : 'neutral'}>
              {enabled ? t('admin.metadata.enabled') : t('admin.metadata.disabled')}
            </StatusChip>
          </div>
        </Card.Header>

        <Card.Content className="space-y-6 p-5 sm:p-6">
          <div className="grid gap-5 md:grid-cols-[minmax(0,1fr)_14rem]">
            <TextField fullWidth isDisabled={locked} name="tmdb-access-token">
              <Label>{t('admin.metadata.tmdbToken')}</Label>
              <div className="relative">
                <Input
                  autoComplete="new-password"
                  className="pr-11"
                  fullWidth
                  onChange={(event) => {
                    onAccessTokenChange(event.currentTarget.value);
                  }}
                  placeholder={settings.configured
                    ? t('admin.metadata.tmdbTokenKeep')
                    : t('admin.metadata.tmdbTokenPlaceholder')}
                  type={isAccessTokenVisible ? 'text' : 'password'}
                  value={accessToken}
                />
                <Tooltip>
                  <Button
                    aria-label={isAccessTokenVisible
                      ? t('admin.metadata.hideToken')
                      : t('admin.metadata.showToken')}
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
                    {isAccessTokenVisible
                      ? t('admin.metadata.hideToken')
                      : t('admin.metadata.showToken')}
                  </Tooltip.Content>
                </Tooltip>
              </div>
              <p className="mt-1.5 text-xs leading-5 text-muted">
                {t('admin.metadata.tmdbTokenHint')}
              </p>
            </TextField>

            <TextField fullWidth isDisabled={locked} name="tmdb-language">
              <Label>{t('admin.metadata.language')}</Label>
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
                {t('admin.metadata.enableTmdb')}
              </Switch.Content>
            </Switch>
            <p className="text-xs text-muted">
              {settings.revision === null
                ? t('admin.metadata.noRevision')
                : interpolate(t('admin.metadata.revision'), {
                  revision: String(settings.revision),
                })}
            </p>
          </div>
        </Card.Content>

        <Card.Footer className="flex flex-col-reverse gap-3 border-t border-border p-5 sm:flex-row sm:items-center sm:justify-between sm:p-6">
          <div>
            {settings.source === 'Database' && (
              <ConfirmDialog
                confirmLabel={t('admin.metadata.removeOverride')}
                description={t('admin.metadata.tmdbRemoveDescription')}
                errorDescription={t('admin.metadata.removeError')}
                isPending={locked}
                onConfirm={onRemove}
                title={t('admin.metadata.tmdbRemoveTitle')}
                trigger={(
                  <Button
                    aria-label={t('admin.metadata.removeDatabaseOverride')}
                    isDisabled={locked}
                    size="sm"
                    variant="danger-soft"
                  >
                    <Trash2 aria-hidden="true" className="size-4" />
                    {t('admin.metadata.removeOverride')}
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
              {t('admin.metadata.testConnection')}
            </Button>
            <Button
              isDisabled={locked || hasConflict || !canSave}
              isPending={operation === 'save'}
              onPress={onSave}
            >
              {operation === 'save'
                ? <RefreshCw aria-hidden="true" className="size-4 animate-spin" />
                : <Save aria-hidden="true" className="size-4" />}
              {t('admin.metadata.saveSettings')}
            </Button>
          </div>
        </Card.Footer>
      </Card>

      {settings.source !== 'None' && (
        <div className="flex items-start gap-2 text-sm text-muted">
          <CheckCircle2 aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-success" />
          {settings.source === 'Database'
            ? t('admin.metadata.databaseApplied')
            : t('admin.metadata.environmentApplied')}
        </div>
      )}
    </div>
  );
}

interface MusicProviderSettingsFormProps {
  description: string;
  enabled: boolean;
  fieldLabel: string;
  fieldValue: string;
  hasConflict: boolean;
  isSecret: boolean;
  isSecretVisible: boolean;
  loading: boolean;
  operation: Operation;
  provider: 'TheAudioDB' | 'MusicBrainz';
  settings: TheAudioDbSettings | MusicBrainzSettings;
  onEnabledChange: (value: boolean) => void;
  onFieldChange: (value: string) => void;
  onReload: () => void;
  onRemove: () => Promise<void>;
  onSave: () => void;
  onTest: () => void;
  onVisibilityChange?: () => void;
}

function MusicProviderSettingsForm({
  description,
  enabled,
  fieldLabel,
  fieldValue,
  hasConflict,
  isSecret,
  isSecretVisible,
  loading,
  operation,
  provider,
  settings,
  onEnabledChange,
  onFieldChange,
  onReload,
  onRemove,
  onSave,
  onTest,
  onVisibilityChange,
}: MusicProviderSettingsFormProps) {
  const t = useTranslate();
  const locked = loading || operation !== null;
  const canUseConfiguredValue = settings.configured && (isSecret || fieldValue.length > 0);
  const canTest = canUseConfiguredValue || fieldValue.length > 0;
  const canSave = settings.encryptionAvailable
    && (settings.source === 'Database' || fieldValue.length > 0);

  return (
    <div className="space-y-4">
      {hasConflict && (
        <Alert role="alert" status="warning">
          <Alert.Indicator>
            <TriangleAlert aria-hidden="true" className="size-4" />
          </Alert.Indicator>
          <Alert.Content>
            <Alert.Title>
              {interpolate(t('admin.metadata.providerConflictTitle'), { provider })}
            </Alert.Title>
            <Alert.Description>
              {t('admin.metadata.conflictDescription')}
            </Alert.Description>
          </Alert.Content>
          <Button isDisabled={locked} onPress={onReload} size="sm" variant="tertiary">
            <RefreshCw aria-hidden="true" className="size-4" />
            {t('admin.metadata.reloadLatest')}
          </Button>
        </Alert>
      )}

      <Card className="gap-0 p-0">
        <Card.Header className="flex-col items-start justify-between gap-4 border-b border-border p-5 sm:flex-row sm:p-6">
          <div className="min-w-0">
            <Card.Title>{provider}</Card.Title>
            <Card.Description className="mt-1">{description}</Card.Description>
          </div>
          <div className="flex w-full shrink-0 flex-wrap justify-start gap-2 sm:w-auto sm:justify-end">
            <StatusChip tone={sourceTone(settings.source)}>
              {t(sourceLabelKey(settings.source))}
            </StatusChip>
            <StatusChip tone={enabled ? 'success' : 'neutral'}>
              {enabled ? t('admin.metadata.enabled') : t('admin.metadata.disabled')}
            </StatusChip>
          </div>
        </Card.Header>

        <Card.Content className="space-y-6 p-5 sm:p-6">
          {!settings.encryptionAvailable && (
            <Alert role="alert" status="warning">
              <Alert.Indicator>
                <KeyRound aria-hidden="true" className="size-4" />
              </Alert.Indicator>
              <Alert.Content>
                <Alert.Title>{t('admin.metadata.encryptionUnavailable')}</Alert.Title>
                <Alert.Description>
                  {t('admin.metadata.encryptionOverrideDescription')}
                </Alert.Description>
              </Alert.Content>
            </Alert>
          )}

          <TextField fullWidth isDisabled={locked} name={`${provider.toLowerCase()}-credential`}>
            <Label>{fieldLabel}</Label>
            <div className="relative">
              <Input
                autoComplete={isSecret ? 'new-password' : 'off'}
                className={isSecret ? 'pr-11' : undefined}
                fullWidth
                onChange={(event) => { onFieldChange(event.currentTarget.value); }}
                placeholder={provider === 'TheAudioDB'
                  ? (settings.configured
                    ? t('admin.metadata.audioDbKeyKeep')
                    : t('admin.metadata.audioDbKeyPlaceholder'))
                  : 'TJXY/1.0 (contact@example.com)'}
                type={isSecret && !isSecretVisible ? 'password' : 'text'}
                value={fieldValue}
              />
              {isSecret && (
                <Tooltip>
                  <Button
                    aria-label={isSecretVisible
                      ? t('admin.metadata.hideApiKey')
                      : t('admin.metadata.showApiKey')}
                    className="absolute right-1 top-1/2 -translate-y-1/2"
                    isIconOnly
                    onPress={onVisibilityChange}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    {isSecretVisible
                      ? <EyeOff aria-hidden="true" className="size-4" />
                      : <Eye aria-hidden="true" className="size-4" />}
                  </Button>
                  <Tooltip.Content>
                    {isSecretVisible
                      ? t('admin.metadata.hideApiKey')
                      : t('admin.metadata.showApiKey')}
                  </Tooltip.Content>
                </Tooltip>
              )}
            </div>
            <p className="mt-1.5 text-xs leading-5 text-muted">
              {isSecret
                ? t('admin.metadata.audioDbKeyHint')
                : t('admin.metadata.musicBrainzHint')}
            </p>
          </TextField>

          <div className="flex flex-col gap-4 border-t border-border pt-5 sm:flex-row sm:items-center sm:justify-between">
            <Switch isDisabled={locked} isSelected={enabled} onChange={onEnabledChange}>
              <Switch.Content>
                <Switch.Control><Switch.Thumb /></Switch.Control>
                {interpolate(t('admin.metadata.enableProvider'), { provider })}
              </Switch.Content>
            </Switch>
            <p className="text-xs text-muted">
              {settings.revision === null
                ? t('admin.metadata.noRevision')
                : interpolate(t('admin.metadata.revision'), {
                  revision: String(settings.revision),
                })}
            </p>
          </div>
        </Card.Content>

        <Card.Footer className="flex flex-col-reverse gap-3 border-t border-border p-5 sm:flex-row sm:items-center sm:justify-between sm:p-6">
          <div>
            {settings.source === 'Database' && (
              <ConfirmDialog
                confirmLabel={t('admin.metadata.removeOverride')}
                description={interpolate(t('admin.metadata.providerRemoveDescription'), {
                  provider,
                })}
                errorDescription={t('admin.metadata.removeError')}
                isPending={locked}
                onConfirm={onRemove}
                title={interpolate(t('admin.metadata.providerRemoveTitle'), { provider })}
                trigger={(
                  <Button
                    aria-label={interpolate(
                      t('admin.metadata.removeProviderDatabaseOverride'),
                      { provider },
                    )}
                    isDisabled={locked}
                    size="sm"
                    variant="danger-soft"
                  >
                    <Trash2 aria-hidden="true" className="size-4" />
                    {t('admin.metadata.removeOverride')}
                  </Button>
                )}
              />
            )}
          </div>
          <div className="flex w-full flex-col gap-2 sm:w-auto sm:flex-row">
            <Button
              aria-label={interpolate(t('admin.metadata.testProviderConnection'), { provider })}
              isDisabled={locked || !canTest}
              isPending={operation === 'test'}
              onPress={onTest}
              variant="secondary"
            >
              {operation === 'test'
                ? <RefreshCw aria-hidden="true" className="size-4 animate-spin" />
                : <FlaskConical aria-hidden="true" className="size-4" />}
              {t('admin.metadata.testConnection')}
            </Button>
            <Button
              aria-label={interpolate(t('admin.metadata.saveProviderSettings'), { provider })}
              isDisabled={locked || hasConflict || !canSave}
              isPending={operation === 'save'}
              onPress={onSave}
            >
              {operation === 'save'
                ? <RefreshCw aria-hidden="true" className="size-4 animate-spin" />
                : <Save aria-hidden="true" className="size-4" />}
              {t('admin.metadata.saveSettings')}
            </Button>
          </div>
        </Card.Footer>
      </Card>
    </div>
  );
}

function MetadataSettingsSkeleton() {
  const t = useTranslate();
  return (
    <div aria-label={t('admin.metadata.loading')} className="space-y-4" role="status">
      <Skeleton className="h-7 w-48 rounded-md" />
      <Skeleton className="h-64 w-full rounded-lg" />
    </div>
  );
}

function sourceLabelKey(source: MetadataSettingsSource): string {
  switch (source) {
    case 'Database':
      return 'admin.metadata.sourceDatabase';
    case 'Environment':
      return 'admin.metadata.sourceEnvironment';
    case 'None':
      return 'admin.metadata.sourceNone';
  }
}

function sourceTone(source: MetadataSettingsSource): 'accent' | 'success' | 'neutral' {
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
