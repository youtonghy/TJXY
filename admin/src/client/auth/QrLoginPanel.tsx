/* eslint-disable @typescript-eslint/no-confusing-void-expression, @typescript-eslint/no-unnecessary-condition, @typescript-eslint/restrict-template-expressions, react-hooks/exhaustive-deps, react-hooks/set-state-in-effect */
import { Alert, Button, Card, Input, Label, Spinner, TextField } from '@heroui/react';
import { QRCodeSVG } from 'qrcode.react';
import { CircleAlert, RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { createQrChallenge, pollQrChallenge, type QrAuthentication } from './qrLoginApi';
import { useTranslate } from '../../settings/i18n';

export function QrLoginPanel({ onAuthenticated }: { onAuthenticated: (authentication: QrAuthentication) => Promise<void> }) {
  const tr = useTranslate();
  const [challenge, setChallenge] = useState<Awaited<ReturnType<typeof createQrChallenge>> | null>(null);
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(true);
  const [secondsLeft, setSecondsLeft] = useState(0);

  const refresh = () => {
    setLoading(true);
    setError(undefined);
    void createQrChallenge().then((next) => {
      setChallenge(next);
      setSecondsLeft(Math.max(0, Math.ceil((Date.parse(next.ExpiresAt) - Date.now()) / 1000)));
    }).catch(() => setError(tr('The QR code could not be created.', '无法创建二维码。'))).finally(() => setLoading(false));
  };

  useEffect(() => { refresh(); }, []);
  useEffect(() => {
    if (!challenge) return undefined;
    const controller = new AbortController();
    const poll = () => {
      void pollQrChallenge(challenge.ChallengeId, challenge.PollToken, controller.signal).then((result) => {
        if (result.Authentication) void onAuthenticated(result.Authentication);
      }).catch(() => undefined);
    };
    poll();
    const interval = window.setInterval(poll, 2000);
    const countdown = window.setInterval(() => setSecondsLeft(Math.max(0, Math.ceil((Date.parse(challenge.ExpiresAt) - Date.now()) / 1000))), 1000);
    return () => { controller.abort(); window.clearInterval(interval); window.clearInterval(countdown); };
  }, [challenge, onAuthenticated]);

  return (
    <div className="space-y-5">
      <Card className="bg-surface-secondary">
        <Card.Content className="flex flex-col items-center gap-4 p-6 text-center">
          {loading ? <Spinner aria-label={tr('Creating QR code', '正在创建二维码')} /> : challenge ? <QRCodeSVG aria-label={tr('QR login code', '二维码登录码')} value={challenge.QrPayload} size={220} /> : null}
          <p className="text-sm text-muted">{secondsLeft > 0 ? tr(`Expires in ${secondsLeft}s`, `${secondsLeft} 秒后过期`) : tr('This code has expired.', '二维码已过期。')}</p>
          <Button isDisabled={loading} onPress={refresh} size="sm" variant="secondary"><RefreshCw className="size-4" />{tr('Refresh code', '刷新二维码')}</Button>
        </Card.Content>
      </Card>
      {error ? <Alert role="alert" status="danger"><Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator><Alert.Content><Alert.Title>{tr('QR login unavailable', '二维码登录不可用')}</Alert.Title><Alert.Description>{error}</Alert.Description></Alert.Content></Alert> : null}
    </div>
  );
}

export function QrApprovalScanner({ onToken }: { onToken: (token: string) => void | Promise<void> }) {
  const tr = useTranslate();
  const [value, setValue] = useState('');
  const decodeFile = async (file: File | undefined) => {
    if (!file) return;
    try {
      const { BrowserQRCodeReader } = await import('@zxing/browser');
      const url = URL.createObjectURL(file);
      try { const result = await new BrowserQRCodeReader().decodeFromImageUrl(url); void onToken(result.getText()); }
      finally { URL.revokeObjectURL(url); }
    } catch { /* Camera and paste remain available when the image has no QR code. */ }
  };
  return (
    <Card>
      <Card.Header><Card.Title>{tr('Scan a login code', '扫描登录二维码')}</Card.Title><Card.Description>{tr('Use your camera, or paste a code from another device.', '使用摄像头，或粘贴其他设备上的二维码内容。')}</Card.Description></Card.Header>
      <Card.Content className="space-y-4">
        <CameraCapture onToken={onToken} />
        <label className="block text-sm text-muted">{tr('Or choose a QR image', '或选择二维码图片')}<input accept="image/*" className="mt-2 block w-full text-sm" onChange={(event) => { void decodeFile(event.currentTarget.files?.[0]); }} type="file" /></label>
        <TextField fullWidth name="qr-token"><Label>{tr('Code', '二维码内容')}</Label><Input value={value} onChange={(event) => setValue(event.currentTarget.value)} /><Button className="mt-2" isDisabled={!value.trim()} onPress={() => { void onToken(value.trim()); }} size="sm" variant="secondary">{tr('Use code', '使用二维码')}</Button></TextField>
      </Card.Content>
    </Card>
  );
}

function CameraCapture({ onToken }: { onToken: (token: string) => void | Promise<void> }) {
  const tr = useTranslate();
  const [status, setStatus] = useState<'idle' | 'active' | 'blocked'>('idle');
  useEffect(() => {
    let controls: { stop: () => void } | undefined;
    let disposed = false;
    const video = document.querySelector<HTMLVideoElement>('[data-qr-camera]');
    if (!video || !navigator.mediaDevices?.getUserMedia) { setStatus('blocked'); return undefined; }
    void import('@zxing/browser').then(async ({ BrowserQRCodeReader }) => {
      if (disposed) return;
      const reader = new BrowserQRCodeReader();
      try {
        controls = await reader.decodeFromVideoDevice(undefined, video, (result) => {
          if (result) void onToken(result.getText());
        });
        if (!disposed) setStatus('active');
      } catch { if (!disposed) setStatus('blocked'); }
    });
    return () => {
      disposed = true;
      controls?.stop();
      const stream = video.srcObject;
      if (stream instanceof MediaStream) stream.getTracks().forEach((track: MediaStreamTrack) => track.stop());
    };
  }, [onToken]);
  return <div className="overflow-hidden rounded-2xl border border-border bg-black"><video aria-label={tr('QR camera preview', '二维码摄像头预览')} autoPlay className="aspect-video w-full object-cover" data-qr-camera muted playsInline />{status === 'blocked' ? <p className="p-3 text-center text-sm text-white/80">{tr('Camera access is unavailable. Paste the QR content below.', '无法使用摄像头，请在下方粘贴二维码内容。')}</p> : <p className="p-3 text-center text-sm text-white/80">{status === 'active' ? tr('Point the camera at a TJXY login code.', '将摄像头对准 TJXY 登录二维码。') : tr('Requesting camera access…', '正在请求摄像头权限…')}</p>}</div>;
}
