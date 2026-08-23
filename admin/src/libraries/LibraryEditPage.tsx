import {
  Button,
  Input,
  Label,
  Skeleton,
  TextField,
  Tooltip,
} from '@heroui/react';
import {
  FolderCog,
  FolderPlus,
  LoaderCircle,
  MapPin,
  Pencil,
  RefreshCw,
  Trash2,
} from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useCallback, useRef, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';

import { AsyncContent } from '../ui/AsyncContent';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { PageHeader } from '../ui/PageHeader';
import { useAuthoritativeLoad } from '../ui/useAuthoritativeLoad';
import { attachFilesystemFolder } from './filesystemApi';
import type {
  EffectiveLibraryPolicy,
  LibraryOption,
  MetadataSourceMode,
  ScanProfile,
} from './libraryApi';
import {
  deleteLibrary,
  listLibraries,
  renameLibrary,
  updateLibraryPolicy,
} from './libraryApi';
import { LibraryPolicyForm } from './LibraryPolicyForm';
import { collectionLabel } from './libraryUi';

type LibraryDetailLoadResult = { record: LibraryOption | null } | { error: unknown };
type LoadMode = 'all' | 'policy';

export function LibraryEditPage() {
  const { id = '' } = useParams();
  return <LibraryEditPageContent id={id} key={id} />;
}

