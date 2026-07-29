import {
  Alert,
  Button,
  Input,
  Label,
  ListBox,
  Select,
  Skeleton,
  TextField,
} from '@heroui/react';
import {
  CheckCircle2,
  ExternalLink,
  FolderOpen,
  RefreshCw,
  RotateCcw,
  TriangleAlert,
} from 'lucide-react';
import { useLogoutIfAccessDenied, useNotify } from 'ra-core';
import { useCallback, useEffect, useRef, useState } from 'react';

import { FolderBrowser, type FolderChoice } from './FolderBrowser';
import { StorageWorkflow, type StoragePhase } from './StorageWorkflow';
import { closeOAuthPopup, navigateOAuthPopup, reserveOAuthPopup } from './oauthPopup';
import type { GoogleOAuthStart, LibraryOption, StorageBindingResult } from './googleDriveApi';
import {
  bindOneDrive,
  listLibraries,
  listOneDriveDirectories,
  startOneDriveOAuth,
} from './googleDriveApi';
import { uniqueChoices } from './directoryChoices';

type BusyOperation = 'start' | 'verify' | 'browse' | 'more' | 'bind' | null;

export function OneDrivePage() {
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [libraries, setLibraries] = useState<LibraryOption[]>([]);
  const [librariesPending, setLibrariesPending] = useState(true);
  const [librariesError, setLibrariesError] = useState(false);
  const [targetLibraryId, setTargetLibraryId] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [oauth, setOAuth] = useState<GoogleOAuthStart | null>(null);
  const [authorized, setAuthorized] = useState(false);
  const [path, setPath] = useState<FolderChoice[]>([]);
  const [directories, setDirectories] = useState<FolderChoice[]>([]);
  const [nextDirectoryPage, setNextDirectoryPage] = useState<string | null>(null);
  const [directoryError, setDirectoryError] = useState(false);
  const [authorizationError, setAuthorizationError] = useState<string | null>(null);
  const [popupBlocked, setPopupBlocked] = useState(false);
  const [reviewing, setReviewing] = useState(false);
  const [bindingError, setBindingError] = useState(false);
  const [binding, setBinding] = useState<StorageBindingResult | null>(null);
  const [busy, setBusy] = useState<BusyOperation>(null);
  const busyRef = useRef<BusyOperation>(null);
  const [authRedirecting, setAuthRedirecting] = useState(false);
  const sequence = useRef(0);
  const effectSequence = useRef(0);
  const mounted = useRef(false);
  const libraryAbort = useRef<AbortController | null>(null);
  const operationAbort = useRef<AbortController | null>(null);
  const oauthPopup = useRef<Window | null>(null);
  const directoryRetry = useRef<(() => void) | null>(null);

  const isCurrent = useCallback((request: number) => mounted.current && request === sequence.current, []);
  const invalidate = useCallback(() => { sequence.current += 1; }, []);

  const loadLibraries = useCallback(async () => {
    libraryAbort.current?.abort();
    const controller = new AbortController();
    libraryAbort.current = controller;
    setLibrariesPending(true);
    setLibrariesError(false);
    try {
      const records = await listLibraries(controller.signal);
      if (libraryAbort.current !== controller) return;
      setLibraries(records);
      const initial = records.find((library) => library.enabled) ?? records[0];
      if (initial !== undefined) {
        setTargetLibraryId(initial.id);
        setDisplayName(initial.name);
      }
    } catch (error: unknown) {
      if (libraryAbort.current !== controller) return;
      const handled = await logoutIfAccessDenied(error);
      if (libraryAbort.current !== controller) return;
      if (handled) setAuthRedirecting(true);
      else setLibrariesError(true);
    } finally {
      if (!controller.signal.aborted && mounted.current) setLibrariesPending(false);
    }
  }, [logoutIfAccessDenied]);

  useEffect(() => {
    mounted.current = true;
    const effect = ++effectSequence.current;
    void Promise.resolve().then(() => {
      if (mounted.current && effect === effectSequence.current) void loadLibraries();
    });
    return () => {
      mounted.current = false;
      effectSequence.current += 1;
      invalidate();
      libraryAbort.current?.abort();
      libraryAbort.current = null;
      operationAbort.current?.abort();
      operationAbort.current = null;
      closeOAuthPopup(oauthPopup.current);
      oauthPopup.current = null;
    };
  }, [invalidate, loadLibraries]);

  const startAuthorization = async () => {
    if (busyRef.current !== null || targetLibraryId.length === 0) return;
    const popup = reserveOAuthPopup('tjxy-onedrive-oauth');
    oauthPopup.current = popup;
    const request = ++sequence.current;
    const controller = new AbortController();
    operationAbort.current = controller;
    busyRef.current = 'start';
    setBusy('start');
    setAuthorizationError(null);
    setPopupBlocked(false);
    setBinding(null);
    try {
      const result = await startOneDriveOAuth(targetLibraryId, controller.signal);
      if (!isCurrent(request)) return;
      setOAuth(result);
      if (popup !== null && navigateOAuthPopup(popup, result.authorizationUrl)) {
        setPopupBlocked(false);
      } else {
        oauthPopup.current = null;
        setPopupBlocked(true);
      }
    } catch (error: unknown) {
      closeOAuthPopup(popup);
      if (oauthPopup.current === popup) oauthPopup.current = null;
      if (!isCurrent(request)) return;
      setPopupBlocked(false);
      const handled = await logoutIfAccessDenied(error);
      if (!isCurrent(request)) return;
      if (handled) {
        setAuthRedirecting(true);
      } else setAuthorizationError('Microsoft authorization could not start.');
    } finally {
      if (isCurrent(request)) {
        if (operationAbort.current === controller) operationAbort.current = null;
        busyRef.current = null;
        setBusy(null);
      }
    }
  };

  const retryPopup = () => {
    if (oauth === null || busyRef.current !== null) return;
    const popup = reserveOAuthPopup('tjxy-onedrive-oauth');
    oauthPopup.current = popup;
    if (popup !== null && navigateOAuthPopup(popup, oauth.authorizationUrl)) {
      setPopupBlocked(false);
    } else {
      oauthPopup.current = null;
      setPopupBlocked(true);
    }
  };

  const verifyAuthorization = async () => {
    if (oauth === null || busyRef.current !== null) return;
    const request = ++sequence.current;
    const controller = new AbortController();
    operationAbort.current = controller;
    busyRef.current = 'verify';
    setBusy('verify');
    setAuthorizationError(null);
    try {
      const page = await listOneDriveDirectories(oauth.state, {}, controller.signal);
      if (!isCurrent(request)) return;
      setAuthorized(true);
      setPath([{ id: 'root', name: 'OneDrive' }]);
      setDirectories(page.items);
      setNextDirectoryPage(page.nextPageToken);
      setReviewing(false);
      closeOAuthPopup(oauthPopup.current);
      oauthPopup.current = null;
    } catch (error: unknown) {
      if (!isCurrent(request)) return;
      const handled = await logoutIfAccessDenied(error);
      if (!isCurrent(request)) return;
      if (handled) {
        setAuthRedirecting(true);
      } else if (isConflict(error)) {
        setAuthorizationError('Microsoft authorization has not completed yet.');
      } else setAuthorizationError('Microsoft authorization could not be verified.');
    } finally {
      if (isCurrent(request)) {
        if (operationAbort.current === controller) operationAbort.current = null;
        busyRef.current = null;
        setBusy(null);
      }
    }
  };

  const loadDirectoryPage = async (
    request: { parentId?: string; pageToken?: string },
    nextPath: FolderChoice[],
    mode: 'replace' | 'append',
  ) => {
    if (oauth === null || busyRef.current !== null) return;
    const requestId = ++sequence.current;
    const controller = new AbortController();
    operationAbort.current = controller;
    busyRef.current = mode === 'append' ? 'more' : 'browse';
    directoryRetry.current = () => { void loadDirectoryPage(request, nextPath, mode); };
    setBusy(mode === 'append' ? 'more' : 'browse');
    setDirectoryError(false);
    try {
      const page = await listOneDriveDirectories(oauth.state, request, controller.signal);
      if (!isCurrent(requestId)) return;
      if (mode === 'append') {
        setDirectories((current) => uniqueChoices([...current, ...page.items]));
      } else {
        setPath(nextPath);
        setDirectories(page.items);
        setReviewing(false);
      }
      setNextDirectoryPage(page.nextPageToken);
    } catch (error: unknown) {
      if (!isCurrent(requestId)) return;
      const handled = await logoutIfAccessDenied(error);
      if (!isCurrent(requestId)) return;
      if (handled) {
        setAuthRedirecting(true);
      } else setDirectoryError(true);
    } finally {
      if (isCurrent(requestId)) {
        if (operationAbort.current === controller) operationAbort.current = null;
        busyRef.current = null;
        setBusy(null);
      }
    }
  };

  const navigateFolder = (index: number) => {
    const folder = path[index];
    if (folder === undefined) return;
    void loadDirectoryPage(folder.id === 'root' ? {} : { parentId: folder.id }, path.slice(0, index + 1), 'replace');
  };

  const openFolder = (folder: FolderChoice) => {
    void loadDirectoryPage({ parentId: folder.id }, [...path, folder], 'replace');
  };

  const loadMoreDirectories = () => {
    const currentFolder = path.at(-1);
    if (currentFolder === undefined || nextDirectoryPage === null) return;
    void loadDirectoryPage(
      {
        ...(currentFolder.id === 'root' ? {} : { parentId: currentFolder.id }),
        pageToken: nextDirectoryPage,
      },
      path,
      'append',
    );
  };

  const bindCurrentFolder = async () => {
    const currentFolder = path.at(-1);
    if (
      oauth === null
      || currentFolder === undefined
      || busyRef.current !== null
      || displayName.trim().length === 0
      || bindingError
    ) return;
    const request = ++sequence.current;
    const controller = new AbortController();
    operationAbort.current = controller;
    busyRef.current = 'bind';
    setBusy('bind');
    setBindingError(false);
    try {
      const result = await bindOneDrive(oauth.state, {
        displayName: displayName.trim(),
        rootObjectId: currentFolder.id,
      }, controller.signal);
      if (!isCurrent(request)) return;
      setBinding(result);
      notify('OneDrive was added.', { type: 'success' });
    } catch (error: unknown) {
      if (!isCurrent(request)) return;
      const handled = await logoutIfAccessDenied(error);
      if (!isCurrent(request)) return;
      if (handled) {
        setAuthRedirecting(true);
      } else setBindingError(true);
    } finally {
      if (isCurrent(request)) {
        if (operationAbort.current === controller) operationAbort.current = null;
        busyRef.current = null;
        setBusy(null);
      }
    }
  };

  const restart = () => {
    invalidate();
    operationAbort.current?.abort();
    operationAbort.current = null;
    closeOAuthPopup(oauthPopup.current);
    oauthPopup.current = null;
    setOAuth(null);
    setAuthorized(false);
    setAuthorizationError(null);
    setPopupBlocked(false);
    setDirectoryError(false);
    setBindingError(false);
    setBinding(null);
    setPath([]);
    setDirectories([]);
    setNextDirectoryPage(null);
    setReviewing(false);
    busyRef.current = null;
    setBusy(null);
    directoryRetry.current = null;
    const library = libraries.find((item) => item.id === targetLibraryId);
    if (library !== undefined) setDisplayName(library.name);
  };

  if (authRedirecting) return null;

  const phase: StoragePhase = binding !== null
    ? 'complete'
    : !authorized
      ? 'authorize'
      : reviewing
        ? 'review'
        : 'choose-folder';
  const currentFolder = path.at(-1);
  const selectedLibrary = libraries.find((library) => library.id === targetLibraryId);
  const isBusy = busy !== null;

  return (
    <StorageWorkflow
      canRestart={oauth !== null || authorized || binding !== null}
      isBusy={isBusy}
      onRestart={restart}
      phase={phase}
      providerName="OneDrive"
      title="OneDrive"
    >
      {phase === 'authorize' && (
        <AuthorizeStep
          authorizationError={authorizationError}
          libraries={libraries}
          librariesError={librariesError}
          librariesPending={librariesPending}
          onRetryLibraries={() => { void loadLibraries(); }}
          onRetryPopup={retryPopup}
          onStart={() => { void startAuthorization(); }}
          onVerify={() => { void verifyAuthorization(); }}
          popupBlocked={popupBlocked}
          selectedLibrary={selectedLibrary}
          setTargetLibraryId={(id) => {
            setTargetLibraryId(id);
            const library = libraries.find((item) => item.id === id);
            if (library !== undefined) setDisplayName(library.name);
          }}
          targetLibraryId={targetLibraryId}
          hasOAuth={oauth !== null}
          isBusy={isBusy}
        />
      )}
      {phase === 'choose-folder' && (
        <section aria-labelledby="onedrive-folder-heading" className="space-y-5">
          <div>
            <h2 className="text-lg font-semibold text-foreground" id="onedrive-folder-heading">Choose a folder</h2>
            <p className="mt-1 text-sm leading-6 text-muted">Browse folders on the authorized account. Only the selected folder is bound.</p>
          </div>
          <FolderBrowser
            ariaLabel="OneDrive folders"
            directories={directories}
            error={directoryError ? new Error('provider') : null}
            hasMore={nextDirectoryPage !== null}
            isDisabled={isBusy}
            isLoading={busy === 'browse'}
            isLoadingMore={busy === 'more'}
            onLoadMore={loadMoreDirectories}
            onNavigate={navigateFolder}
            onOpen={openFolder}
            onRetry={() => { directoryRetry.current?.(); }}
            path={path}
          />
          <div className="flex justify-end">
            <Button isDisabled={isBusy || currentFolder === undefined} onPress={() => { setReviewing(true); }}>
              <FolderOpen aria-hidden="true" className="size-4" />Use this folder
            </Button>
          </div>
        </section>
      )}
      {phase === 'review' && currentFolder !== undefined && (
        <ReviewStep
          bindingError={bindingError}
          currentFolder={currentFolder}
          displayName={displayName}
          isBusy={isBusy}
          onBack={() => { setReviewing(false); }}
          onBind={() => { void bindCurrentFolder(); }}
          onDisplayNameChange={setDisplayName}
          selectedLibrary={selectedLibrary}
        />
      )}
      {phase === 'complete' && binding !== null && <CompleteStep binding={binding} />}
    </StorageWorkflow>
  );
}

