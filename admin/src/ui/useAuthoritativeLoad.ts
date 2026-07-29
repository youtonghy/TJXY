import { useCallback, useEffect, useRef, useState } from 'react';

export function useAuthoritativeLoad<Result>(
  fetchResult: (signal: AbortSignal) => Promise<Result>,
  prepareResult: (result: Result) => (() => void) | Promise<() => void>,
) {
  const [loading, setLoading] = useState(true);
  const requestSequence = useRef(0);
  const activeRequest = useRef<AbortController | null>(null);
  const mounted = useRef(false);
  const effectSequence = useRef(0);

  const reload = useCallback(async () => {
    if (!mounted.current) return;
    activeRequest.current?.abort();
    const abort = new AbortController();
    activeRequest.current = abort;
    const sequence = ++requestSequence.current;
    const isCurrentRequest = () => (
      mounted.current
      && !abort.signal.aborted
      && sequence === requestSequence.current
    );
    setLoading(true);
    try {
      const result = await fetchResult(abort.signal);
      if (!isCurrentRequest()) return;
      const commitResult = await prepareResult(result);
      if (!isCurrentRequest()) return;
      commitResult();
    } finally {
      if (isCurrentRequest()) {
        if (activeRequest.current === abort) activeRequest.current = null;
        setLoading(false);
      }
    }
  }, [fetchResult, prepareResult]);

  const refreshWhenIdle = useCallback(async () => {
    if (activeRequest.current !== null) return;
    await reload();
  }, [reload]);

  useEffect(() => {
    mounted.current = true;
    const effect = ++effectSequence.current;
    void Promise.resolve().then(async () => {
      if (!mounted.current || effect !== effectSequence.current) return;
      await reload();
    }).catch(() => undefined);
    return () => {
      mounted.current = false;
      effectSequence.current += 1;
      requestSequence.current += 1;
      activeRequest.current?.abort();
      activeRequest.current = null;
    };
  }, [reload]);

  const isMounted = useCallback(() => mounted.current, []);
  return { isMounted, loading, refreshWhenIdle, reload };
}
