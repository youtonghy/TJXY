import { Alert, Button, Input, Label, TextField } from '@heroui/react';
import { useTranslate } from '../../settings/i18n';

export function ServerAddressField({
  required,
  value,
  pending,
  error,
  ok,
  onChange,
  onSave,
}: {
  required?: boolean;
  value: string;
  pending?: boolean;
  error?: string;
  ok?: boolean;
  onChange: (value: string) => void;
  onSave: () => void;
}) {
  const tr = useTranslate();
  return (
    <div className="space-y-3">
      <TextField fullWidth isRequired={required} name="server">
        <Label>{tr('Server address', '服务器地址')}</Label>
        <Input
          autoComplete="url"
          fullWidth
          placeholder="http://127.0.0.1:8096"
          value={value}
          onChange={(event) => { onChange(event.currentTarget.value); }}
        />
      </TextField>
      {error && <Alert status="danger"><Alert.Content><Alert.Description>{error}</Alert.Description></Alert.Content></Alert>}
      {ok && <p className="text-sm text-success">{tr('Server is reachable.', '服务器可访问。')}</p>}
      <Button isDisabled={pending || !value.trim()} type="button" variant="secondary" onPress={onSave}>
        {pending ? tr('Checking…', '正在检查…') : tr('Save server', '保存服务器')}
      </Button>
    </div>
  );
}
