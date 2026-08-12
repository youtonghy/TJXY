import { StatusChip } from '../ui/StatusChip';
import { useTranslate } from '../settings/i18n';

export function UserStatus({ isAdministrator, isDisabled }: {
  isAdministrator: boolean;
  isDisabled: boolean;
}) {
  const tr = useTranslate();
  return (
    <div className="flex flex-wrap gap-2">
      <StatusChip tone={isAdministrator ? 'accent' : 'neutral'}>
        {isAdministrator ? tr('Administrator', '管理员') : tr('Standard', '普通用户')}
      </StatusChip>
      <StatusChip tone={isDisabled ? 'warning' : 'success'}>
        {isDisabled ? tr('Disabled', '已禁用') : tr('Enabled', '已启用')}
      </StatusChip>
    </div>
  );
}
