import {
  Alert,
  Button,
  Card,
  FieldError,
  Input,
  Label,
  ListBox,
  Select,
  Skeleton,
  Table,
  TextField,
  Tooltip,
} from '@heroui/react';
import { Segment } from '@heroui-pro/react/segment';
import {
  Activity,
  CirclePlay,
  GitBranch,
  ListPlus,
  LoaderCircle,
  RefreshCw,
  ScanSearch,
  Search,
  ShieldCheck,
  Tags,
  TriangleAlert,
  X,
} from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';

import { AsyncContent } from '../ui/AsyncContent';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { PageHeader } from '../ui/PageHeader';
import { StatusChip, type StatusTone } from '../ui/StatusChip';
import { useAuthoritativeLoad } from '../ui/useAuthoritativeLoad';
import { useTranslate } from '../settings/i18n';
import type {
  ScheduledTask,
  ScheduledTaskState,
  StorageRootOption,
  TaskJob,
  TaskJobOutcome,
  TaskJobStatus,
  TaskSnapshot,
} from './taskApi';
import {
  cancelScheduledTask,
  discoverTitles,
  expandItem,
  fullScanRoot,
  getTaskSnapshot,
  indexMediaSources,
  probeMedia,
  resolveMetadata,
  startScheduledTask,
  validateStorage,
} from './taskApi';

const POLL_INTERVAL_MS = 5_000;
type LoadResult = { snapshot: TaskSnapshot } | { error: unknown };
type CommandResult = 'succeeded' | 'handled' | 'failed';

const scheduledTones: Record<ScheduledTaskState, StatusTone> = {
  Idle: 'neutral',
  Running: 'accent',
};

const jobTones: Record<TaskJobStatus, StatusTone> = {
  Pending: 'neutral',
  Retrying: 'warning',
  Running: 'accent',
  Completed: 'success',
  Cancelled: 'neutral',
  Failed: 'danger',
};

