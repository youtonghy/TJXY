import {
  Alert,
  Button,
  Input,
  Label,
  ListBox,
  Select,
  Skeleton,
  TextField,
  ToggleButton,
  ToggleButtonGroup,
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
import { uniqueChoices } from './directoryChoices';
import { useTranslate } from '../settings/i18n';
import { closeOAuthPopup, navigateOAuthPopup, reserveOAuthPopup } from './oauthPopup';
import { StorageWorkflow, type StoragePhase } from './StorageWorkflow';
import type {
  GoogleDriveChoice,
  GoogleDriveScope,
  GoogleOAuthStart,
  LibraryOption,
  StorageBindingResult,
} from './googleDriveApi';
import {
  bindGoogleDrive,
  listGoogleDirectories,
  listLibraries,
  listSharedDrives,
  startGoogleDriveOAuth,
} from './googleDriveApi';

type BusyOperation = 'libraries' | 'start' | 'verify' | 'browse' | 'shared-more' | 'directory-more' | 'bind' | null;

export function GoogleDrivePage() {
  const tr = useTranslate();
  const notify = useNotify();
  const logoutIfAccessDenied = useLogoutIfAccessDenied();
  const [libraries, setLibraries] = useState<LibraryOption[]>([]);
  const [librariesPending, setLibrariesPending] = useState(true);
  const [librariesError, setLibrariesError] = useState(false);
  const [targetLibraryId, setTargetLibraryId] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [oauth, setOAuth] = useState<GoogleOAuthStart | null>(null);
  const [authorized, setAuthorized] = useState(false);
  const [scope, setScope] = useState<GoogleDriveScope>('MyDrive');
  const [sharedDrives, setSharedDrives] = useState<GoogleDriveChoice[]>([]);
  const [nextSharedPage, setNextSharedPage] = useState<string | null>(null);
  const [sharedError, setSharedError] = useState(false);
  const [noSharedDrives, setNoSharedDrives] = useState(false);
  const [sharedDriveId, setSharedDriveId] = useState('');
  const [path, setPath] = useState<FolderChoice[]>([]);
  const [directories, setDirectories] = useState<FolderChoice[]>([]);
  const [nextDirectoryPage, setNextDirectoryPage] = useState<string | null>(null);
  const [directoryError, setDirectoryError] = useState(false);
  const [reviewing, setReviewing] = useState(false);
  const [authorizationError, setAuthorizationError] = useState<string | null>(null);
  const [popupBlocked, setPopupBlocked] = useState(false);
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
      if (handled) {
        setAuthRedirecting(true);
      } else {
        setLibrariesError(true);
      }
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
    const popup = reserveOAuthPopup('tjxy-google-oauth');
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
      const result = await startGoogleDriveOAuth(targetLibraryId, controller.signal);
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
      } else {
        setAuthorizationError('Google authorization could not start.');
      }
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
    const popup = reserveOAuthPopup('tjxy-google-oauth');
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
    setSharedError(false);
    setNoSharedDrives(false);
    setDirectoryError(false);
    try {
      const [drives, folders] = await Promise.all([
        listSharedDrives(oauth.state, undefined, controller.signal),
        listGoogleDirectories(oauth.state, { scope: 'MyDrive' }, controller.signal),
      ]);
      if (!isCurrent(request)) return;
      setAuthorized(true);
      setScope('MyDrive');
      setSharedDrives(drives.items);
      setNextSharedPage(drives.nextPageToken);
      setSharedDriveId(drives.items[0]?.id ?? '');
      setPath([{ id: 'root', name: 'My Drive' }]);
      setDirectories(folders.items);
      setNextDirectoryPage(folders.nextPageToken);
      setReviewing(false);
      closeOAuthPopup(oauthPopup.current);
      oauthPopup.current = null;
    } catch (error: unknown) {
      controller.abort();
      if (!isCurrent(request)) return;
      const handled = await logoutIfAccessDenied(error);
      if (!isCurrent(request)) return;
      if (handled) {
        setAuthRedirecting(true);
      } else if (isConflict(error)) {
        setAuthorizationError('Google authorization has not completed yet.');
      } else {
        setAuthorizationError('Google authorization could not be verified.');
      }
    } finally {
      if (isCurrent(request)) {
        if (operationAbort.current === controller) operationAbort.current = null;
        busyRef.current = null;
        setBusy(null);
      }
    }
  };

  const loadDirectoryPage = async (
    request: { scope: GoogleDriveScope; sharedDriveId?: string; parentId?: string; pageToken?: string },
    nextPath: FolderChoice[],
    mode: 'replace' | 'append',
  ) => {
    if (oauth === null || busyRef.current !== null) return;
    const operation = mode === 'append' ? 'directory-more' : 'browse';
    const requestId = ++sequence.current;
    const controller = new AbortController();
    operationAbort.current = controller;
    busyRef.current = operation;
    directoryRetry.current = () => { void loadDirectoryPage(request, nextPath, mode); };
    setBusy(operation);
    setDirectoryError(false);
    try {
      const page = await listGoogleDirectories(oauth.state, request, controller.signal);
      if (!isCurrent(requestId)) return;
      if (mode === 'append') {
        setDirectories((current) => uniqueChoices([...current, ...page.items]));
      } else {
        setScope(request.scope);
        if (request.scope === 'SharedDrive') setSharedDriveId(request.sharedDriveId ?? '');
        else {
          setSharedError(false);
          setNoSharedDrives(false);
        }
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
      } else {
        setDirectoryError(true);
      }
    } finally {
      if (isCurrent(requestId)) {
        if (operationAbort.current === controller) operationAbort.current = null;
        busyRef.current = null;
        setBusy(null);
      }
    }
  };

  const navigateFolder = (pathIndex: number) => {
    const folder = path[pathIndex];
    if (folder === undefined) return;
    void loadDirectoryPage(
      {
        scope,
        ...(scope === 'SharedDrive' ? { sharedDriveId } : {}),
        ...(folder.id === 'root' ? {} : { parentId: folder.id }),
      },
      path.slice(0, pathIndex + 1),
      'replace',
    );
  };

  const openFolder = (folder: FolderChoice) => {
    void loadDirectoryPage(
      {
        scope,
        ...(scope === 'SharedDrive' ? { sharedDriveId } : {}),
        parentId: folder.id,
      },
      [...path, folder],
      'replace',
    );
  };

  const changeScope = (keys: Set<React.Key>) => {
    const nextScope = String(keys.values().next().value ?? '');
    if (nextScope !== 'MyDrive' && nextScope !== 'SharedDrive') return;
    if (nextScope === 'MyDrive') {
      void loadDirectoryPage({ scope: 'MyDrive' }, [{ id: 'root', name: 'My Drive' }], 'replace');
      return;
    }
    const drive = sharedDrives.find((item) => item.id === sharedDriveId) ?? sharedDrives[0];
    if (drive === undefined) {
      setSharedError(false);
      setNoSharedDrives(true);
      return;
    }
    setNoSharedDrives(false);
    void loadDirectoryPage(
      { scope: 'SharedDrive', sharedDriveId: drive.id, parentId: drive.id },
      [drive],
      'replace',
    );
  };

  const changeSharedDrive = (key: React.Key | null) => {
    if (typeof key !== 'string') return;
    const drive = sharedDrives.find((item) => item.id === key);
    if (drive === undefined) return;
    setNoSharedDrives(false);
    void loadDirectoryPage(
      { scope: 'SharedDrive', sharedDriveId: drive.id, parentId: drive.id },
      [drive],
      'replace',
    );
  };

  const loadMoreSharedDrives = async () => {
    if (oauth === null || nextSharedPage === null || busyRef.current !== null) return;
    const request = ++sequence.current;
    const controller = new AbortController();
    operationAbort.current = controller;
    busyRef.current = 'shared-more';
    setBusy('shared-more');
    setSharedError(false);
    try {
      const page = await listSharedDrives(oauth.state, nextSharedPage, controller.signal);
      if (!isCurrent(request)) return;
      const mergedDrives = uniqueChoices([...sharedDrives, ...page.items]);
      setSharedDrives(mergedDrives);
      setNextSharedPage(page.nextPageToken);
      setNoSharedDrives(mergedDrives.length === 0);
    } catch (error: unknown) {
      if (!isCurrent(request)) return;
      const handled = await logoutIfAccessDenied(error);
      if (!isCurrent(request)) return;
      if (handled) {
        setAuthRedirecting(true);
      } else {
        setSharedError(true);
      }
    } finally {
      if (isCurrent(request)) {
        if (operationAbort.current === controller) operationAbort.current = null;
        busyRef.current = null;
        setBusy(null);
      }
    }
  };

  const loadMoreDirectories = () => {
    const currentFolder = path.at(-1);
    if (currentFolder === undefined || nextDirectoryPage === null) return;
    void loadDirectoryPage(
      {
        scope,
        ...(scope === 'SharedDrive' ? { sharedDriveId } : {}),
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
      const result = await bindGoogleDrive(oauth.state, {
        scope,
        displayName: displayName.trim(),
        rootObjectId: currentFolder.id,
        ...(scope === 'SharedDrive' ? { sharedDriveId } : {}),
      }, controller.signal);
      if (!isCurrent(request)) return;
      setBinding(result);
      notify(tr('Google Drive was added.', 'Google Drive 已添加。'), { type: 'success' });
    } catch (error: unknown) {
      if (!isCurrent(request)) return;
      const handled = await logoutIfAccessDenied(error);
      if (!isCurrent(request)) return;
      if (handled) {
        setAuthRedirecting(true);
      } else {
        setBindingError(true);
      }
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
    setPopupBlocked(false);
    setAuthorizationError(null);
    setSharedError(false);
    setNoSharedDrives(false);
    setDirectoryError(false);
    setBindingError(false);
    setBinding(null);
    setScope('MyDrive');
    setSharedDrives([]);
    setNextSharedPage(null);
    setSharedDriveId('');
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
      providerName="Google Drive"
      title="Google Drive"
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
        <ChooseFolderStep
          directories={directories}
          directoryError={directoryError}
          hasMore={nextDirectoryPage !== null}
          isBusy={isBusy}
          isLoading={busy === 'browse'}
          isLoadingMore={busy === 'directory-more'}
          loadMoreDirectories={loadMoreDirectories}
          navigateFolder={navigateFolder}
          onOpenFolder={openFolder}
          path={path}
          scope={scope}
          sharedDriveId={sharedDriveId}
          sharedDrives={sharedDrives}
          sharedError={sharedError}
          noSharedDrives={noSharedDrives}
          hasMoreSharedDrives={nextSharedPage !== null}
          isLoadingSharedDrives={busy === 'shared-more'}
          loadMoreSharedDrives={() => { void loadMoreSharedDrives(); }}
          onChangeScope={changeScope}
          onChangeSharedDrive={changeSharedDrive}
          onRetryDirectory={() => { directoryRetry.current?.(); }}
          onUseFolder={() => { setReviewing(true); }}
          currentFolder={currentFolder}
        />
      )}

      {phase === 'review' && currentFolder !== undefined && (
        <ReviewStep
          currentFolder={currentFolder}
          displayName={displayName}
          isBusy={isBusy}
          onBack={() => { setReviewing(false); }}
          onBind={() => { void bindCurrentFolder(); }}
          onDisplayNameChange={setDisplayName}
          scope={scope}
          selectedLibrary={selectedLibrary}
          sharedDriveId={sharedDriveId}
          sharedDrives={sharedDrives}
          bindingError={bindingError}
        />
      )}

      {phase === 'complete' && binding !== null && (
        <CompleteStep binding={binding} />
      )}
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
  const tr = useTranslate();
  return (
    <section aria-labelledby="google-authorize-heading" className="max-w-2xl space-y-5">
      <div>
        <h2 className="text-lg font-semibold text-foreground" id="google-authorize-heading">{tr('Authorize Google Drive', '授权 Google Drive')}</h2>
        <p className="mt-1 text-sm leading-6 text-muted">{tr('Choose an enabled library, then complete authorization in the provider window.', '选择已启用的媒体库，然后在服务商窗口中完成授权。')}</p>
      </div>
      {librariesPending ? (
        <div aria-label="Loading target libraries" className="space-y-2" role="status"><Skeleton className="h-12 w-full" /></div>
      ) : librariesError ? (
        <Alert role="alert" status="danger">
          <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
          <Alert.Content><Alert.Title>Target libraries could not be loaded</Alert.Title><Alert.Description>Retry before starting authorization.</Alert.Description></Alert.Content>
          <Button onPress={onRetryLibraries} size="sm" variant="tertiary"><RefreshCw aria-hidden="true" className="size-4" />Retry</Button>
        </Alert>
      ) : libraries.length === 0 ? (
        <p className="border-y border-border py-8 text-sm text-muted">{tr('Create an enabled library before connecting Google Drive.', '连接 Google Drive 前请先创建已启用的媒体库。')}</p>
      ) : (
        <>
          <Select
            fullWidth
            isDisabled={isBusy || hasOAuth}
            onChange={(key) => { if (typeof key === 'string') setTargetLibraryId(key); }}
            value={targetLibraryId}
          >
            <Label>{tr('Target library', '目标媒体库')}</Label>
            <Select.Trigger><Select.Value /><Select.Indicator /></Select.Trigger>
            <Select.Popover>
              <ListBox>
                {libraries.map((library) => (
                  <ListBox.Item id={library.id} isDisabled={!library.enabled} key={library.id} textValue={library.enabled ? library.name : `${library.name} disabled`}>
                    <span className="min-w-0 flex-1 break-words">{library.name}</span>
                    {!library.enabled && <span className="text-xs text-muted">Disabled</span>}
                    <ListBox.ItemIndicator />
                  </ListBox.Item>
                ))}
              </ListBox>
            </Select.Popover>
          </Select>
          {selectedLibrary !== undefined && !selectedLibrary.enabled && (
            <p className="text-sm text-danger">Select an enabled library to continue.</p>
          )}
          {authorizationError !== null && (
            <Alert role="alert" status="warning">
              <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
              <Alert.Content><Alert.Title>Authorization needs attention</Alert.Title><Alert.Description>{authorizationError}</Alert.Description></Alert.Content>
            </Alert>
          )}
          {popupBlocked && (
            <Alert role="alert" status="warning">
              <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
              <Alert.Content><Alert.Title>The authorization window was blocked</Alert.Title><Alert.Description>Allow popups for this admin page, then retry. Your authorization state is still available.</Alert.Description></Alert.Content>
              <Button isDisabled={isBusy} onPress={onRetryPopup} size="sm" variant="tertiary"><RotateCcw aria-hidden="true" className="size-4" />Retry</Button>
            </Alert>
          )}
          <div className="flex flex-wrap gap-2">
            {!hasOAuth && (
              <Button isDisabled={targetLibraryId.length === 0 || selectedLibrary?.enabled !== true} isPending={isBusy} onPress={onStart}>
                <ExternalLink aria-hidden="true" className="size-4" />{tr('Authorize Google Drive', '授权 Google Drive')}
              </Button>
            )}
            {hasOAuth && (
              <Button isPending={isBusy} onPress={onVerify} variant="secondary">
                <RefreshCw aria-hidden="true" className="size-4" />{tr('Check authorization', '检查授权')}
              </Button>
            )}
          </div>
        </>
      )}
    </section>
  );
}

