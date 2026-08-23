import { ProgressBar } from '@heroui/react';

import { useTranslate } from '../settings/i18n';
import { passwordStrengthScore } from './passwordStrengthUtils';

export function PasswordStrength({ password }: { password: string }) {
  const tr = useTranslate();
  const score = passwordStrengthScore(password);
  const label = password.length === 0
    ? tr('Enter a password to see its strength.', '输入密码后查看强度。')
    : score <= 2
      ? tr('Weak password', '密码强度较弱')
      : score <= 4
        ? tr('Moderate password', '密码强度中等')
        : tr('Strong password', '密码强度较强');
  const color = score <= 2 ? 'danger' : score <= 4 ? 'accent' : 'success';

  return (
    <div aria-live="polite" className="space-y-2">
      <div className="flex items-center justify-between gap-3 text-xs">
        <span className="font-medium text-foreground">{tr('Password strength', '密码强度')}</span>
        <span className="text-muted">{label}</span>
      </div>
      <ProgressBar aria-label={tr('Password strength', '密码强度')} color={color} maxValue={5} size="sm" value={score}>
        <ProgressBar.Track><ProgressBar.Fill /></ProgressBar.Track>
      </ProgressBar>
      <p className="text-xs text-muted">{tr('Use at least 8 characters with upper/lowercase letters, a number, and a symbol.', '至少使用 8 个字符，并包含大小写字母、数字和特殊字符。')}</p>
    </div>
  );
}