export function TasksPage() {
  const tr = useTranslate();
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [snapshot, setSnapshot] = useState<TaskSnapshot>({ scheduled: [], jobs: [], roots: [] });
  const [hasLoaded, setHasLoaded] = useState(false);
  const [loadError, setLoadError] = useState<NonNullable<unknown> | null>(null);
  const [authRedirecting, setAuthRedirecting] = useState(false);
  const [manualRefreshPending, setManualRefreshPending] = useState(false);
  const [selectedRoot, setSelectedRoot] = useState('');
  const [itemId, setItemId] = useState('');
  const [busyOperations, setBusyOperations] = useState<ReadonlySet<string>>(() => new Set());
  const operationRef = useRef(new Set<string>());

  const prepareLoadResult = useCallback(async (result: LoadResult) => {
    if ('snapshot' in result) {
      return () => {
        setSnapshot(result.snapshot);
        setSelectedRoot((current) => (
          result.snapshot.roots.some((root) => root.key === current)
            ? current
            : result.snapshot.roots[0]?.key ?? ''
        ));
        setHasLoaded(true);
        setLoadError(null);
        setAuthRedirecting(false);
      };
    }
    if (await logoutIfAccessDenied(result.error)) {
      return () => { setAuthRedirecting(true); };
    }
    return () => {
      setLoadError(result.error ?? new Error('Task loading failed.'));
    };
  }, [logoutIfAccessDenied]);

  const {
    isMounted,
    loading,
    refreshWhenIdle,
    reload,
  } = useAuthoritativeLoad(fetchTaskSnapshot, prepareLoadResult);

  const reloadManually = useCallback(async () => {
    if (!isMounted()) return;
    setManualRefreshPending(true);
    try {
      await reload();
    } finally {
      if (isMounted()) setManualRefreshPending(false);
    }
  }, [isMounted, reload]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void refreshWhenIdle().catch(() => undefined);
    }, POLL_INTERVAL_MS);
    return () => { window.clearInterval(timer); };
  }, [refreshWhenIdle]);

  const setOperationBusy = (operation: string, isBusy: boolean) => {
    if (isBusy) operationRef.current.add(operation);
    else operationRef.current.delete(operation);
    setBusyOperations(new Set(operationRef.current));
  };

  const run = async (
    operation: string,
    command: () => Promise<unknown>,
    success: string,
    failureFeedback: 'toast' | 'inline' = 'toast',
  ): Promise<CommandResult> => {
    if (operationRef.current.has(operation)) return 'failed';
    setOperationBusy(operation, true);
    try {
      const jobIds = await command();
      if (!isMounted()) return 'handled';
      const suffix = Array.isArray(jobIds) && jobIds.length > 0
        ? ` ${String(jobIds.length)} durable job${jobIds.length === 1 ? '' : 's'} accepted.`
        : '';
      notify(`${success}${suffix}`, { type: 'success' });
      await reload();
      return 'succeeded';
    } catch (error: unknown) {
      if (!isMounted()) return 'handled';
      if (await logoutIfAccessDenied(error)) return 'handled';
      if (!isMounted()) return 'handled';
      if (failureFeedback === 'toast') {
        notify('The task command could not be completed.', { type: 'error' });
      }
      return 'failed';
    } finally {
      if (isMounted()) setOperationBusy(operation, false);
      else operationRef.current.delete(operation);
    }
  };

  if (authRedirecting) return null;

  const selectedRootOption = snapshot.roots.find((root) => root.key === selectedRoot);
  const reloadButtonPending = hasLoaded ? manualRefreshPending : loading;
  return (
    <div className="space-y-7">
      <PageHeader
        actions={(
          <Tooltip>
            <Button
              aria-label={tr('Reload tasks', '刷新任务')}
              isIconOnly
              isPending={reloadButtonPending}
              onPress={() => { void reloadManually(); }}
              size="sm"
              variant="ghost"
            >
              <RefreshCw aria-hidden="true" className={`size-4${reloadButtonPending ? ' animate-spin' : ''}`} />
            </Button>
            <Tooltip.Content>{tr('Reload tasks', '刷新任务')}</Tooltip.Content>
          </Tooltip>
        )}
        description={tr('Run scheduled maintenance, submit targeted work, and inspect durable job history.', '运行计划维护、提交指定任务并查看持久任务历史。')}
        title="Tasks"
      />

      {manualRefreshPending && (
        <p aria-live="polite" className="sr-only" role="status">{tr('Refreshing tasks...', '正在刷新任务...')}</p>
      )}

      <AsyncContent
        empty={null}
        error={loadError}
        hasData={hasLoaded}
        isEmpty={false}
        isPending={loading}
        loading={<TasksSkeleton />}
        onRetry={() => { void reloadManually(); }}
      >
        <div className="space-y-10">
          <ScheduledTasks
            busyOperations={busyOperations}
            onRun={run}
            tasks={snapshot.scheduled}
          />
          <ManualCommands
            busyOperations={busyOperations}
            itemId={itemId}
            onItemIdChange={setItemId}
            onRootChange={setSelectedRoot}
            onRun={run}
            roots={snapshot.roots}
            selectedRoot={selectedRoot}
            selectedRootOption={selectedRootOption}
          />
          <RecentJobs jobs={snapshot.jobs} />
        </div>
      </AsyncContent>
    </div>
  );
}