function ChooseFolderStep({
  directories,
  directoryError,
  hasMore,
  isBusy,
  isLoading,
  isLoadingMore,
  loadMoreDirectories,
  navigateFolder,
  onOpenFolder,
  path,
  scope,
  sharedDriveId,
  sharedDrives,
  sharedError,
  noSharedDrives,
  hasMoreSharedDrives,
  isLoadingSharedDrives,
  loadMoreSharedDrives,
  onChangeScope,
  onChangeSharedDrive,
  onRetryDirectory,
  onUseFolder,
  currentFolder,
}: {
  directories: FolderChoice[];
  directoryError: boolean;
  hasMore: boolean;
  isBusy: boolean;
  isLoading: boolean;
  isLoadingMore: boolean;
  loadMoreDirectories: () => void;
  navigateFolder: (index: number) => void;
  onOpenFolder: (folder: FolderChoice) => void;
  path: FolderChoice[];
  scope: GoogleDriveScope;
  sharedDriveId: string;
  sharedDrives: GoogleDriveChoice[];
  sharedError: boolean;
  noSharedDrives: boolean;
  hasMoreSharedDrives: boolean;
  isLoadingSharedDrives: boolean;
  loadMoreSharedDrives: () => void;
  onChangeScope: (keys: Set<React.Key>) => void;
  onChangeSharedDrive: (key: React.Key | null) => void;
  onRetryDirectory: () => void;
  onUseFolder: () => void;
  currentFolder: FolderChoice | undefined;
}) {
  const tr = useTranslate();
  return (
    <section aria-labelledby="google-folder-heading" className="space-y-5">
      <div>
        <h2 className="text-lg font-semibold text-foreground" id="google-folder-heading">{tr('Choose a folder', '选择文件夹')}</h2>
        <p className="mt-1 text-sm leading-6 text-muted">{tr('Browse folders on the authorized account. Only the selected folder is bound.', '浏览已授权账户中的文件夹。仅绑定所选文件夹。')}</p>
      </div>
      <ToggleButtonGroup
        aria-label={tr('Drive scope', '云端硬盘范围')}
        disallowEmptySelection
        isDisabled={isBusy}
        onSelectionChange={onChangeScope}
        selectedKeys={[scope]}
        selectionMode="single"
      >
        <ToggleButton id="MyDrive">{tr('My Drive', '我的云端硬盘')}</ToggleButton>
        <ToggleButton id="SharedDrive">{tr('Shared Drive', '共享云端硬盘')}</ToggleButton>
      </ToggleButtonGroup>
      {scope === 'SharedDrive' && (
        <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
          <Select fullWidth isDisabled={isBusy} onChange={onChangeSharedDrive} value={sharedDriveId}>
            <Label>{tr('Shared Drive', '共享云端硬盘')}</Label>
            <Select.Trigger><Select.Value /><Select.Indicator /></Select.Trigger>
            <Select.Popover><ListBox>{sharedDrives.map((drive) => <ListBox.Item id={drive.id} key={drive.id} textValue={drive.name}>{drive.name}<ListBox.ItemIndicator /></ListBox.Item>)}</ListBox></Select.Popover>
          </Select>
          {hasMoreSharedDrives && nextPageButton(sharedError, loadMoreSharedDrives, isBusy, isLoadingSharedDrives)}
        </div>
      )}
      {noSharedDrives && (
        <Alert status="accent">
          <Alert.Content>
            <Alert.Title>{tr('No Shared Drives are available', '没有可用的共享云端硬盘')}</Alert.Title>
            <Alert.Description>{hasMoreSharedDrives ? 'Load the next provider page, then try Shared Drive again.' : 'This account has no Shared Drives to browse.'}</Alert.Description>
          </Alert.Content>
          {hasMoreSharedDrives && nextPageButton(sharedError, loadMoreSharedDrives, isBusy, isLoadingSharedDrives)}
        </Alert>
      )}
      {sharedError && (scope === 'SharedDrive' || noSharedDrives) && (
        <Alert role="alert" status="danger">
          <Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator>
          <Alert.Content><Alert.Title>Shared Drives could not be loaded</Alert.Title><Alert.Description>The current Drive context is unchanged.</Alert.Description></Alert.Content>
        </Alert>
      )}
      <FolderBrowser
        ariaLabel="Google Drive folders"
        directories={directories}
        error={directoryError ? new Error('provider') : null}
        hasMore={hasMore}
        isDisabled={isBusy}
        isLoading={isLoading}
        isLoadingMore={isLoadingMore}
        onLoadMore={loadMoreDirectories}
        onNavigate={navigateFolder}
        onOpen={onOpenFolder}
        onRetry={onRetryDirectory}
        path={path}
      />
      <div className="flex justify-end">
        <Button isDisabled={isBusy || currentFolder === undefined} onPress={onUseFolder}>
          <FolderOpen aria-hidden="true" className="size-4" />{tr('Use this folder', '使用此文件夹')}
        </Button>
      </div>
    </section>
  );
}

