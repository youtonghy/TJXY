import { useEffect, useState } from 'react';
import { clientBlob, isRetryableClientError } from '../api/clientApi';

/// Poster reads can fail transiently while the server recovers from storage
/// reconciliation, index rebuilds, or brief database contention. Retrying keeps
/// tiles from staying blank once the server recovers.
const RETRY_DELAYS_MS = [1000, 3000, 6000, 10000] as const;

export function MediaImage({ itemId, tag, libraryId, alt, className = '' }: { itemId: string; tag?: string; libraryId?: string; alt: string; className?: string }) {
  const requestKey = `${itemId}\u0000${tag ?? ''}\u0000${libraryId ?? ''}`;
  const [loaded, setLoaded] = useState<{ requestKey: string; src: string }>();
  useEffect(() => {
    if (!tag && !libraryId) return undefined;
    const controller = new AbortController();
    let objectUrl: string | undefined;
    let retryTimer: number | undefined;
    const query = new URLSearchParams();
    if (tag) query.set('tag', tag);
    if (libraryId) query.set('libraryId', libraryId);
    const load = (attempt: number) => {
      void clientBlob(`/Items/${itemId}/Images/Primary?${query.toString()}`, controller.signal)
        .then((blob) => {
          if (controller.signal.aborted) return;
          objectUrl = URL.createObjectURL(blob);
          setLoaded({ requestKey, src: objectUrl });
        })
        .catch((error: unknown) => {
          if (controller.signal.aborted || !isRetryableClientError(error)) return;
          const delay = RETRY_DELAYS_MS[attempt];
          if (delay === undefined) return;
          retryTimer = window.setTimeout(() => { load(attempt + 1); }, delay);
        });
    };
    load(0);
    return () => {
      controller.abort();
      if (retryTimer !== undefined) window.clearTimeout(retryTimer);
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [itemId, libraryId, requestKey, tag]);
  const src = loaded?.requestKey === requestKey ? loaded.src : undefined;
  return src ? <img alt={alt} className={className} src={src} /> : <div aria-label={alt} className={`${className} bg-default`} role="img" />;
}
