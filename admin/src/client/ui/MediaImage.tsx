import { useEffect, useState } from 'react';
import { clientBlob } from '../api/clientApi';
export function MediaImage({ itemId, tag, alt, className = '' }: { itemId: string; tag?: string; alt: string; className?: string }) {
  const [src, setSrc] = useState<string>();
  useEffect(() => { if (!tag) return undefined; const controller = new AbortController(); let objectUrl: string | undefined; void clientBlob(`/Items/${itemId}/Images/Primary?tag=${encodeURIComponent(tag)}`, controller.signal).then((blob) => { if (controller.signal.aborted) return; objectUrl = URL.createObjectURL(blob); setSrc(objectUrl); }).catch(() => undefined); return () => { controller.abort(); if (objectUrl) URL.revokeObjectURL(objectUrl); }; }, [itemId, tag]);
  return src ? <img alt={alt} className={className} src={src} /> : <div aria-label={alt} className={`${className} bg-default`} role="img" />;
}
