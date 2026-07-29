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
import type { ApiKeyInfo } from './apiKeyApi';
import { createApiKey, deleteApiKey, listApiKeys } from './apiKeyApi';
import { formatAccessDate } from './formatAccessDate';

const KEY_MASK = '****************';
type LoadResult = { records: ApiKeyInfo[] } | { error: unknown };
type BusyOperation = 'create' | 'delete' | null;

export function ApiKeysPanel() {
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
      notify('API key created.', { type: 'success' });
      setCreateOpen(false);
      setAppName('');
      await reload();
    } catch (caught: unknown) {
      if (!isMounted()) return;
      if (!(await logoutIfAccessDenied(caught)) && isMounted()) {
        notify('The API key could not be created.', { type: 'error' });
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
      notify(`API key deleted for ${key.appName}.`, { type: 'success' });
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
      if (isMounted()) notify('API key copied.', { type: 'success' });
    } catch {
      if (isMounted()) notify('The API key could not be copied.', { type: 'error' });
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
          >API Keys</h2>
          <p className="mt-1 text-sm text-muted">Long-lived credentials for trusted integrations.</p>
        </div>
        <div className="flex items-center gap-2">
          <Tooltip>
            <Button
              aria-label="Reload API keys"
              isDisabled={isLocked}
              isIconOnly
              onPress={() => { void reload(); }}
              size="sm"
              variant="ghost"
            >
              <RefreshCw aria-hidden="true" className={`size-4${loading ? ' animate-spin' : ''}`} />
            </Button>
            <Tooltip.Content>Reload API keys</Tooltip.Content>
          </Tooltip>
          <Button
            isDisabled={isLocked}
            onPress={() => { setCreateOpen(true); }}
            size="sm"
          >
            <Plus aria-hidden="true" className="size-4" />
            Create API key
          </Button>
        </div>
      </div>

      {loading && hasLoaded && (
        <p aria-live="polite" className="text-sm text-muted" role="status">Refreshing API keys...</p>
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
          ariaLabel="API keys collection"
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
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label="API Keys" className="table-fixed">
          <Table.Header>
            <Table.Column isRowHeader>Application</Table.Column>
            <Table.Column>Key</Table.Column>
            <Table.Column>Created</Table.Column>
            <Table.Column>Last used</Table.Column>
            <Table.Column className="w-48 text-right">Actions</Table.Column>
          </Table.Header>
          <Table.Body>
            {props.keys.map((key) => (
              <Table.Row id={key.id} key={key.id}>
                <Table.Cell><span className="break-words font-semibold text-foreground">{key.appName}</span></Table.Cell>
                <Table.Cell><ApiKeyValue isVisible={props.revealed.has(key.id)} value={key.accessToken} /></Table.Cell>
                <Table.Cell>{formatAccessDate(key.dateCreated)}</Table.Cell>
                <Table.Cell>{key.dateLastActivity === null ? 'Never' : formatAccessDate(key.dateLastActivity)}</Table.Cell>
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
  return (
    <ul aria-label="API Keys mobile" className="divide-y divide-border border-y border-border">
      {props.keys.map((key) => (
        <li aria-label={key.appName} className="space-y-4 py-4" key={key.id}>
          <dl className="grid grid-cols-[6rem_minmax(0,1fr)] gap-x-3 gap-y-3 text-sm">
            <MobileField label="Application"><span className="break-words font-semibold">{key.appName}</span></MobileField>
            <MobileField label="Key"><ApiKeyValue isVisible={props.revealed.has(key.id)} value={key.accessToken} /></MobileField>
            <MobileField label="Created">{formatAccessDate(key.dateCreated)}</MobileField>
            <MobileField label="Last used">{key.dateLastActivity === null ? 'Never' : formatAccessDate(key.dateLastActivity)}</MobileField>
          </dl>
          <ApiKeyActions apiKey={key} {...props} />
        </li>
      ))}
    </ul>
  );
}

function ApiKeyValue({ isVisible, value }: { isVisible: boolean; value: string }) {
  return (
    <code aria-label={isVisible ? 'Visible API key' : 'Hidden API key'} className="break-all font-mono text-xs text-foreground">
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
  const visible = revealed.has(apiKey.id);
  return (
    <div aria-label={`Actions for ${apiKey.appName}`} className="flex justify-end gap-1">
      <Tooltip>
        <Button
          aria-label={`${visible ? 'Hide' : 'Show'} key for ${apiKey.appName}`}
          isDisabled={isLocked}
          isIconOnly
          onPress={() => { onToggleReveal(apiKey.id); }}
          size="sm"
          variant="ghost"
        >
          {visible ? <EyeOff aria-hidden="true" className="size-4" /> : <Eye aria-hidden="true" className="size-4" />}
        </Button>
        <Tooltip.Content>{visible ? 'Hide API key' : 'Show API key'}</Tooltip.Content>
      </Tooltip>
      <Tooltip>
        <Button
          aria-label={`Copy key for ${apiKey.appName}`}
          isDisabled={isLocked}
          isIconOnly
          onPress={() => { void onCopy(apiKey); }}
          size="sm"
          variant="ghost"
        >
          <Copy aria-hidden="true" className="size-4" />
        </Button>
        <Tooltip.Content>Copy API key</Tooltip.Content>
      </Tooltip>
      <ConfirmDialog
        confirmLabel="Delete key"
        description={<>Delete the API key for <strong>{apiKey.appName}</strong>?</>}
        isPending={isLocked}
        onConfirm={() => onDelete(apiKey)}
        title="Delete API key"
        trigger={(
          <Button
            aria-label={`Delete key for ${apiKey.appName}`}
            className="min-w-24"
            isDisabled={isLocked}
            size="sm"
            variant="danger-soft"
          >
            <Trash2 aria-hidden="true" className="size-4" />
            Delete
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
  return (
    <Modal isOpen={isOpen} onOpenChange={(nextOpen) => { if (!nextOpen && !isPending) onClose(); }}>
      <Modal.Backdrop isDismissable={!isPending} isKeyboardDismissDisabled={isPending}>
        <Modal.Container placement="center" size="sm">
          <Modal.Dialog>
            <Modal.CloseTrigger aria-label="Close" isDisabled={isPending} />
            <Modal.Header>
              <Modal.Icon><KeyRound aria-hidden="true" className="size-5" /></Modal.Icon>
              <Modal.Heading>Create API key</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <p className="mb-4 text-sm text-muted">Name the integration that will use this credential.</p>
              <form id="create-api-key-form" onSubmit={(event) => { void onSubmit(event); }}>
                <TextField fullWidth isRequired name="appName">
                  <Label>Application name</Label>
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
              <Button isDisabled={isPending} onPress={onClose} variant="tertiary">Cancel</Button>
              <Button
                form="create-api-key-form"
                isDisabled={appName.trim().length === 0}
                isPending={isPending}
                type="submit"
              >
                {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <Plus aria-hidden="true" className="size-4" />}
                <span className="inline-flex min-h-5 items-center">Create key</span>
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
  return (
    <div aria-label="Loading API keys" className="space-y-3" role="status">
      <Skeleton className="h-10 w-full" />
      <Skeleton className="h-16 w-full" />
      <Skeleton className="h-16 w-full" />
    </div>
  );
}

function EmptyApiKeys() {
  return (
    <div className="border-y border-border py-10 text-center">
      <p className="font-medium text-foreground">No API keys</p>
      <p className="mt-1 text-sm text-muted">Create a key when an integration needs server access.</p>
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
