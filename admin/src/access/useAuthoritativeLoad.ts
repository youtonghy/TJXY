import { useCallback, useEffect, useRef, useState } from 'react';

export function useAuthoritativeLoad<Result>(
  fetchResult: (signal: AbortSignal) => Promise<Result>,
  applyResult: (result: Result) => void,
) {
  const [loading, setLoading] = useState(true);
  const requestSequence = useRef(0);
  const activeRequest = useRef<AbortController | null>(null);
  const mounted = useRef(false);

  const reload = useCallback(async () => {
    if (!mounted.current) return;
    activeRequest.current?.abort();
    const abort = new AbortController();
    activeRequest.current = abort;
    const sequence = ++requestSequence.current;
    setLoading(true);
    const result = await fetchResult(abort.signal);
    if (abort.signal.aborted || sequence !== requestSequence.current) return;
    applyResult(result);
    setLoading(false);
  }, [applyResult, fetchResult]);

  useEffect(() => {
    mounted.current = true;
    void reload();
    return () => {
      mounted.current = false;
      requestSequence.current += 1;
      activeRequest.current?.abort();
    };
  }, [reload]);

  const isMounted = useCallback(() => mounted.current, []);
  return { isMounted, loading, reload };
}