function ScheduledTasks({
  busyOperations,
  onRun,
  tasks,
}: {
  busyOperations: ReadonlySet<string>;
  onRun: (
    operation: string,
    command: () => Promise<unknown>,
    success: string,
    failureFeedback?: 'toast' | 'inline',
  ) => Promise<CommandResult>;
  tasks: ScheduledTask[];
}) {
  const tr = useTranslate();
  return (
    <section aria-labelledby="scheduled-tasks-heading" className="space-y-4">
      <SectionHeading
        description={tr('Server maintenance routines reported by the scheduler.', '调度器报告的服务器维护例程。')}
        id="scheduled-tasks-heading"
        title={tr('Scheduled tasks', '计划任务')}
      />
      {tasks.length === 0 ? (
        <EmptyState message={tr('No scheduled tasks are available.', '暂无可用的计划任务。')} />
      ) : (
        <ul aria-label={tr('Scheduled tasks', '计划任务')} className="divide-y divide-border border-y border-border">
          {tasks.map((task) => {
            const operation = `scheduled-${task.id}`;
            const isPending = busyOperations.has(operation);
            return (
              <li className="grid min-w-0 gap-4 py-4 md:grid-cols-[minmax(0,1fr)_10rem_7rem_auto] md:items-center" key={task.id}>
                <div className="min-w-0">
                  <p className="break-words font-semibold text-foreground">{task.name}</p>
                  <p className="mt-1 break-words text-sm text-muted">{task.description}</p>
                  <p className="mt-1 break-all font-mono text-xs text-muted">{task.key}</p>
                </div>
                <LabeledValue label={tr('Category', '类别')}>{task.category}</LabeledValue>
                <LabeledValue label={tr('Status', '状态')}><TaskStatus state={task.state} /></LabeledValue>
                <div className="flex justify-end">
                  {task.state === 'Running' ? (
                    <ConfirmDialog
                      confirmLabel={tr('Cancel task', '取消任务')}
                      description={<>{tr('Stop the active ', '停止正在运行的')}<strong>{task.name}</strong>{tr(' task?', '任务？')}</>}
                      isPending={isPending}
                      onConfirm={async () => {
                        const result = await onRun(
                          operation,
                          () => cancelScheduledTask(task.id),
                          'Scheduled task cancelled.',
                          'inline',
                        );
                        if (result === 'failed') throw new Error('Task cancellation failed.');
                        if (result === 'succeeded') focusScheduledHeading();
                      }}
                      title={tr('Cancel scheduled task', '取消计划任务')}
                      trigger={(
                        <Button
                          aria-label={`${tr('Cancel', '取消')} ${task.name}`}
                          className="min-w-24"
                          isPending={isPending}
                          size="sm"
                          variant="danger-soft"
                        >
                          {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <X aria-hidden="true" className="size-4" />}
                          <span className="inline-flex min-h-5 items-center">{tr('Cancel', '取消')}</span>
                        </Button>
                      )}
                    />
                  ) : (
                    <Button
                      aria-label={`${tr('Start', '启动')} ${task.name}`}
                      className="min-w-24"
                      isPending={isPending}
                      onPress={() => {
                        void onRun(
                          operation,
                          () => startScheduledTask(task.id),
                          'Scheduled task started.',
                        ).then((result) => {
                          if (result === 'succeeded') focusScheduledHeading();
                        });
                      }}
                      size="sm"
                      variant="secondary"
                    >
                      {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <CirclePlay aria-hidden="true" className="size-4" />}
                      <span className="inline-flex min-h-5 items-center">{tr('Start', '启动')}</span>
                    </Button>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

function ManualCommands({
  busyOperations,
  itemId,
  onItemIdChange,
  onRootChange,
  onRun,
  roots,
  selectedRoot,
  selectedRootOption,
}: {
  busyOperations: ReadonlySet<string>;
  itemId: string;
  onItemIdChange: (value: string) => void;
  onRootChange: (value: string) => void;
  onRun: (operation: string, command: () => Promise<unknown>, success: string) => Promise<CommandResult>;
  roots: StorageRootOption[];
  selectedRoot: string;
  selectedRootOption: StorageRootOption | undefined;
}) {
  const tr = useTranslate();
  const [commandTarget, setCommandTarget] = useState<'root' | 'item'>('root');
  const run = (operation: string, command: () => Promise<unknown>, success: string) => {
    void onRun(operation, command, success);
  };
  const validItem = isUuid(itemId);
  return (
    <section aria-labelledby="manual-commands-heading">
      <Card className="gap-0 overflow-hidden p-0" variant="secondary">
        <Card.Header className="border-b border-border p-5 sm:p-6">
          <div>
            <Card.Title
              className="text-lg"
              id="manual-commands-heading"
              render={(props) => <h2 {...props} />}
              tabIndex={-1}
            >
              {tr('Manual commands', '手动命令')}
            </Card.Title>
            <Card.Description className="mt-1">{tr('Submit focused jobs without waiting for a scheduled maintenance cycle.', '无需等待计划维护周期即可提交指定任务。')}</Card.Description>
          </div>
        </Card.Header>
        <Card.Content className="space-y-6 p-5 sm:p-6">
          <Segment
            aria-label={tr('Command target', '命令目标')}
            className="w-full sm:max-w-sm"
            onSelectionChange={(key) => {
              if (key === 'root' || key === 'item') setCommandTarget(key as 'root' | 'item');
            }}
            selectedKey={commandTarget}
          >
            <Segment.Item className="min-w-0 flex-1" id="root">{tr('Library root', '媒体库根目录')}</Segment.Item>
            <Segment.Item className="min-w-0 flex-1" id="item">{tr('Catalog item', '目录项目')}</Segment.Item>
          </Segment>

          {commandTarget === 'root' ? (
            <section aria-labelledby="root-commands-heading" className="space-y-5">
              <div>
                <h3 className="font-semibold text-foreground" id="root-commands-heading">{tr('Library root', '媒体库根目录')}</h3>
                <p className="mt-1 text-sm text-muted">{tr('Run discovery or maintenance against one attached storage root.', '针对一个已连接的存储根目录运行发现或维护。')}</p>
              </div>
              {roots.length === 0 ? (
                <Alert status="accent">
                  <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
                  <Alert.Content>
                    <Alert.Title>{tr('No library roots', '没有媒体库根目录')}</Alert.Title>
                    <Alert.Description>{tr('No storage roots are attached to a library.', '媒体库尚未连接存储根目录。')}</Alert.Description>
                  </Alert.Content>
                </Alert>
              ) : (
                <Select fullWidth onChange={(key) => { if (typeof key === 'string') onRootChange(key); }} value={selectedRoot}>
                  <Label>{tr('Library root', '媒体库根目录')}</Label>
                  <Select.Trigger>
                    <Select.Value />
                    <Select.Indicator />
                  </Select.Trigger>
                  <Select.Popover>
                    <ListBox>
                      {roots.map((root) => (
                        <ListBox.Item id={root.key} key={root.key} textValue={root.label}>
                          {root.label}
                          <ListBox.ItemIndicator />
                        </ListBox.Item>
                      ))}
                    </ListBox>
                  </Select.Popover>
                </Select>
              )}
              <div className="space-y-3">
                <CommandButton
                  icon={<ScanSearch aria-hidden="true" className="size-4" />}
                  isDisabled={selectedRootOption === undefined}
                  isPending={busyOperations.has('full-scan-root')}
                  label={tr('Full scan', '完整扫描')}
                  onPress={() => {
                    run(
                      'full-scan-root',
                      () => fullScanRoot(
                        selectedRootOption?.libraryId ?? '',
                        selectedRootOption?.storageRootId ?? '',
                      ),
                      tr('Full scan submitted.', '完整扫描已提交。'),
                    );
                  }}
                  variant="primary"
                />
                <div className="grid gap-3 sm:grid-cols-2">
                  <CommandButton
                    icon={<ShieldCheck aria-hidden="true" className="size-4" />}
                    isDisabled={selectedRootOption === undefined}
                    isPending={busyOperations.has('validate-storage')}
                    label={tr('Validate storage', '验证存储')}
                    onPress={() => {
                      run(
                        'validate-storage',
                        () => validateStorage(selectedRootOption?.storageRootId ?? ''),
                        tr('Storage validation submitted.', '存储验证已提交。'),
                      );
                    }}
                  />
                  <CommandButton
                    icon={<Search aria-hidden="true" className="size-4" />}
                    isDisabled={selectedRootOption === undefined}
                    isPending={busyOperations.has('discover-titles')}
                    label={tr('Discover titles', '发现标题')}
                    onPress={() => {
                      run(
                        'discover-titles',
                        () => discoverTitles(selectedRootOption?.storageRootId ?? ''),
                        tr('Title discovery submitted.', '标题发现已提交。'),
                      );
                    }}
                  />
                </div>
              </div>
            </section>
          ) : (
            <section aria-labelledby="item-commands-heading" className="space-y-5">
              <div>
                <h3 className="font-semibold text-foreground" id="item-commands-heading">{tr('Catalog item', '目录项目')}</h3>
                <p className="mt-1 text-sm text-muted">{tr('Submit metadata and media operations for one indexed catalog item.', '为一个已索引的目录项目提交元数据和媒体操作。')}</p>
              </div>
              <TextField
                fullWidth
                isInvalid={itemId.length > 0 && !validItem}
                name="catalogItemId"
              >
                <Label>{tr('Catalog item ID', '目录项目 ID')}</Label>
                <Input
                  maxLength={64}
                  onChange={(event) => { onItemIdChange(event.currentTarget.value); }}
                  placeholder={tr('Enter a catalog item UUID', '输入目录项目 UUID')}
                  value={itemId}
                />
                <FieldError>{tr('Enter a valid UUID.', '请输入有效的 UUID。')}</FieldError>
              </TextField>
              <div className="grid gap-3 sm:grid-cols-2">
                <CommandButton
                  icon={<Tags aria-hidden="true" className="size-4" />}
                  isDisabled={!validItem}
                  isPending={busyOperations.has('resolve-metadata')}
                  label={tr('Resolve metadata', '解析元数据')}
                  onPress={() => {
                    run('resolve-metadata', () => resolveMetadata(itemId), tr('Metadata resolution submitted.', '元数据解析已提交。'));
                  }}
                />
                <CommandButton
                  icon={<GitBranch aria-hidden="true" className="size-4" />}
                  isDisabled={!validItem}
                  isPending={busyOperations.has('expand-item')}
                  label={tr('Expand item', '展开项目')}
                  onPress={() => {
                    run('expand-item', () => expandItem(itemId), tr('Item expansion submitted.', '项目展开已提交。'));
                  }}
                />
                <CommandButton
                  icon={<ListPlus aria-hidden="true" className="size-4" />}
                  isDisabled={!validItem}
                  isPending={busyOperations.has('index-media-sources')}
                  label={tr('Index sources', '索引来源')}
                  onPress={() => {
                    run('index-media-sources', () => indexMediaSources(itemId), tr('Source indexing submitted.', '来源索引已提交。'));
                  }}
                />
                <CommandButton
                  icon={<Activity aria-hidden="true" className="size-4" />}
                  isDisabled={!validItem}
                  isPending={busyOperations.has('probe-media')}
                  label={tr('Probe media', '探测媒体')}
                  onPress={() => {
                    run('probe-media', () => probeMedia(itemId), tr('Media probe submitted.', '媒体探测已提交。'));
                  }}
                />
              </div>
            </section>
          )}
        </Card.Content>
      </Card>
    </section>
  );
}

function CommandButton({
  icon,
  isDisabled,
  isPending,
  label,
  onPress,
  variant = 'secondary',
}: {
  icon: ReactNode;
  isDisabled: boolean;
  isPending: boolean;
  label: string;
  onPress: () => void;
  variant?: 'primary' | 'secondary';
}) {
  return (
    <Button
      fullWidth
      isDisabled={isDisabled}
      isPending={isPending}
      onPress={onPress}
      variant={variant}
    >
      {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : icon}
      <span className="inline-flex min-h-5 items-center">{label}</span>
    </Button>
  );
}

function RecentJobs({ jobs }: { jobs: TaskJob[] }) {
  const tr = useTranslate();
  return (
    <section aria-labelledby="recent-jobs-heading" className="space-y-4">
      <SectionHeading
        description={tr('The latest durable submissions and their terminal outcomes.', '最新的持久任务提交及其最终结果。')}
        id="recent-jobs-heading"
        title={tr('Recent durable jobs', '最近的持久任务')}
      />
      {jobs.length === 0 ? (
        <EmptyState message={tr('No durable jobs have been submitted.', '尚未提交持久任务。')} />
      ) : (
        <Table variant="secondary">
          <Table.ScrollContainer className="max-h-[32rem] overflow-auto">
            <Table.Content aria-label={tr('Recent durable jobs', '最近的持久任务')} className="min-w-[52rem] table-fixed">
              <Table.Header>
                <Table.Column isRowHeader>{tr('Task', '任务')}</Table.Column>
                <Table.Column>{tr('Scope', '范围')}</Table.Column>
                <Table.Column>{tr('Status', '状态')}</Table.Column>
                <Table.Column>{tr('Result', '结果')}</Table.Column>
                <Table.Column className="w-24 text-right">{tr('Attempts', '尝试次数')}</Table.Column>
                <Table.Column>{tr('Created', '创建时间')}</Table.Column>
                <Table.Column>{tr('Finished', '完成时间')}</Table.Column>
              </Table.Header>
              <Table.Body>
                {jobs.map((job) => (
                  <Table.Row id={job.id} key={job.id}>
                    <Table.Cell>
                      <p className="break-words font-medium">{readableIdentifier(job.taskKind)}</p>
                      <p className="break-all font-mono text-xs text-muted">{job.taskKind}</p>
                    </Table.Cell>
                    <Table.Cell>
                      <p>{readableIdentifier(job.scopeType)}</p>
                      <p className="break-all text-xs text-muted">{job.scopeId}</p>
                    </Table.Cell>
                    <Table.Cell><JobStatus status={job.status} /></Table.Cell>
                    <Table.Cell><JobOutcome outcome={job.outcome} /></Table.Cell>
                    <Table.Cell><span className="block text-right tabular-nums">{job.attemptCount}</span></Table.Cell>
                    <Table.Cell>{formatDate(job.createdAt)}</Table.Cell>
                    <Table.Cell>{formatDate(job.completedAt)}</Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      )}
    </section>
  );
}

function SectionHeading({ id, title, description }: { id: string; title: string; description: string }) {
  return (
    <div>
      <h2 className="text-lg font-semibold text-foreground" id={id} tabIndex={-1}>{title}</h2>
      <p className="mt-1 text-sm text-muted">{description}</p>
    </div>
  );
}

function LabeledValue({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[6rem_minmax(0,1fr)] items-center gap-3 text-sm md:block">
      <span className="font-medium text-muted md:sr-only">{label}</span>
      <span className="min-w-0 text-foreground">{children}</span>
    </div>
  );
}

function TaskStatus({ state }: { state: ScheduledTaskState }) {
  const tr = useTranslate();
  const tone = scheduledTones[state];
  return <span data-tone={tone}><StatusChip tone={tone}>{tr(state, state === 'Running' ? '运行中' : '空闲')}</StatusChip></span>;
}

function JobStatus({ status }: { status: TaskJobStatus }) {
  const tr = useTranslate();
  const tone = jobTones[status];
  const labels: Record<TaskJobStatus, string> = { Pending: '等待中', Retrying: '重试中', Running: '运行中', Completed: '已完成', Cancelled: '已取消', Failed: '失败' };
  return <span data-tone={tone}><StatusChip tone={tone}>{tr(status, labels[status])}</StatusChip></span>;
}

function JobOutcome({ outcome }: { outcome: TaskJobOutcome | null }) {
  const tr = useTranslate();
  if (outcome === null) return <span className="text-muted">-</span>;
  return (
    <StatusChip tone="warning">
      {outcome === 'NoMetadataMatch' ? tr('No remote metadata match', '没有匹配的远程元数据') : tr('Completed with warnings', '已完成，但有警告')}
    </StatusChip>
  );
}

function EmptyState({ message }: { message: string }) {
  return <p className="border-y border-border py-8 text-center text-sm text-muted">{message}</p>;
}

function TasksSkeleton() {
  const tr = useTranslate();
  return (
    <div aria-label={tr('Loading tasks', '正在加载任务')} className="space-y-8" role="status">
      <div className="space-y-3">
        <Skeleton className="h-6 w-48" />
        <Skeleton className="h-20 w-full" />
        <Skeleton className="h-20 w-full" />
      </div>
      <div className="grid gap-6 lg:grid-cols-2">
        <Skeleton className="h-48 w-full" />
        <Skeleton className="h-48 w-full" />
      </div>
      <Skeleton className="h-56 w-full" />
    </div>
  );
}

function formatDate(value: string | null): string {
  return value === null ? 'Not finished' : new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(value));
}

function readableIdentifier(value: string): string {
  return value
    .replace(/([a-z0-9])([A-Z])/gu, '$1 $2')
    .replace(/[_-]+/gu, ' ')
    .replace(/\s+/gu, ' ')
    .trim();
}

function isUuid(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(value.trim());
}

function focusScheduledHeading() {
  window.setTimeout(() => {
    document.getElementById('scheduled-tasks-heading')?.focus();
  }, 0);
}

async function fetchTaskSnapshot(signal: AbortSignal): Promise<LoadResult> {
  try {
    return { snapshot: await getTaskSnapshot(signal) };
  } catch (error: unknown) {
    return { error };
  }
}
