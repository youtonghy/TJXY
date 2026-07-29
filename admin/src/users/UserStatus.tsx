import { StatusChip } from '../ui/StatusChip';

export function UserStatus({ isAdministrator, isDisabled }: {
  isAdministrator: boolean;
  isDisabled: boolean;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      <StatusChip tone={isAdministrator ? 'accent' : 'neutral'}>
        {isAdministrator ? 'Administrator' : 'Standard'}
      </StatusChip>
      <StatusChip tone={isDisabled ? 'warning' : 'success'}>
        {isDisabled ? 'Disabled' : 'Enabled'}
      </StatusChip>
    </div>
  );
}
