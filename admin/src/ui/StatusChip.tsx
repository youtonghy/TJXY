import { Chip } from '@heroui/react';
import type { ReactNode } from 'react';

export type StatusTone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger';

const statusColors = {
  neutral: 'default',
  accent: 'accent',
  success: 'success',
  warning: 'warning',
  danger: 'danger',
} as const;

export interface StatusChipProps {
  tone: StatusTone;
  children: ReactNode;
}

export function StatusChip({ tone, children }: StatusChipProps) {
  return (
    <Chip color={statusColors[tone]} size="sm" variant="soft">
      {children}
    </Chip>
  );
}