function nextPageButton(error: boolean, onPress: () => void, isBusy: boolean, isLoading: boolean) {
  return (
    <Button isDisabled={isBusy && !isLoading} isPending={isLoading} onPress={onPress} size="sm" variant="secondary">
      <RefreshCw aria-hidden="true" className={`size-4${isLoading ? ' animate-spin' : ''}`} />
      {error ? 'Retry Shared Drives' : 'Load more Shared Drives'}
    </Button>
  );
}

function ReviewStep({
  currentFolder,
  displayName,
  isBusy,
  onBack,
  onBind,
  onDisplayNameChange,
  scope,
  selectedLibrary,
  sharedDriveId,
  sharedDrives,
  bindingError,
}: {
  currentFolder: FolderChoice;
  displayName: string;
  isBusy: boolean;
  onBack: () => void;
  onBind: () => void;
  onDisplayNameChange: (value: string) => void;
  scope: GoogleDriveScope;
  selectedLibrary: LibraryOption | undefined;
  sharedDriveId: string;
  sharedDrives: GoogleDriveChoice[];
  bindingError: boolean;
}) {
  const tr = useTranslate();
  const sharedDrive = sharedDrives.find((drive) => drive.id === sharedDriveId);
  return (
    <section aria-labelledby="google-review-heading" className="max-w-2xl space-y-5">
      <div>
        <h2 className="text-lg font-semibold text-foreground" id="google-review-heading">{tr('Review binding', '检查绑定')}</h2>
        <p className="mt-1 text-sm leading-6 text-muted">{tr('Confirm the target library and provider folder before creating the binding.', '创建绑定前请确认目标媒体库和服务商文件夹。')}</p>
      </div>
      <dl className="grid gap-4 border-y border-border py-4 text-sm sm:grid-cols-3">
        <ReviewField label={tr('Target library', '目标媒体库')}>{selectedLibrary?.name ?? tr('Unknown library', '未知媒体库')}</ReviewField>
        <ReviewField label="Drive scope">{scope === 'MyDrive' ? 'My Drive' : sharedDrive?.name ?? 'Shared Drive'}</ReviewField>
        <ReviewField label={tr('Folder', '文件夹')}>{currentFolder.name}</ReviewField>
      </dl>
      <TextField fullWidth isRequired name="displayName">
        <Label>{tr('Display name', '显示名称')}</Label>
        <Input disabled={isBusy || bindingError} maxLength={2048} onChange={(event) => { onDisplayNameChange(event.currentTarget.value); }} value={displayName} />
      </TextField>
      {bindingError && <Alert role="alert" status="danger"><Alert.Indicator><TriangleAlert aria-hidden="true" className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>The binding result could not be confirmed</Alert.Title><Alert.Description>This authorization cannot be reused. Restart authorization before trying again.</Alert.Description></Alert.Content></Alert>}
      <div className="flex flex-wrap justify-between gap-2">
        <Button isDisabled={isBusy || bindingError} onPress={onBack} variant="tertiary"><RotateCcw aria-hidden="true" className="size-4" />{tr('Back to folder', '返回文件夹')}</Button>
        <Button isDisabled={bindingError || displayName.trim().length === 0} isPending={isBusy} onPress={onBind}><CheckCircle2 aria-hidden="true" className="size-4" />{tr('Add Google Drive', '添加 Google Drive')}</Button>
      </div>
    </section>
  );
}