function AuthorizeStep({
  authorizationError,
  libraries,
  librariesError,
  librariesPending,
  onRetryLibraries,
  onRetryPopup,
  onStart,
  onVerify,
  popupBlocked,
  selectedLibrary,
  setTargetLibraryId,
  targetLibraryId,
  hasOAuth,
  isBusy,
}: {
  authorizationError: string | null;
  libraries: LibraryOption[];
  librariesError: boolean;
  librariesPending: boolean;
  onRetryLibraries: () => void;
  onRetryPopup: () => void;
  onStart: () => void;
  onVerify: () => void;
  popupBlocked: boolean;
  selectedLibrary: LibraryOption | undefined;
  setTargetLibraryId: (id: string) => void;
  targetLibraryId: string;
  hasOAuth: boolean;
  isBusy: boolean;
}) {
  return (
    <section aria-labelledby="onedrive-authorize-heading" className="max-w-2xl space-y-5">
      <div>
        <h2 className="text-lg font-semibold text-foreground" id="onedrive-authorize-heading">Authorize OneDrive</h2>
        <p className="mt-1 text-sm leading-6 text-muted">Choose an enabled library, then complete authorization in the provider window.</p>
      </div>
      {librariesPending ? (
        <div aria-label="Loading target libraries" className="space-y-2" role="status"><Skeleton className="h-12 w-full" /></div>
      ) : librariesError ? (
        <Alert role="alert" status="danger"><Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>Target libraries could not be loaded</Alert.Title><Alert.Description>Retry before starting authorization.</Alert.Description></Alert.Content><Button onPress={onRetryLibraries} size="sm" variant="tertiary"><RefreshCw aria-hidden="true" className="size-4" />Retry</Button></Alert>
      ) : libraries.length === 0 ? (
        <p className="border-y border-border py-8 text-sm text-muted">Create an enabled library before connecting OneDrive.</p>
      ) : (
        <>
          <Select fullWidth isDisabled={isBusy || hasOAuth} onChange={(key) => { if (typeof key === 'string') setTargetLibraryId(key); }} value={targetLibraryId}>
            <Label>Target library</Label>
            <Select.Trigger><Select.Value /><Select.Indicator /></Select.Trigger>
            <Select.Popover><ListBox>{libraries.map((library) => <ListBox.Item id={library.id} isDisabled={!library.enabled} key={library.id} textValue={library.enabled ? library.name : `${library.name} disabled`}>{library.name}{!library.enabled && <span className="ml-auto text-xs text-muted">Disabled</span>}<ListBox.ItemIndicator /></ListBox.Item>)}</ListBox></Select.Popover>
          </Select>
          {selectedLibrary !== undefined && !selectedLibrary.enabled && <p className="text-sm text-danger">Select an enabled library to continue.</p>}
          {authorizationError !== null && <Alert role="alert" status="warning"><Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>Authorization needs attention</Alert.Title><Alert.Description>{authorizationError}</Alert.Description></Alert.Content></Alert>}
          {popupBlocked && <Alert role="alert" status="warning"><Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>The authorization window was blocked</Alert.Title><Alert.Description>Allow popups for this admin page, then retry. Your authorization state is still available.</Alert.Description></Alert.Content><Button isDisabled={isBusy} onPress={onRetryPopup} size="sm" variant="tertiary"><RotateCcw aria-hidden="true" className="size-4" />Retry</Button></Alert>}
          <div className="flex flex-wrap gap-2">
            {!hasOAuth && <Button isDisabled={targetLibraryId.length === 0 || selectedLibrary?.enabled !== true} isPending={isBusy} onPress={onStart}><ExternalLink aria-hidden="true" className="size-4" />Authorize OneDrive</Button>}
            {hasOAuth && <Button isPending={isBusy} onPress={onVerify} variant="secondary"><RefreshCw aria-hidden="true" className="size-4" />Check authorization</Button>}
          </div>
        </>
      )}
    </section>
  );
}

