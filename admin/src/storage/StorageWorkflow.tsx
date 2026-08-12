import { Button } from '@heroui/react';
import { Check, RotateCcw } from 'lucide-react';
import type { ReactNode } from 'react';

import { PageHeader } from '../ui/PageHeader';
import { useTranslate } from '../settings/i18n';

export type StoragePhase = 'authorize' | 'choose-folder' | 'review' | 'complete';

export interface StorageWorkflowProps {
  title: string;
  providerName: 'Google Drive' | 'OneDrive';
  phase: StoragePhase;
  isBusy: boolean;
  canRestart?: boolean;
  onRestart: () => void;
  children: ReactNode;
}

const phases: readonly { id: Exclude<StoragePhase, 'complete'>; label: string }[] = [
  { id: 'authorize', label: 'Authorize' },
  { id: 'choose-folder', label: 'Choose folder' },
  { id: 'review', label: 'Review' },
];

export function StorageWorkflow({
  title,
  providerName,
  phase,
  isBusy,
  canRestart = phase !== 'authorize',
  onRestart,
  children,
}: StorageWorkflowProps) {
  const tr = useTranslate();
  const currentIndex = phase === 'complete'
    ? phases.length
    : phases.findIndex((item) => item.id === phase);

  return (
    <div className="space-y-6">
      <PageHeader
        actions={canRestart ? (
          <Button isDisabled={isBusy} onPress={onRestart} size="sm" variant="secondary">
            <RotateCcw aria-hidden="true" className="size-4" />
            {tr('Restart authorization', '重新授权')}
          </Button>
        ) : undefined}
        description={tr(`Authorize ${providerName}, choose the exact folder, and review the binding before it is created.`, `授权${providerName}，选择准确的文件夹，并在创建前检查绑定信息。`)}
        title={title}
      />

      <ol
        aria-label={tr(`${providerName} setup progress`, `${providerName} 设置进度`)}
        className="grid grid-cols-1 gap-2 sm:grid-cols-3"
      >
        {phases.map((item, index) => {
          const isCurrent = phase === item.id;
          const isComplete = index < currentIndex || phase === 'complete';
          return (
            <li
              aria-current={isCurrent ? 'step' : undefined}
              className={[
                'flex min-h-14 items-center gap-3 border-l-2 px-3 py-2 sm:border-l-0 sm:border-t-2',
                isCurrent ? 'border-accent bg-accent/8 text-foreground' : 'border-border text-muted',
              ].join(' ')}
              key={item.id}
            >
              <span
                aria-hidden="true"
                className={[
                  'inline-flex size-6 shrink-0 items-center justify-center rounded-full border text-xs font-semibold',
                  isComplete
                    ? 'border-success bg-success text-success-foreground'
                    : isCurrent
                      ? 'border-accent bg-accent text-accent-foreground'
                      : 'border-border bg-surface text-muted',
                ].join(' ')}
              >
                {isComplete ? <Check className="size-3.5" /> : index + 1}
              </span>
              <span className="min-w-0">
                <span className="block text-sm font-semibold">{tr(item.label, item.id === 'authorize' ? '授权' : item.id === 'choose-folder' ? '选择文件夹' : '检查')}</span>
                <span className="block text-xs">
                  {isComplete ? tr('Complete', '已完成') : isCurrent ? tr('Current step', '当前步骤') : tr('Upcoming', '即将开始')}
                </span>
              </span>
            </li>
          );
        })}
      </ol>

      <div className="border-t border-border pt-6">{children}</div>
    </div>
  );
}
