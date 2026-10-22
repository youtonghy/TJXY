import { Alert, Button, Card, ProgressBar } from '@heroui/react';
import { Download, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { checkClientUpdate, isArch, isClientVersion, isPlatform, type ClientUpdateCheck } from '../../clientReleases/clientReleaseApi';
import { formatBytes } from '../../clientReleases/releaseFormat';
import { getApiBaseUrl, resolveApiUrl } from '../api/apiBase';
import { getClientToken } from '../auth/clientSession';
import { useTranslate } from '../../settings/i18n';
import { getNativeUpdater, type NativeAppInfo, type NativeUpdater } from './nativeUpdater';

type InstallState = { kind: 'idle' } | { kind: 'installing'; received: number; total: number } | { kind: 'opened' } | { kind: 'restarting' } | { kind: 'failed'; message: string };

/** Lets native shells check the server for a newer client build and install it. */
export function ClientUpdateCard() {
  const updater = getNativeUpdater();
  return updater === null ? null : <UpdateCard updater={updater} />;
}

function UpdateCard({ updater }: { updater: NativeUpdater }) {
  const tr = useTranslate();
  const [app, setApp] = useState<NativeAppInfo | null>(null);
  const [check, setCheck] = useState<ClientUpdateCheck | null>(null);
  const [checking, setChecking] = useState(false);
  const [checkFailed, setCheckFailed] = useState(false);
  const [install, setInstall] = useState<InstallState>({ kind: 'idle' });
  const supported = app !== null && isPlatform(app.platform) && isArch(app.arch) && isClientVersion(app.version);

  const runCheck = useCallback(async (info: NativeAppInfo) => {
    if (!isPlatform(info.platform) || !isArch(info.arch) || !isClientVersion(info.version)) return;
    setChecking(true); setCheckFailed(false);
    try { setCheck(await checkClientUpdate(info.platform, info.arch, info.version)); }
    catch { setCheckFailed(true); }
    finally { setChecking(false); }
  }, []);

  useEffect(() => {
    let active = true;
    updater.getAppInfo()
      .then((info) => { if (!active) return; setApp(info); void runCheck(info); })
      .catch(() => { if (active) setCheckFailed(true); });
    return () => { active = false; };
  }, [runCheck, updater]);

  const latest = check?.updateAvailable === true ? check.latest : null;

  const startInstall = async () => {
    const token = getClientToken();
    if (latest === null || token === null) return;
    setInstall({ kind: 'installing', received: 0, total: latest.size });
    const unsubscribe = updater.onProgress?.((received, total) => { setInstall({ kind: 'installing', received, total }); });
    try {
      const result = await updater.install({
        url: resolveApiUrl(latest.downloadPath, getApiBaseUrl()),
        headers: { Authorization: `MediaBrowser Token="${token}"` },
        fileName: latest.fileName,
        sha256: latest.sha256,
        size: latest.size,
      });
      setInstall({ kind: result.outcome === 'restarting' ? 'restarting' : 'opened' });
    } catch (reason) {
      setInstall({ kind: 'failed', message: reason instanceof Error && reason.message.length > 0 ? reason.message : tr('The update could not be installed.', '无法安装更新。') });
    } finally { unsubscribe?.(); }
  };

  return (
    <Card>
      <Card.Header>
        <Card.Title>{tr('App updates', '应用更新')}</Card.Title>
        <Card.Description>{app === null ? tr('Reading the app version…', '正在读取应用版本…') : `${tr('Current version', '当前版本')} ${app.version}`}</Card.Description>
      </Card.Header>
      <Card.Content className="space-y-3">
        {app !== null && !supported && <p className="text-sm text-muted">{tr('In-app updates are not available on this device.', '此设备不支持应用内更新。')}</p>}
        {checkFailed && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('Could not check for updates', '无法检查更新')}</Alert.Title></Alert.Content></Alert>}
        {supported && check !== null && latest === null && !checkFailed && <p className="text-sm text-muted">{tr('You are on the latest version.', '已是最新版本。')}</p>}
        {latest !== null && (
          <div className="space-y-2">
            <p className="font-medium">{tr('New version available', '发现新版本')} {latest.version} <span className="text-sm font-normal text-muted">({formatBytes(latest.size)})</span></p>
            {latest.notes.trim().length > 0 && <p className="whitespace-pre-wrap text-sm text-muted">{latest.notes}</p>}
          </div>
        )}
        {install.kind === 'installing' && <ProgressBar aria-label={tr('Download progress', '下载进度')} maxValue={Math.max(install.total, 1)} value={install.received}><ProgressBar.Track><ProgressBar.Fill /></ProgressBar.Track></ProgressBar>}
        {install.kind === 'opened' && <p className="text-sm">{tr('The installer has been opened. Follow its steps to finish updating.', '已打开安装程序，请按提示完成更新。')}</p>}
        {install.kind === 'restarting' && <p className="text-sm">{tr('Installing the update. The app will close and restart.', '正在安装更新，应用将关闭并重新启动。')}</p>}
        {install.kind === 'failed' && <Alert role="alert" status="danger"><Alert.Content><Alert.Title>{tr('Update failed', '更新失败')}</Alert.Title><Alert.Description>{install.message}</Alert.Description></Alert.Content></Alert>}
        <div className="flex flex-wrap gap-2">
          {supported && <Button isDisabled={install.kind === 'installing'} isPending={checking} onPress={() => { void runCheck(app); }} variant="secondary"><RefreshCw aria-hidden="true" className="size-4" />{tr('Check for updates', '检查更新')}</Button>}
          {latest !== null && <Button isDisabled={install.kind === 'restarting'} isPending={install.kind === 'installing'} onPress={() => { void startInstall(); }}><Download aria-hidden="true" className="size-4" />{tr('Download and install', '下载并安装')}</Button>}
        </div>
      </Card.Content>
    </Card>
  );
}
