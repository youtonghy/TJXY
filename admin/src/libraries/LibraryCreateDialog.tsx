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
import { LoaderCircle, Plus } from 'lucide-react';
import { useState, type SyntheticEvent } from 'react';

import type {
  CreateLibraryRequest,
  LibraryCollectionType,
  MetadataSourceMode,
  ScanProfile,
} from './libraryApi';
import { localMetadataAccessMode } from './libraryApi';
import { collectionOptions, scanProfileOptions } from './libraryUi';
import { useTranslate } from '../settings/i18n';

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
  const tr = useTranslate();
  const [name, setName] = useState('');
  const [collectionType, setCollectionType] = useState<LibraryCollectionType>('movies');
  const [scanProfile, setScanProfile] = useState<ScanProfile>('Lazy');
  const [metadataSourceMode, setMetadataSourceMode] = useState<MetadataSourceMode>('automatic_scrape');
  const [importMetadata, setImportMetadata] = useState(true);
  const [importImages, setImportImages] = useState(true);
  const [path, setPath] = useState('');
  const [enabled, setEnabled] = useState(true);

  const reset = () => {
    setName('');
    setCollectionType('movies');
    setScanProfile('Lazy');
    setMetadataSourceMode('automatic_scrape');
    setImportMetadata(true);
    setImportImages(true);
    setPath('');
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
    const normalizedPath = path.trim();
    if (isPending || normalizedName.length === 0 || normalizedPath.length === 0) return;
    if (await onCreate({
      name: normalizedName,
      collectionType,
      enabled,
      scanProfile,
      metadataSourceMode,
      localMetadataAccessMode: localMetadataAccessMode(importMetadata, importImages),
      path: normalizedPath,
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
            <Modal.CloseTrigger aria-label={tr('Close', '关闭')} isDisabled={isPending} />
            <Modal.Header>
              <Modal.Heading>{tr('Add library', '添加媒体库')}</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <form className="space-y-5" id="create-library-form" onSubmit={(event) => { void submit(event); }}>
                <TextField fullWidth isRequired name="libraryName">
                  <Label>{tr('Library name', '媒体库名称')}</Label>
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
                  label={tr('Content type', '内容类型')}
                  onChange={setCollectionType}
                  options={collectionOptions.filter((option) => (
                    option.value === 'movies' || option.value === 'tvshows' || option.value === 'music'
                  ))}
                  value={collectionType}
                />
                <TextField fullWidth isRequired name="mediaPath">
                  <Label>{tr('Media folder', '媒体文件夹')}</Label>
                  <div className="flex min-w-0 gap-2">
                    <Input
                      className="min-w-0 flex-1"
                      disabled={isPending}
                      maxLength={4096}
                      onChange={(event) => {
                        setPath(event.currentTarget.value);
                      }}
                      placeholder={tr('/mnt/media or choose a server folder', '/mnt/media 或选择服务器文件夹')}
                      value={path}
                    />
                  </div>
                </TextField>
                <RadioGroup
                  isDisabled={isPending}
                  onChange={(value) => {
                    const mode = value as MetadataSourceMode;
                    setMetadataSourceMode(mode);
                    if (mode === 'automatic_scrape') {
                      setImportMetadata(true);
                      setImportImages(true);
                    }
                  }}
                  value={metadataSourceMode}
                >
                  <Label>{tr('Metadata source', '元数据来源')}</Label>
                  <Radio value="automatic_scrape">
                    <Radio.Control><Radio.Indicator /></Radio.Control>
                    <Radio.Content>
                      <span className="font-medium">{tr('Automatic scrape', '自动抓取')}</span>
                      <span className="text-sm text-muted">{tr('Use local NFO and artwork first, then fill gaps from TMDB.', '优先使用本地 NFO 和图片，再从 TMDB 补充缺失字段。')}</span>
                    </Radio.Content>
                  </Radio>
                  <Radio value="local_only">
                    <Radio.Control><Radio.Indicator /></Radio.Control>
                    <Radio.Content>
                      <span className="font-medium">{tr('Local metadata only', '仅本地元数据')}</span>
                      <span className="text-sm text-muted">{tr('Import NFO and local artwork without remote metadata requests.', '导入 NFO 和本地图片，不请求远程元数据。')}</span>
                    </Radio.Content>
                  </Radio>
                </RadioGroup>
                {metadataSourceMode === 'local_only' && (
                  <div className="space-y-3 rounded-lg border border-border p-4">
                    <Label>{tr('Local imports', '本地导入')}</Label>
                    <Switch isSelected={importMetadata} onChange={setImportMetadata}>
                      <Switch.Content><Switch.Control><Switch.Thumb /></Switch.Control>{tr('Import NFO metadata', '导入 NFO 元数据')}</Switch.Content>
                    </Switch>
                    <Switch isSelected={importImages} onChange={setImportImages}>
                      <Switch.Content><Switch.Control><Switch.Thumb /></Switch.Control>{tr('Import local images', '导入本地图片')}</Switch.Content>
                    </Switch>
                  </div>
                )}
                <OptionSelect
                  isDisabled={isPending}
                  label={tr('Scan profile', '扫描配置')}
                  onChange={setScanProfile}
                  options={scanProfileOptions}
                  value={scanProfile}
                />
                <Switch isDisabled={isPending} isSelected={enabled} onChange={setEnabled}>
                  <Switch.Content>
                    <Switch.Control><Switch.Thumb /></Switch.Control>
                    {tr('Enabled', '已启用')}
                  </Switch.Content>
                </Switch>
              </form>
            </Modal.Body>
            <Modal.Footer>
              <Button isDisabled={isPending} onPress={close} variant="tertiary">{tr('Cancel', '取消')}</Button>
              <Button
                form="create-library-form"
                isDisabled={name.trim().length === 0 || path.trim().length === 0}
                isPending={isPending}
                type="submit"
              >
                {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <Plus aria-hidden="true" className="size-4" />}
                <span className="inline-flex min-h-5 items-center">{tr('Create library', '创建媒体库')}</span>
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
      </Modal>
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
  const tr = useTranslate();
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
            <ListBox.Item id={option.value} key={option.value} textValue={tr(option.label)}>
              {tr(option.label)}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}