function LibraryEditPageContent({ id }: { id: string }) {
  const navigate = useNavigate();
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [library, setLibrary] = useState<LibraryOption | null>(null);
  const [hasLoaded, setHasLoaded] = useState(false);
  const [loadError, setLoadError] = useState<NonNullable<unknown> | null>(null);
  const [authRedirecting, setAuthRedirecting] = useState(false);
  const [name, setName] = useState('');
  const [enabled, setEnabled] = useState(false);
  const [scanProfile, setScanProfile] = useState<ScanProfile>('Lazy');
  const [metadataSourceMode, setMetadataSourceMode] = useState<MetadataSourceMode>('automatic_scrape');
  const [importMetadata, setImportMetadata] = useState(true);
  const [importImages, setImportImages] = useState(true);
  const [advanced, setAdvanced] = useState(false);
  const [policy, setPolicy] = useState<EffectiveLibraryPolicy>(defaultPolicy);
  const [policyConflict, setPolicyConflict] = useState(false);
  const [renamePending, setRenamePending] = useState(false);
  const [policyPending, setPolicyPending] = useState(false);
  const [deletePending, setDeletePending] = useState(false);
  const [attachPending, setAttachPending] = useState(false);
  const [attachPath, setAttachPath] = useState('');
  const renameRef = useRef(false);
  const policyRef = useRef(false);
  const deleteRef = useRef(false);
  const loadModeRef = useRef<LoadMode>('all');

  const fetchLibrary = useCallback((signal: AbortSignal) => (
    fetchLibraryById(id, signal)
  ), [id]);

  const prepareLoadResult = useCallback(async (result: LibraryDetailLoadResult) => {
    if ('record' in result) {
      const loadMode = loadModeRef.current;
      return () => {
        setLibrary((current) => {
          if (loadMode !== 'policy' || current === null || result.record === null) {
            return result.record;
          }
          return { ...result.record, name: current.name };
        });
        setHasLoaded(true);
        setLoadError(null);
        setAuthRedirecting(false);
        setPolicyConflict(false);
        if (result.record !== null) {
          if (loadMode === 'all') setName(result.record.name);
          setEnabled(result.record.enabled);
          setScanProfile(result.record.scanProfile);
          setMetadataSourceMode(result.record.metadataSourceMode);
          const mode = result.record.localMetadataAccessMode;
          setImportMetadata(mode === 'import' || mode === 'import_metadata_only');
          setImportImages(mode === 'import' || mode === 'import_images_only');
          setAdvanced(false);
          setPolicy(policyFromLibrary(result.record));
        }
        loadModeRef.current = 'all';
      };
    }
    if (await logoutIfAccessDenied(result.error)) {
      return () => { setAuthRedirecting(true); };
    }
    return () => { setLoadError(result.error ?? new Error('Library loading failed.')); };
  }, [logoutIfAccessDenied]);

  const { isMounted, loading, reload } = useAuthoritativeLoad(fetchLibrary, prepareLoadResult);

  const reloadAll = () => {
    loadModeRef.current = 'all';
    void reload();
  };

  const rename = async () => {
    if (library === null || renameRef.current) return;
    const nextName = name.trim();
    if (nextName.length === 0 || nextName === library.name) return;
    renameRef.current = true;
    setRenamePending(true);
    try {
      await renameLibrary(library.name, nextName);
      if (!isMounted()) return;
      setLibrary((current) => current === null ? null : { ...current, name: nextName });
      setName(nextName);
      notify('Library renamed.', { type: 'success' });
    } catch (error: unknown) {
      if (!isMounted()) return;
      if (await logoutIfAccessDenied(error)) return;
      if (isMounted()) notify('The library could not be renamed.', { type: 'error' });
    } finally {
      renameRef.current = false;
      if (isMounted()) setRenamePending(false);
    }
  };

  const savePolicy = async () => {
    if (library === null || policyRef.current || policyConflict) return;
    policyRef.current = true;
    setPolicyPending(true);
    setPolicyConflict(false);
    try {
      await updateLibraryPolicy({
        id: library.id,
        enabled,
        scanProfile,
        profileVersion: library.profileVersion,
        metadataSourceMode,
        localMetadataAccessMode: resolveLocalMetadataAccessMode(importMetadata, importImages),
        ...(advanced ? { effectivePolicy: policy } : {}),
      });
      if (!isMounted()) return;
      notify('Scan policy saved.', { type: 'success' });
      loadModeRef.current = 'policy';
      await reload();
    } catch (error: unknown) {
      if (!isMounted()) return;
      if (await logoutIfAccessDenied(error)) return;
      if (!isMounted()) return;
      if (isConflictError(error)) {
        setPolicyConflict(true);
      } else {
        notify('The scan policy could not be saved.', { type: 'error' });
      }
    } finally {
      policyRef.current = false;
      if (isMounted()) setPolicyPending(false);
    }
  };

  const attachFolder = async () => {
    if (library === null || attachPending) return;
    const path = attachPath.trim();
    if (path.length === 0) return;
    setAttachPending(true);
    try {
      await attachFilesystemFolder(library.id, path);
      if (!isMounted()) return;
      notify('Media folder attached.', { type: 'success' });
      setAttachPath('');
      reloadAll();
    } catch (error: unknown) {
      if (!isMounted()) return;
      if (await logoutIfAccessDenied(error)) return;
      if (isMounted()) notify('The media folder could not be attached.', { type: 'error' });
    } finally {
      if (isMounted()) setAttachPending(false);
    }
  };

  const remove = async () => {
    if (library === null || deleteRef.current || renameRef.current || policyRef.current) return;
    deleteRef.current = true;
    setDeletePending(true);
    try {
      await deleteLibrary(library.name);
      if (!isMounted()) return;
      notify('Library deleted.', { type: 'success' });
      void navigate('/admin/libraries');
    } catch (error: unknown) {
      if (!isMounted()) return;
      if (await logoutIfAccessDenied(error)) return;
      if (!isMounted()) return;
      throw error;
    } finally {
      deleteRef.current = false;
      if (isMounted()) setDeletePending(false);
    }
  };

  if (authRedirecting) return null;

  const anyMutationPending = renamePending || policyPending || deletePending || attachPending;
  const pageTitle = library?.name ?? 'Library settings';
  return (
    <div className="space-y-5">
      <PageHeader
        actions={(
          <Tooltip>
            <Button
              aria-label="Reload library"
              isDisabled={anyMutationPending}
              isIconOnly
              isPending={loading}
              onPress={reloadAll}
              size="sm"
              variant="ghost"
            >
              <RefreshCw aria-hidden="true" className={`size-4${loading ? ' animate-spin' : ''}`} />
            </Button>
            <Tooltip.Content>Reload library</Tooltip.Content>
          </Tooltip>
        )}
        breadcrumbs={[
          { label: 'Libraries', to: '/admin/libraries' },
          { label: pageTitle },
        ]}
        description="Manage identity, versioned scanning policy, storage folders, and deletion."
        title={pageTitle}
      />

      {loading && hasLoaded && (
        <p aria-live="polite" className="text-sm text-muted" role="status">Refreshing library settings...</p>
      )}

      <AsyncContent
        empty={<LibraryNotFound />}
        error={loadError}
        hasData={hasLoaded}
        isEmpty={hasLoaded && library === null}
        isPending={loading}
        loading={<LibraryEditSkeleton />}
        onRetry={reloadAll}
      >
        {library !== null && (
          <div>
            <IdentitySection
              isPending={renamePending}
              library={library}
              name={name}
              onNameChange={setName}
              onRename={() => { void rename(); }}
            />
            <LibraryPolicyForm
              advanced={advanced}
              enabled={enabled}
              hasConflict={policyConflict}
              isPending={policyPending}
              library={library}
              onAdvancedChange={setAdvanced}
              onEnabledChange={setEnabled}
              onMetadataSourceModeChange={setMetadataSourceMode}
              onImportMetadataChange={setImportMetadata}
              onImportImagesChange={setImportImages}
              onPolicyChange={setPolicy}
              onProfileChange={setScanProfile}
              onReloadLatest={reloadAll}
              onSave={() => { void savePolicy(); }}
              policy={policy}
              scanProfile={scanProfile}
              metadataSourceMode={metadataSourceMode}
              importMetadata={importMetadata}
              importImages={importImages}
            />
            <StorageFoldersSection
              attachPath={attachPath}
              isPending={attachPending}
              library={library}
              onAttach={() => { void attachFolder(); }}
              onPathChange={setAttachPath}
            />
            <DangerZone
              isDisabled={renamePending || policyPending}
              isPending={deletePending}
              library={library}
              onDelete={remove}
            />
          </div>
        )}
      </AsyncContent>
    </div>
  );
}

