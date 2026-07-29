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
import { LoaderCircle, Pencil, RefreshCw, Save, Trash2 } from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useCallback, useRef, useState, type ReactNode, type SyntheticEvent } from 'react';

import { AsyncContent } from '../ui/AsyncContent';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { ResponsiveCollection } from '../ui/ResponsiveCollection';
import { useAuthoritativeLoad } from '../ui/useAuthoritativeLoad';
import type { DeviceInfo } from './deviceApi';
import { deleteDevice, listDevices, updateDeviceName } from './deviceApi';
import { formatAccessDate } from './formatAccessDate';

type LoadResult = { records: DeviceInfo[] } | { error: unknown };
type BusyOperation = 'rename' | 'revoke' | null;

export function DevicesPanel() {
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [hasLoaded, setHasLoaded] = useState(false);
  const [loadError, setLoadError] = useState<NonNullable<unknown> | null>(null);
  const [authRedirecting, setAuthRedirecting] = useState(false);
  const [busy, setBusy] = useState<BusyOperation>(null);
  const [editing, setEditing] = useState<DeviceInfo | null>(null);
  const [customName, setCustomName] = useState('');
  const operationRef = useRef<BusyOperation>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);

  const prepareLoadResult = useCallback(async (result: LoadResult) => {
    if ('records' in result) {
      return () => {
        setDevices(result.records);
        setHasLoaded(true);
        setLoadError(null);
        setAuthRedirecting(false);
      };
    }
    if (await logoutIfAccessDenied(result.error)) {
      return () => { setAuthRedirecting(true); };
    }
    return () => {
      setLoadError(result.error ?? new Error('Device loading failed.'));
    };
  }, [logoutIfAccessDenied]);

  const { isMounted, loading, reload } = useAuthoritativeLoad(fetchDevices, prepareLoadResult);

  const openRename = (device: DeviceInfo) => {
    setEditing(device);
    setCustomName(device.customName ?? '');
  };

  const saveName = async (event?: SyntheticEvent<HTMLFormElement>) => {
    event?.preventDefault();
    if (editing === null || operationRef.current !== null) return;
    operationRef.current = 'rename';
    setBusy('rename');
    try {
      const nextName = customName.trim();
      await updateDeviceName(editing.id, nextName.length === 0 ? null : nextName);
      if (!isMounted()) return;
      notify('Device name saved.', { type: 'success' });
      setEditing(null);
      await reload();
    } catch (caught: unknown) {
      if (!isMounted()) return;
      if (!(await logoutIfAccessDenied(caught)) && isMounted()) {
        notify('The device name could not be saved.', { type: 'error' });
      }
    } finally {
      operationRef.current = null;
      if (isMounted()) setBusy(null);
    }
  };

  const revoke = async (device: DeviceInfo) => {
    if (operationRef.current !== null) return;
    operationRef.current = 'revoke';
    setBusy('revoke');
    try {
      await deleteDevice(device.id);
      if (!isMounted()) return;
      notify('Device access revoked.', { type: 'success' });
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

  if (authRedirecting) return null;

  const isLocked = loading || busy !== null;
  return (
    <section aria-labelledby="devices-heading" className="space-y-5">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h2
            className="text-lg font-semibold text-foreground"
            id="devices-heading"
            ref={headingRef}
            tabIndex={-1}
          >Devices</h2>
          <p className="mt-1 text-sm text-muted">Sessions currently associated with this server.</p>
        </div>
        <Tooltip>
          <Button
            aria-label="Reload devices"
            isDisabled={isLocked}
            isIconOnly
            onPress={() => { void reload(); }}
            size="sm"
            variant="ghost"
          >
            <RefreshCw aria-hidden="true" className={`size-4${loading ? ' animate-spin' : ''}`} />
          </Button>
          <Tooltip.Content>Reload devices</Tooltip.Content>
        </Tooltip>
      </div>

      {loading && hasLoaded && (
        <p aria-live="polite" className="text-sm text-muted" role="status">Refreshing devices...</p>
      )}

      <AsyncContent
        empty={<EmptyDevices />}
        error={loadError}
        hasData={hasLoaded}
        isEmpty={hasLoaded && devices.length === 0}
        isPending={loading}
        loading={<DeviceSkeleton />}
        onRetry={() => { void reload(); }}
      >
        <ResponsiveCollection
          ariaLabel="Devices collection"
          desktop={<DeviceTable devices={devices} isLocked={isLocked} onRename={openRename} onRevoke={revoke} />}
          mobile={<DeviceMobileList devices={devices} isLocked={isLocked} onRename={openRename} onRevoke={revoke} />}
        />
      </AsyncContent>

      <RenameDeviceModal
        customName={customName}
        device={editing}
        isPending={busy === 'rename'}
        onClose={() => { setEditing(null); }}
        onNameChange={setCustomName}
        onSubmit={saveName}
      />
    </section>
  );
}

interface DeviceCollectionProps {
  devices: DeviceInfo[];
  isLocked: boolean;
  onRename: (device: DeviceInfo) => void;
  onRevoke: (device: DeviceInfo) => Promise<void>;
}

function DeviceTable({ devices, isLocked, onRename, onRevoke }: DeviceCollectionProps) {
  return (
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label="Devices" className="table-fixed">
          <Table.Header>
            <Table.Column isRowHeader>Device</Table.Column>
            <Table.Column>Application</Table.Column>
            <Table.Column>Last user</Table.Column>
            <Table.Column>Last activity</Table.Column>
            <Table.Column className="w-40 text-right">Actions</Table.Column>
          </Table.Header>
          <Table.Body>
            {devices.map((device) => (
              <Table.Row id={device.id} key={device.id}>
                <Table.Cell><DeviceName device={device} /></Table.Cell>
                <Table.Cell><span className="break-words">{device.appName} {device.appVersion}</span></Table.Cell>
                <Table.Cell><span className="break-words">{device.lastUserName}</span></Table.Cell>
                <Table.Cell>{formatAccessDate(device.dateLastActivity)}</Table.Cell>
                <Table.Cell>
                  <DeviceActions device={device} isLocked={isLocked} onRename={onRename} onRevoke={onRevoke} />
                </Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}

function DeviceMobileList({ devices, isLocked, onRename, onRevoke }: DeviceCollectionProps) {
  return (
    <ul aria-label="Devices mobile" className="divide-y divide-border border-y border-border">
      {devices.map((device) => {
        const label = effectiveName(device);
        return (
          <li aria-label={label} className="space-y-4 py-4" key={device.id}>
            <dl className="grid grid-cols-[6rem_minmax(0,1fr)] gap-x-3 gap-y-3 text-sm">
              <MobileField label="Device"><DeviceName device={device} /></MobileField>
              <MobileField label="Application"><span className="break-words">{device.appName} {device.appVersion}</span></MobileField>
              <MobileField label="Last user"><span className="break-words">{device.lastUserName}</span></MobileField>
              <MobileField label="Last activity">{formatAccessDate(device.dateLastActivity)}</MobileField>
            </dl>
            <DeviceActions device={device} isLocked={isLocked} onRename={onRename} onRevoke={onRevoke} />
          </li>
        );
      })}
    </ul>
  );
}

function DeviceName({ device }: { device: DeviceInfo }) {
  const hasCustomName = (device.customName?.trim().length ?? 0) > 0;
  return (
    <div className="min-w-0 py-1">
      <p className="break-words font-semibold text-foreground">{effectiveName(device)}</p>
      {hasCustomName && (
        <p className="break-words text-xs text-muted">{device.name}</p>
      )}
    </div>
  );
}

function DeviceActions({ device, isLocked, onRename, onRevoke }: Omit<DeviceCollectionProps, 'devices'> & { device: DeviceInfo }) {
  const label = effectiveName(device);
  return (
    <div aria-label={`Actions for ${label}`} className="flex justify-end gap-1">
      <Tooltip>
        <Button
          aria-label={`Edit ${label}`}
          isDisabled={isLocked}
          isIconOnly
          onPress={() => { onRename(device); }}
          size="sm"
          variant="ghost"
        >
          <Pencil aria-hidden="true" className="size-4" />
        </Button>
        <Tooltip.Content>Edit device name</Tooltip.Content>
      </Tooltip>
      <ConfirmDialog
        confirmLabel="Revoke device"
        description={<>Revoke all active sessions for <strong>{label}</strong>?</>}
        isPending={isLocked}
        onConfirm={() => onRevoke(device)}
        title="Revoke device"
        trigger={(
          <Button
            aria-label={`Revoke ${label}`}
            className="min-w-24"
            isDisabled={isLocked}
            size="sm"
            variant="danger-soft"
          >
            <Trash2 aria-hidden="true" className="size-4" />
            Revoke
          </Button>
        )}
      />
    </div>
  );
}

function RenameDeviceModal({
  customName,
  device,
  isPending,
  onClose,
  onNameChange,
  onSubmit,
}: {
  customName: string;
  device: DeviceInfo | null;
  isPending: boolean;
  onClose: () => void;
  onNameChange: (name: string) => void;
  onSubmit: (event?: SyntheticEvent<HTMLFormElement>) => void | Promise<void>;
}) {
  return (
    <Modal
      isOpen={device !== null}
      onOpenChange={(isOpen) => { if (!isOpen && !isPending) onClose(); }}
    >
      <Modal.Backdrop isDismissable={!isPending} isKeyboardDismissDisabled={isPending}>
        <Modal.Container placement="center" size="sm">
          <Modal.Dialog>
            <Modal.CloseTrigger aria-label="Close" isDisabled={isPending} />
            <Modal.Header>
              <Modal.Heading>Edit device name</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <p className="mb-4 text-sm text-muted">
                Set a recognizable name for {device === null ? 'this device' : effectiveName(device)}.
              </p>
              <form id="rename-device-form" onSubmit={(event) => { void onSubmit(event); }}>
                <TextField fullWidth name="customName">
                  <Label>Custom device name</Label>
                  <Input
                    autoFocus
                    disabled={isPending}
                    maxLength={256}
                    onChange={(event) => { onNameChange(event.target.value); }}
                    value={customName}
                  />
                </TextField>
              </form>
            </Modal.Body>
            <Modal.Footer>
              <Button isDisabled={isPending} onPress={onClose} variant="tertiary">Cancel</Button>
              <Button
                form="rename-device-form"
                isPending={isPending}
                type="submit"
              >
                {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <Save aria-hidden="true" className="size-4" />}
                <span className="inline-flex min-h-5 items-center">Save device name</span>
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

function DeviceSkeleton() {
  return (
    <div aria-label="Loading devices" className="space-y-3" role="status">
      <Skeleton className="h-10 w-full" />
      <Skeleton className="h-16 w-full" />
      <Skeleton className="h-16 w-full" />
    </div>
  );
}

function EmptyDevices() {
  return (
    <div className="border-y border-border py-10 text-center">
      <p className="font-medium text-foreground">No active devices</p>
      <p className="mt-1 text-sm text-muted">Signed-in devices will appear here.</p>
    </div>
  );
}

function effectiveName(device: DeviceInfo): string {
  const customName = device.customName?.trim();
  return customName === undefined || customName.length === 0 ? device.name : customName;
}

async function fetchDevices(signal: AbortSignal): Promise<LoadResult> {
  try {
    return { records: await listDevices(signal) };
  } catch (error: unknown) {
    return { error };
  }
}
