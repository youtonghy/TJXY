import { useEffect, useState } from 'react';
import { clientBlob } from '../api/clientApi';

export function MediaImage({ itemId, tag, libraryId, alt, className = '' }: { itemId: string; tag?: string; libraryId?: string; alt: string; className?: string }) {
  const requestKey = `${itemId}\u0000${tag ?? ''}\u0000${libraryId ?? ''}`;
  const [loaded, setLoaded] = useState<{ requestKey: string; src: string }>();
  useEffect(() => {
    if (!tag && !libraryId) return undefined;
    const controller = new AbortController();
    let objectUrl: string | undefined;
    const query = new URLSearchParams();
    if (tag) query.set('tag', tag);
    if (libraryId) query.set('libraryId', libraryId);
    void clientBlob(`/Items/${itemId}/Images/Primary?${query.toString()}`, controller.signal)
      .then((blob) => {
        if (controller.signal.aborted) return;
        objectUrl = URL.createObjectURL(blob);
        setLoaded({ requestKey, src: objectUrl });
      })
      .catch(() => undefined);
    return () => {
      controller.abort();
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [itemId, libraryId, requestKey, tag]);
  const src = loaded?.requestKey === requestKey ? loaded.src : undefined;
  return src ? <img alt={alt} className={className} src={src} /> : <div aria-label={alt} className={`${className} bg-default`} role="img" />;
}