function resolveLocalMetadataAccessMode(importMetadata: boolean, importImages: boolean) {
  if (importMetadata && importImages) return 'import' as const;
  if (importMetadata) return 'import_metadata_only' as const;
  if (importImages) return 'import_images_only' as const;
  return 'direct' as const;
}

function StorageFoldersSection({
  attachPath,
  isPending,
  library,
  onAttach,
  onPathChange,
}: {
  attachPath: string;
  isPending: boolean;
  library: LibraryOption;
  onAttach: () => void;
  onPathChange: (path: string) => void;
}) {
  return (
    <section aria-labelledby="storage-folders-heading" className="space-y-5 border-t border-border py-7">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-foreground" id="storage-folders-heading">Media folders</h2>
          <p className="mt-1 text-sm text-muted">Attach server folders that contain this library's media.</p>
        </div>
      </div>
      <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
        <TextField fullWidth>
          <Label>Absolute folder path</Label>
          <Input
            disabled={isPending}
            maxLength={4096}
            onChange={(event) => { onPathChange(event.currentTarget.value); }}
            placeholder="/mnt/media"
            value={attachPath}
          />
        </TextField>
        <Button isDisabled={attachPath.trim().length === 0} isPending={isPending} onPress={onAttach} variant="secondary">
          {isPending
            ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" />
            : <FolderPlus aria-hidden="true" className="size-4" />}
          Add folder
        </Button>
      </div>
      {library.locations.length === 0 ? (
        <p className="border-y border-border py-7 text-center text-sm text-muted">No media folders attached.</p>
      ) : (
        <ul aria-label="Attached media folders" className="divide-y divide-border border-y border-border">
          {library.locations.map((location, index) => (
            <li className="flex min-h-12 items-center gap-3 px-2" key={location}>
              <MapPin aria-hidden="true" className="size-4 shrink-0 text-muted" />
              <span className="text-sm font-medium text-foreground">Storage root {index + 1}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function IdentitySection({
  isPending,
  library,
  name,
  onNameChange,
  onRename,
}: {
  isPending: boolean;
  library: LibraryOption;
  name: string;
  onNameChange: (name: string) => void;
  onRename: () => void;
}) {
  return (
    <section aria-labelledby="library-identity-heading" className="space-y-5 pb-7">
      <div>
        <h2 className="text-base font-semibold text-foreground" id="library-identity-heading">Identity</h2>
        <p className="mt-1 text-sm text-muted">Rename the library without changing its storage roots or content type.</p>
      </div>
      <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
        <TextField fullWidth isRequired name="libraryName">
          <Label>Library name</Label>
          <Input
            disabled={isPending}
            maxLength={256}
            onChange={(event) => { onNameChange(event.currentTarget.value); }}
            value={name}
          />
        </TextField>
        <Button
          className="min-w-32"
          isDisabled={name.trim().length === 0 || name.trim() === library.name}
          isPending={isPending}
          onPress={onRename}
          variant="secondary"
        >
          {isPending ? <LoaderCircle aria-hidden="true" className="size-4 animate-spin" /> : <Pencil aria-hidden="true" className="size-4" />}
          <span className="inline-flex min-h-5 items-center">Rename</span>
        </Button>
      </div>
      <dl className="grid gap-4 text-sm sm:grid-cols-3">
        <Definition label="Content type">{collectionLabel(library.collectionType)}</Definition>
        <Definition label="Storage roots">{library.locations.length}</Definition>
        <Definition label="Profile version">{library.profileVersion}</Definition>
      </dl>
    </section>
  );
}

function DangerZone({
  isDisabled,
  isPending,
  library,
  onDelete,
}: {
  isDisabled: boolean;
  isPending: boolean;
  library: LibraryOption;
  onDelete: () => Promise<void>;
}) {
  return (
    <section aria-labelledby="library-danger-heading" className="space-y-5 border-t border-border py-7">
      <div>
        <h2 className="text-base font-semibold text-danger" id="library-danger-heading">Danger zone</h2>
        <p className="mt-1 text-sm text-muted">Permanently remove this library configuration. Catalog cleanup is handled by the server.</p>
      </div>
      <ConfirmDialog
        confirmLabel="Delete library"
        description={<>Permanently delete <strong className="font-semibold text-foreground">{library.name}</strong>?</>}
        isPending={isPending}
        onConfirm={onDelete}
        title={`Delete ${library.name}?`}
        trigger={(
          <Button isDisabled={isDisabled} variant="danger-soft">
            <Trash2 aria-hidden="true" className="size-4" />
            Delete library
          </Button>
        )}
      />
    </section>
  );
}

function Definition({ label, children }: { label: string; children: string | number }) {
  return (
    <div className="border-t border-border pt-3">
      <dt className="font-medium text-muted">{label}</dt>
      <dd className="mt-1 break-words text-foreground">{children}</dd>
    </div>
  );
}

function LibraryEditSkeleton() {
  return (
    <div aria-label="Loading library settings" className="space-y-8" role="status">
      <Skeleton className="h-28 w-full" />
      <Skeleton className="h-64 w-full" />
      <Skeleton className="h-52 w-full" />
    </div>
  );
}

function LibraryNotFound() {
  return (
    <div className="flex min-h-60 flex-col items-center justify-center gap-4 border-y border-border py-10 text-center">
      <FolderCog aria-hidden="true" className="size-7 text-muted" />
      <div>
        <h2 className="text-lg font-semibold text-foreground">Library not found</h2>
        <p className="mt-1 text-sm text-muted">The requested library is no longer available.</p>
      </div>
      <Link className="text-sm font-semibold text-accent hover:underline" to="/admin/libraries">Back to Libraries</Link>
    </div>
  );
}

function policyFromLibrary(library: LibraryOption): EffectiveLibraryPolicy {
  return {
    objectSelectionScope: library.objectSelectionScope,
    metadataPolicy: library.metadataPolicy,
    expansionPolicy: library.expansionPolicy,
    probePolicy: library.probePolicy,
  };
}

function isConflictError(error: unknown): boolean {
  if (typeof error !== 'object' || error === null) return false;
  return ('status' in error && error.status === 409)
    || ('category' in error && error.category === 'conflict');
}

async function fetchLibraryById(id: string, signal: AbortSignal): Promise<LibraryDetailLoadResult> {
  try {
    const records = await listLibraries(signal);
    return { record: records.find((library) => library.id === id) ?? null };
  } catch (error: unknown) {
    return { error };
  }
}

const defaultPolicy: EffectiveLibraryPolicy = {
  objectSelectionScope: 'title_layer',
  metadataPolicy: 'basic',
  expansionPolicy: 'on_browse',
  probePolicy: 'on_playback',
};
