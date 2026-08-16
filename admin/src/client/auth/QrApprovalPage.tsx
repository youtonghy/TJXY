/* eslint-disable @typescript-eslint/no-confusing-void-expression */
import { Alert, Button, Modal, Spinner } from '@heroui/react';
import { CheckCircle2, CircleAlert } from 'lucide-react';
import { useCallback, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { QrApprovalScanner } from './QrLoginPanel';
import { approveQrLogin, previewQrApproval, type QrPreview } from './qrLoginApi';
import { useTranslate } from '../../settings/i18n';

export function QrApprovalPage() {
  const tr = useTranslate();
  const navigate = useNavigate();
  const [preview, setPreview] = useState<QrPreview | null>(null);
  const [error, setError] = useState<string>();
  const [token, setToken] = useState<string>();
  const [pending, setPending] = useState(false);
  const [approved, setApproved] = useState(false);
  const scan = useCallback(async (value: string) => {
    const match = /^tjxy-login:v1:([^:]+):(.+)$/.exec(value);
    if (!match) { setError(tr('That is not a TJXY login code.', '这不是 TJXY 登录二维码。')); return; }
    const approvalToken = match.at(2);
    if (!approvalToken) { setError(tr('That login code is incomplete.', '二维码内容不完整。')); return; }
    setError(undefined);
    try {
      const next = await previewQrApproval(approvalToken);
      setToken(approvalToken);
      setPreview(next);
    } catch { setError(tr('This login code is invalid or expired.', '二维码无效或已过期。')); }
  }, [tr]);
  const approve = async () => {
    if (!token || pending) return;
    setPending(true);
    try { await approveQrLogin(token); setApproved(true); setPreview(null); }
    catch { setError(tr('The login could not be approved.', '无法批准此次登录。')); }
    finally { setPending(false); }
  };
  return (
    <div className="space-y-6">
      <div><h1 className="text-2xl font-semibold">{tr('Authorize another device', '授权其他设备')}</h1><p className="mt-1 text-sm text-muted">{tr('Scan a login code to sign in another browser or app.', '扫描登录二维码，为其他浏览器或 App 登录。')}</p></div>
      {approved ? <Alert status="success"><Alert.Indicator><CheckCircle2 className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{tr('Device authorized', '设备已授权')}</Alert.Title><Alert.Description>{tr('The other device can finish signing in now.', '其他设备现在可以完成登录。')}</Alert.Description></Alert.Content></Alert> : <QrApprovalScanner onToken={scan} />}
      {error ? <Alert role="alert" status="danger"><Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{tr('Scan failed', '扫描失败')}</Alert.Title><Alert.Description>{error}</Alert.Description></Alert.Content></Alert> : null}
      <Button onPress={() => { void navigate('/app/profile'); }} variant="tertiary">{tr('Back to profile', '返回个人中心')}</Button>
      <Modal isOpen={preview !== null} onOpenChange={(open) => { if (!open && !pending) setPreview(null); }}><Modal.Backdrop isDismissable={!pending}><Modal.Container placement="center" size="sm"><Modal.Dialog><Modal.CloseTrigger isDisabled={pending} /><Modal.Header><Modal.Heading>{tr('Confirm login', '确认登录')}</Modal.Heading></Modal.Header><Modal.Body>{preview ? <div className="space-y-2 text-sm"><p>{tr('Sign in the following device?', '确认登录以下设备？')}</p><p className="font-medium text-foreground">{preview.DeviceName}</p><p className="text-muted">{preview.ClientName} · {preview.ApplicationVersion}</p></div> : <Spinner aria-label={tr('Loading device', '正在加载设备')} />}</Modal.Body><Modal.Footer><Button isDisabled={pending} onPress={() => setPreview(null)} variant="tertiary">{tr('Cancel', '取消')}</Button><Button isPending={pending} onPress={() => { void approve(); }}>{tr('Approve login', '批准登录')}</Button></Modal.Footer></Modal.Dialog></Modal.Container></Modal.Backdrop></Modal>
    </div>
  );
}
