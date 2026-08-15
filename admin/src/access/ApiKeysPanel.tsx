import {
  Button,
  Input,
  Label,
  Modal,
  Skeleton,
  Table,
  TextField,
  Tooltip,
} from '@heroui/react';
import {
  Copy,
  Eye,
  EyeOff,
  KeyRound,
  LoaderCircle,
  Plus,
  RefreshCw,
  Trash2,
} from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useCallback, useRef, useState, type ReactNode, type SyntheticEvent } from 'react';

import { AsyncContent } from '../ui/AsyncContent';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { ResponsiveCollection } from '../ui/ResponsiveCollection';
import { useAuthoritativeLoad } from '../ui/useAuthoritativeLoad';
import { useTranslate } from '../settings/i18n';
import type { ApiKeyInfo } from './apiKeyApi';
import { createApiKey, deleteApiKey, listApiKeys } from './apiKeyApi';
import { formatAccessDate } from './formatAccessDate';

const KEY_MASK = '****************';
type LoadResult = { records: ApiKeyInfo[] } | { error: unknown };
type BusyOperation = 'create' | 'delete' | null;

export function ApiKeysPanel() {
  const tr = useTranslate();
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [keys, setKeys] = useState<ApiKeyInfo[]>([]);
  const [hasLoaded, setHasLoaded] = useState(false);
  const [loadError, setLoadError] = useState<NonNullable<unknown> | null>(null);
  const [authRedirecting, setAuthRedirecting] = useState(false);
  const [busy, setBusy] = useState<BusyOperation>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [appName, setAppName] = useState('');
  const [revealed, setRevealed] = useState<ReadonlySet<number>>(() => new Set());
  const operationRef = useRef<BusyOperation>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);

  const resetReveals = useCallback(() => { setRevealed(new Set()); }, []);

  const prepareLoadResult = useCallback(async (result: LoadResult) => {
    if ('records' in result) {
      return () => {
        setKeys(result.records);
        setHasLoaded(true);
        setLoadError(null);
        setAuthRedirecting(false);
      };
    }
    if (await logoutIfAccessDenied(result.error)) {
      return () => { setAuthRedirecting(true); };
    }
    return () => {
      setLoadError(result.error ?? new Error('API key loading failed.'));
    };
  }, [logoutIfAccessDenied]);

  const { isMounted, loading, reload: loadKeys } = useAuthoritativeLoad(fetchApiKeys, prepareLoadResult);

  const reload = useCallback(async () => {
    resetReveals();
    await loadKeys();
  }, [loadKeys, resetReveals]);

  const create = async (event?: SyntheticEvent<HTMLFormElement>) => {
    event?.preventDefault();
    const nextAppName = appName.trim();
    if (operationRef.current !== null || nextAppName.length === 0) return;
    operationRef.current = 'create';
    setBusy('create');
    try {
      await createApiKey(nextAppName);
      if (!isMounted()) return;
      notify(tr('API key created.', 'API 密钥已创建。'), { type: 'success' });
      setCreateOpen(false);
      setAppName('');
      await reload();
    } catch (caught: unknown) {
      if (!isMounted()) return;
      if (!(await logoutIfAccessDenied(caught)) && isMounted()) {
        notify(tr('The API key could not be created.', '无法创建 API 密钥。'), { type: 'error' });
      }
    } finally {
      operationRef.current = null;
      if (isMounted()) setBusy(null);
    }
  };

  const remove = async (key: ApiKeyInfo) => {
    if (operationRef.current !== null) return;
    operationRef.current = 'delete';
    setBusy('delete');
    try {
      await deleteApiKey(key.accessToken);
      if (!isMounted()) return;
      notify(tr(`API key deleted for ${key.appName}.`, `已删除 ${key.appName} 的 API 密钥。`), { type: 'success' });
      await reload();
      window.setTimeout(() => { headingRef.current?.focus(); }, 0);
    } catch (caught: unknown) {
      if (!isMounted()) return;
      if (await logoutIfAccessDenied(caught)) return;
      if (!isMounted()) return;
      throw caught;
    } finally {
      operationRef.current = null;
      if (isMounted()) setBusy(null);
    }
  };

  const toggleReveal = (id: number) => {
    setRevealed((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const copy = async (key: ApiKeyInfo) => {
    try {
      await navigator.clipboard.writeText(key.accessToken);
      if (isMounted()) notify(tr('API key copied.', 'API 密钥已复制。'), { type: 'success' });
    } catch {
      if (isMounted()) notify(tr('The API key could not be copied.', '无法复制 API 密钥。'), { type: 'error' });
    }
  };

  if (authRedirecting) return null;

  const isLocked = loading || busy !== null;
  return (
    <section aria-labelledby="api-keys-heading" className="space-y-5">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2
            className="text-lg font-semibold text-foreground"
            id="api-keys-heading"
            ref={headingRef}
            tabIndex={-1}
          >{tr('API Keys', 'API 密钥')}</h2>
          <p className="mt-1 text-sm text-muted">{tr('Long-lived credentials for trusted integrations.', '供可信集成使用的长期凭据。')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Tooltip>
            <Button
              aria-label={tr('Reload API keys', '重新加载 API 密钥')}
              isDisabled={isLocked}
              isIconOnly
              onPress={() => { void reload(); }}
              size="sm"
              variant="ghost"
            >
              <RefreshCw aria-hidden="true" className={`size-4${loading ? ' animate-spin' : ''}`} />
            </Button>
            <Tooltip.Content>{tr('Reload API keys', '重新加载 API 密钥')}</Tooltip.Content>
          </Tooltip>
          <Button
            isDisabled={isLocked}
            onPress={() => { setCreateOpen(true); }}
            size="sm"
          >
            <Plus aria-hidden="true" className="size-4" />
            {tr('Create API key', '创建 API 密钥')}
          </Button>
        </div>
      </div>

      {loading && hasLoaded && (
        <p aria-live="polite" className="text-sm text-muted" role="status">{tr('Refreshing API keys...', '正在刷新 API 密钥…')}</p>
      )}

      <AsyncContent
        empty={<EmptyApiKeys />}
        error={loadError}
        hasData={hasLoaded}
        isEmpty={hasLoaded && keys.length === 0}
        isPending={loading}
        loading={<ApiKeySkeleton />}
        onRetry={() => { void reload(); }}
      >
        <ResponsiveCollection
          ariaLabel={tr('API keys collection', 'API 密钥集合')}
          desktop={<ApiKeyTable isLocked={isLocked} keys={keys} onCopy={copy} onDelete={remove} onToggleReveal={toggleReveal} revealed={revealed} />}
          mobile={<ApiKeyMobileList isLocked={isLocked} keys={keys} onCopy={copy} onDelete={remove} onToggleReveal={toggleReveal} revealed={revealed} />}
        />
      </AsyncContent>

      <CreateApiKeyModal
        appName={appName}
        isOpen={createOpen}
        isPending={busy === 'create'}
        onAppNameChange={setAppName}
        onClose={() => { setCreateOpen(false); }}
        onSubmit={create}
      />
    </section>
  );
}

interface ApiKeyCollectionProps {
  keys: ApiKeyInfo[];
  revealed: ReadonlySet<number>;
  isLocked: boolean;
  onToggleReveal: (id: number) => void;
  onCopy: (key: ApiKeyInfo) => Promise<void>;
  onDelete: (key: ApiKeyInfo) => Promise<void>;
}

function ApiKeyTable(props: ApiKeyCollectionProps) {
  const tr = useTranslate();
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label={tr('API Keys', 'API 密钥')} className="table-fixed">
          <Table.Header>
            <Table.Column isRowHeader>{tr('Application', '应用')}</Table.Column>
            <Table.Column>{tr('Key', '密钥')}</Table.Column>
            <Table.Column>{tr('Created', '创建时间')}</Table.Column>
            <Table.Column>{tr('Last used', '最近使用')}</Table.Column>
            <Table.Column className="w-48 text-right">{tr('Actions', '操作')}</Table.Column>
          </Table.Header>
          <Table.Body>
            {props.keys.map((key) => (
              <Table.Row id={key.id} key={key.id}>
                <Table.Cell><span className="break-words font-semibold text-foreground">{key.appName}</span></Table.Cell>
                <Table.Cell><ApiKeyValue isVisible={props.revealed.has(key.id)} value={key.accessToken} /></Table.Cell>
                <Table.Cell>{formatAccessDate(key.dateCreated)}</Table.Cell>
                <Table.Cell>{key.dateLastActivity === null ? tr('Never', '从未') : formatAccessDate(key.dateLastActivity)}</Table.Cell>
                <Table.Cell><ApiKeyActions apiKey={key} {...props} /></Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}

function ApiKeyMobileList(props: ApiKeyCollectionProps) {
  const tr = useTranslate();
  return (
    <ul aria-label={tr('API Keys mobile', 'API 密钥移动端列表')} className="divide-y divide-border border-y border-border">
      {props.keys.map((key) => (
        <li aria-label={key.appName} className="space-y-4 py-4" key={key.id}>
          <dl className="grid grid-cols-[6rem_minmax(0,1fr)] gap-x-3 gap-y-3 text-sm">
            <MobileField label={tr('Application', '应用')}><span className="break-words font-semibold">{key.appName}</span></MobileField>
            <MobileField label={tr('Key', '密钥')}><ApiKeyValue isVisible={props.revealed.has(key.id)} value={key.accessToken} /></MobileField>
            <MobileField label={tr('Created', '创建时间')}>{formatAccessDate(key.dateCreated)}</MobileField>
            <MobileField label={tr('Last used', '最近使用')}>{key.dateLastActivity === null ? tr('Never', '从未') : formatAccessDate(key.dateLastActivity)}</MobileField>
          </dl>
          <ApiKeyActions apiKey={key} {...props} />
        </li>
      ))}
    </ul>
  );
}

function ApiKeyValue({ isVisible, value }: { isVisible: boolean; value: string }) {
  const tr = useTranslate();
  return (
    <code aria-label={isVisible ? tr('Visible API key', '可见的 API 密钥') : tr('Hidden API key', '隐藏的 API 密钥')} className="break-all font-mono text-xs text-foreground">
      {isVisible ? value : KEY_MASK}
    </code>
  );
}

function ApiKeyActions({
  apiKey,
  isLocked,
  onCopy,
  onDelete,
  onToggleReveal,
  revealed,
}: Omit<ApiKeyCollectionProps, 'keys'> & { apiKey: ApiKeyInfo }) {
  const tr = useTranslate();
  const visible = revealed.has(apiKey.id);
  return (
    <div aria-label={`${tr('Actions for', '操作：')} ${apiKey.appName}`} className="flex justify-end gap-1">
      <Tooltip>
        <Button
          aria-label={`${visible ? tr('Hide', '隐藏') : tr('Show', '显示')} ${tr('key for', '密钥：')} ${apiKey.appName}`}
          isDisabled={isLocked}
          isIconOnly
          onPress={() => { onToggleReveal(apiKey.id); }}
          size="sm"
          variant="ghost"
        >
          {visible ? <EyeOff aria-hidden="true" className="size-4" /> : <Eye aria-hidden="true" className="size-4" />}
        </Button>
        <Tooltip.Content>{visible ? tr('Hide API key', '隐藏 API 密钥') : tr('Show API key', '显示 API 密钥')}</Tooltip.Content>
      </Tooltip>
      <Tooltip>
        <Button
          aria-label={`${tr('Copy key for', '复制密钥：')} ${apiKey.appName}`}
          isDisabled={isLocked}
          isIconOnly
          onPress={() => { void onCopy(apiKey); }}
          size="sm"
          variant="ghost"
        >
          <Copy aria-hidden="true" className="size-4" />
        </Button>
        <Tooltip.Content>{tr('Copy API key', '复制 API 密钥')}</Tooltip.Content>
      </Tooltip>
      <ConfirmDialog
        confirmLabel={tr('Delete key', '删除密钥')}
        description={<>{tr('Delete the API key for', '删除以下应用的 API 密钥：')} <strong>{apiKey.appName}</strong>？</>}
        isPending={isLocked}
        onConfirm={() => onDelete(apiKey)}
        title={tr('Delete API key', '删除 API 密钥')}
        trigger={(
          <Button
            aria-label={`${tr('Delete key for', '删除密钥：')} ${apiKey.appName}`}
            className="min-w-24"
            isDisabled={isLocked}
            size="sm"
            variant="danger-soft"
          >
            <Trash2 aria-hidden="true" className="size-4" />
            {tr('Delete', '删除')}
          </Button>
        )}
      />
    </div>
  );
}

function CreateApiKeyModal({
  appName,
  isOpen,
  isPending,
  onAppNameChange,
  onClose,
  onSubmit,
}: {
  appName: string;
  isOpen: boolean;
  isPending: boolean;
  onAppNameChange: (appName: string) => void;
  onClose: () => void;
  onSubmit: (event?: SyntheticEvent<HTMLFormElement>) => void | Promise<void>;
}) {
  const tr = useTranslate();
  return (
    <Modal isOpen={isOpen} onOpenChange={(nextOpen) => { if (!nextOpen && !isPending) onClose(); }}>
      <Modal.Backdrop isDismissable={!isPending} isKeyboardDismissDisabled={isPending}>
        <Modal.Container placement="center" size="sm">
          <Modal.Dialog>
            <Modal.CloseTrigger aria-label={tr('Close', '关闭')} isDisabled={isPending} />
            <Modal.Header>
              <Modal.Icon><KeyRound aria-hidden="true" className="size-5" /></Modal.Icon>
              <Modal.Heading>{tr('Create API key', '创建 API 密钥')}</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <p className="mb-4 text-sm text-muted">{tr('Name the integration that will use this credential.', '为使用此凭据的集成命名。')}</p>
              <form id="create-api-key-form" onSubmit={(event) => { void onSubmit(event); }}>
                <TextField fullWidth isRequired name="appName">
                  <Label>{tr('Application name', '应用名称')}</Label>
                  <Input
                    autoFocus
                    disabled={isPending}
                    maxLength={256}
                    onChange={(event) => { onAppNameChange(event.target.value); }}
                    value={appName}
                  />
                </TextField>
              </form>
            </Modal.Body>
            <Modal.Footer>
              <Button isDisabled={isPending} onPress={onClose} variant="tertiary">{tr('Cancel', '取消')}</Button>
              <Button
                form="create-api-key-form"
                isDisabled={appName.trim().length === 0}
                isPending={isPending}
                type="submit"
              >
                {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <Plus aria-hidden="true" className="size-4" />}
                <span className="inline-flex min-h-5 items-center">{tr('Create key', '创建密钥')}</span>
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function MobileField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="font-medium text-muted">{label}</dt>
      <dd className="min-w-0 text-foreground">{children}</dd>
    </>
  );
}

function ApiKeySkeleton() {
  const tr = useTranslate();
  return (
    <div aria-label={tr('Loading API keys', '正在加载 API 密钥')} className="space-y-3" role="status">
      <Skeleton className="h-10 w-full" />
      <Skeleton className="h-16 w-full" />
      <Skeleton className="h-16 w-full" />
    </div>
  );
}

function EmptyApiKeys() {
  const tr = useTranslate();
  return (
    <div className="border-y border-border py-10 text-center">
      <p className="font-medium text-foreground">{tr('No API keys', '暂无 API 密钥')}</p>
      <p className="mt-1 text-sm text-muted">{tr('Create a key when an integration needs server access.', '当集成需要访问服务器时，请创建密钥。')}</p>
    </div>
  );
}

async function fetchApiKeys(signal: AbortSignal): Promise<LoadResult> {
  try {
    return { records: await listApiKeys(signal) };
  } catch (error: unknown) {
    return { error };
  }
}
