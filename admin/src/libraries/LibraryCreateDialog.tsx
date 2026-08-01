import {
  Button,
  Input,
  Label,
  ListBox,
  Modal,
  Radio,
  RadioGroup,
  Select,
  Switch,
  TextField,
} from '@heroui/react';
import { FolderOpen, LoaderCircle, Plus } from 'lucide-react';
import { useState, type SyntheticEvent } from 'react';

import { FolderPickerDialog } from './FolderPickerDialog';
import type { FilesystemSelection } from './filesystemApi';
import type {
  CreateLibraryRequest,
  LibraryCollectionType,
  MetadataSourceMode,
  ScanProfile,
} from './libraryApi';
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
  const [collectionType, setCollectionType] = useState<LibraryCollectionType>('movies');
  const [scanProfile, setScanProfile] = useState<ScanProfile>('Lazy');
  const [metadataSourceMode, setMetadataSourceMode] = useState<MetadataSourceMode>('automatic_scrape');
  const [filesystemSelection, setFilesystemSelection] = useState<FilesystemSelection | null>(null);
  const [folderLabel, setFolderLabel] = useState('');
  const [folderPickerOpen, setFolderPickerOpen] = useState(false);
  const [enabled, setEnabled] = useState(true);

  const reset = () => {
    setName('');
    setCollectionType('movies');
    setScanProfile('Lazy');
    setMetadataSourceMode('automatic_scrape');
    setFilesystemSelection(null);
    setFolderLabel('');
    setFolderPickerOpen(false);
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
    if (isPending || normalizedName.length === 0 || filesystemSelection === null) return;
    if (await onCreate({
      name: normalizedName,
      collectionType,
      enabled,
      scanProfile,
      metadataSourceMode,
      filesystemSelection,
    })) {
      reset();
    }
  };

  return (
    <>
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
                  options={collectionOptions.filter((option) => option.value === 'movies' || option.value === 'tvshows')}
                  value={collectionType}
                />
                <div className="space-y-2">
                  <span className="text-sm font-medium text-foreground">Media folder</span>
                  <div className="flex min-h-12 items-center gap-3 border border-border px-3">
                    <FolderOpen aria-hidden="true" className="size-5 shrink-0 text-accent" />
                    <span className={`min-w-0 flex-1 break-words text-sm ${folderLabel.length > 0 ? 'text-foreground' : 'text-muted'}`}>
                      {folderLabel.length > 0 ? folderLabel : 'No folder selected'}
                    </span>
                    <Button isDisabled={isPending} onPress={() => { setFolderPickerOpen(true); }} size="sm" variant="secondary">
                      Browse
                    </Button>
                  </div>
                </div>
                <RadioGroup
                  isDisabled={isPending}
                  onChange={(value) => { setMetadataSourceMode(value as MetadataSourceMode); }}
                  value={metadataSourceMode}
                >
                  <Label>Metadata source</Label>
                  <Radio value="automatic_scrape">
                    <Radio.Control><Radio.Indicator /></Radio.Control>
                    <Radio.Content>
                      <span className="font-medium">Automatic scrape</span>
                      <span className="text-sm text-muted">Use local NFO and artwork first, then fill gaps from TMDB.</span>
                    </Radio.Content>
                  </Radio>
                  <Radio value="local_only">
                    <Radio.Control><Radio.Indicator /></Radio.Control>
                    <Radio.Content>
                      <span className="font-medium">Local metadata only</span>
                      <span className="text-sm text-muted">Import NFO and local artwork without remote metadata requests.</span>
                    </Radio.Content>
                  </Radio>
                </RadioGroup>
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
                isDisabled={name.trim().length === 0 || filesystemSelection === null}
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
      <FolderPickerDialog
        isDisabled={isPending}
        isOpen={folderPickerOpen}
        onClose={() => { setFolderPickerOpen(false); }}
        onSelect={(selection, displayPath) => {
          setFilesystemSelection(selection);
          setFolderLabel(displayPath);
        }}
      />
    </>
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
