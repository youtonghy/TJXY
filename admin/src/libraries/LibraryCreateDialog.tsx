import {
  Button,
  Input,
  Label,
  ListBox,
  Modal,
  Select,
  Switch,
  TextField,
} from '@heroui/react';
import { LoaderCircle, Plus } from 'lucide-react';
import { useState, type SyntheticEvent } from 'react';

import type { CreateLibraryRequest, LibraryCollectionType, ScanProfile } from './libraryApi';
import { collectionOptions, scanProfileOptions } from './libraryUi';

export interface LibraryCreateDialogProps {
  isOpen: boolean;
  isPending: boolean;
  onClose: () => void;
  onCreate: (request: CreateLibraryRequest) => Promise<boolean>;
}

export function LibraryCreateDialog({
  isOpen,
  isPending,
  onClose,
  onCreate,
}: LibraryCreateDialogProps) {
  const [name, setName] = useState('');
  const [collectionType, setCollectionType] = useState<LibraryCollectionType>('mixed');
  const [scanProfile, setScanProfile] = useState<ScanProfile>('Lazy');
  const [enabled, setEnabled] = useState(true);

  const reset = () => {
    setName('');
    setCollectionType('mixed');
    setScanProfile('Lazy');
    setEnabled(true);
  };

  const close = () => {
    if (isPending) return;
    reset();
    onClose();
  };

  const submit = async (event: SyntheticEvent<HTMLFormElement>) => {
    event.preventDefault();
    const normalizedName = name.trim();
    if (isPending || normalizedName.length === 0) return;
    if (await onCreate({ name: normalizedName, collectionType, enabled, scanProfile })) {
      reset();
    }
  };

  return (
    <Modal isOpen={isOpen} onOpenChange={(nextOpen) => { if (!nextOpen) close(); }}>
      <Modal.Backdrop isDismissable={!isPending} isKeyboardDismissDisabled={isPending}>
        <Modal.Container placement="center" size="sm">
          <Modal.Dialog>
            <Modal.CloseTrigger aria-label="Close" isDisabled={isPending} />
            <Modal.Header>
              <Modal.Heading>Add library</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <form className="space-y-5" id="create-library-form" onSubmit={(event) => { void submit(event); }}>
                <TextField fullWidth isRequired name="libraryName">
                  <Label>Library name</Label>
                  <Input
                    autoFocus
                    disabled={isPending}
                    maxLength={256}
                    onChange={(event) => { setName(event.currentTarget.value); }}
                    value={name}
                  />
                </TextField>
                <OptionSelect
                  isDisabled={isPending}
                  label="Content type"
                  onChange={setCollectionType}
                  options={collectionOptions}
                  value={collectionType}
                />
                <OptionSelect
                  isDisabled={isPending}
                  label="Scan profile"
                  onChange={setScanProfile}
                  options={scanProfileOptions}
                  value={scanProfile}
                />
                <Switch isDisabled={isPending} isSelected={enabled} onChange={setEnabled}>
                  <Switch.Content>
                    <Switch.Control><Switch.Thumb /></Switch.Control>
                    Enabled
                  </Switch.Content>
                </Switch>
              </form>
            </Modal.Body>
            <Modal.Footer>
              <Button isDisabled={isPending} onPress={close} variant="tertiary">Cancel</Button>
              <Button
                form="create-library-form"
                isDisabled={name.trim().length === 0}
                isPending={isPending}
                type="submit"
              >
                {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <Plus aria-hidden="true" className="size-4" />}
                <span className="inline-flex min-h-5 items-center">Create library</span>
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function OptionSelect<T extends string>({
  isDisabled,
  label,
  onChange,
  options,
  value,
}: {
  isDisabled: boolean;
  label: string;
  onChange: (value: T) => void;
  options: readonly { value: T; label: string }[];
  value: T;
}) {
  return (
    <Select
      fullWidth
      isDisabled={isDisabled}
      onChange={(key) => { if (typeof key === 'string') onChange(key as T); }}
      value={value}
    >
      <Label>{label}</Label>
      <Select.Trigger>
        <Select.Value />
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox>
          {options.map((option) => (
            <ListBox.Item id={option.value} key={option.value} textValue={option.label}>
              {option.label}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}
