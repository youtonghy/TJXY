import { Alert, Button, Input, Label, ListBox, Select, TextField } from '@heroui/react';
import { Plus } from 'lucide-react';
import { useMemo, useState } from 'react';
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
  onSave: () => unknown;
}) {
  const tr = useTranslate();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const [selectedValue, setSelectedValue] = useState(value);
  const options = useMemo(() => Array.from(new Set([value, 'https://tjxy.200461.xyz'].filter(Boolean))), [value]);

  function startEditing() {
    setDraft(value);
    setSelectedValue(value);
    setEditing(true);
  }

  return (
    <div className="space-y-3" data-server-selector="true">
      {!editing ? (
        <div className="flex items-end gap-2">
          <Select aria-label={tr('Server', '服务器')} className="min-w-0 flex-1" isRequired={required} value={value} onChange={(next) => { if (typeof next === 'string') { setSelectedValue(next); onChange(next); } }}>
            <Label>{tr('Server', '服务器')}</Label>
            <Select.Trigger><Select.Value /><Select.Indicator /></Select.Trigger>
            <Select.Popover><ListBox>{options.map((origin) => <ListBox.Item id={origin} key={origin} textValue={origin}>{origin}<ListBox.ItemIndicator /></ListBox.Item>)}</ListBox></Select.Popover>
          </Select>
          <Button aria-label={tr('Add server', '添加服务器')} isIconOnly onPress={startEditing} type="button" variant="secondary"><Plus aria-hidden className="size-5" /></Button>
        </div>
      ) : (
        <TextField fullWidth isRequired={required} name="server">
          <Label>{tr('Server address', '服务器地址')}</Label>
          <Input
            autoComplete="url"
            fullWidth
            placeholder="http://127.0.0.1:8096"
            value={draft}
            onChange={(event) => { setDraft(event.currentTarget.value); onChange(event.currentTarget.value); }}
          />
        </TextField>
      )}
      {error && <Alert status="danger"><Alert.Content><Alert.Description>{error}</Alert.Description></Alert.Content></Alert>}
      {ok && <p className="text-sm text-success">{tr('Server is reachable.', '服务器可访问。')}</p>}
      {editing && <div className="flex gap-2">
        <Button isDisabled={(pending ?? false) || !draft.trim()} type="button" variant="secondary" onPress={() => {
          const result = onSave();
          if (result instanceof Promise) {
            void result.then(() => { setEditing(false); setDraft(value); setSelectedValue(value); }).catch(() => undefined);
          } else {
            setEditing(false);
          }
        }}>
          {pending ? tr('Checking…', '正在检查…') : tr('Use server', '使用此服务器')}
        </Button>
        <Button isDisabled={pending} type="button" variant="tertiary" onPress={() => { setEditing(false); setDraft(selectedValue); onChange(selectedValue); }}>
          {tr('Cancel', '取消')}
        </Button>
      </div>}
    </div>
  );
}