function ReviewStep({
  bindingError,
  currentFolder,
  displayName,
  isBusy,
  onBack,
  onBind,
  onDisplayNameChange,
  selectedLibrary,
}: {
  bindingError: boolean;
  currentFolder: FolderChoice;
  displayName: string;
  isBusy: boolean;
  onBack: () => void;
  onBind: () => void;
  onDisplayNameChange: (value: string) => void;
  selectedLibrary: LibraryOption | undefined;
}) {
  return (
    <section aria-labelledby="onedrive-review-heading" className="max-w-2xl space-y-5">
      <div><h2 className="text-lg font-semibold text-foreground" id="onedrive-review-heading">Review binding</h2><p className="mt-1 text-sm leading-6 text-muted">Confirm the target library and provider folder before creating the binding.</p></div>
      <dl className="grid gap-4 border-y border-border py-4 text-sm sm:grid-cols-2"><ReviewField label="Target library">{selectedLibrary?.name ?? 'Unknown library'}</ReviewField><ReviewField label="Folder">{currentFolder.name}</ReviewField></dl>
      <TextField fullWidth isRequired name="displayName"><Label>Display name</Label><Input disabled={isBusy || bindingError} maxLength={2048} onChange={(event) => { onDisplayNameChange(event.currentTarget.value); }} value={displayName} /></TextField>
      {bindingError && <Alert role="alert" status="danger"><Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>The binding result could not be confirmed</Alert.Title><Alert.Description>This authorization cannot be reused. Restart authorization before trying again.</Alert.Description></Alert.Content></Alert>}
      <div className="flex flex-wrap justify-between gap-2"><Button isDisabled={isBusy || bindingError} onPress={onBack} variant="tertiary"><RotateCcw aria-hidden="true" className="size-4" />Back to folder</Button><Button isDisabled={bindingError || displayName.trim().length === 0} isPending={isBusy} onPress={onBind}><CheckCircle2 aria-hidden="true" className="size-4" />Add OneDrive</Button></div>
    </section>
  );
}

function ReviewField({ label, children }: { label: string; children: string }) {
  return <div><dt className="font-medium text-muted">{label}</dt><dd className="mt-1 break-words text-foreground">{children}</dd></div>;
}

function CompleteStep({ binding }: { binding: StorageBindingResult }) {
  return <section aria-labelledby="onedrive-complete-heading" className="max-w-2xl space-y-5"><Alert status="success"><Alert.Indicator><CheckCircle2 aria-hidden="true" className="size-5" /></Alert.Indicator><Alert.Content><Alert.Title id="onedrive-complete-heading">OneDrive is connected</Alert.Title><Alert.Description>{binding.restartRequired ? 'Restart the server before the new storage root becomes active.' : 'The storage root is active and ready for its initial sync.'}</Alert.Description></Alert.Content></Alert><dl className="grid gap-4 border-y border-border py-4 text-sm sm:grid-cols-2"><ReviewField label="Initial sync job">{binding.initialSyncJobId}</ReviewField><ReviewField label="Storage root">{binding.rootId}</ReviewField></dl></section>;
}

function isConflict(error: unknown): boolean {
  return typeof error === 'object' && error !== null
    && (('category' in error && error.category === 'conflict') || ('status' in error && error.status === 409));
}