function ReviewField({ label, children }: { label: string; children: string }) {
  return <div><dt className="font-medium text-muted">{label}</dt><dd className="mt-1 break-words text-foreground">{children}</dd></div>;
}

function CompleteStep({ binding }: { binding: StorageBindingResult }) {
  const tr = useTranslate();
  return (
    <section aria-labelledby="google-complete-heading" className="max-w-2xl space-y-5">
      <Alert status="success">
        <Alert.Indicator><CheckCircle2 aria-hidden="true" className="size-5" /></Alert.Indicator>
        <Alert.Content>
          <Alert.Title id="google-complete-heading">{tr('Google Drive is connected', 'Google Drive 已连接')}</Alert.Title>
          <Alert.Description>{binding.restartRequired ? tr('Restart the server before the new storage root becomes active.', '请重启服务器后启用新的存储根目录。') : tr('The storage root is active and ready for its initial sync.', '存储根目录已启用，可以开始初始同步。')}</Alert.Description>
        </Alert.Content>
      </Alert>
      <dl className="grid gap-4 border-y border-border py-4 text-sm sm:grid-cols-2">
        <ReviewField label="Initial sync job">{binding.initialSyncJobId}</ReviewField>
        <ReviewField label="Storage root">{binding.rootId}</ReviewField>
      </dl>
    </section>
  );
}

function isConflict(error: unknown): boolean {
  return typeof error === 'object' && error !== null
    && (('category' in error && error.category === 'conflict') || ('status' in error && error.status === 409));
}
