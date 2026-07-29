import { act, renderHook, waitFor } from '@testing-library/react';

import { useAuthoritativeLoad } from './useAuthoritativeLoad';

it('awaits async result application before settling the current load', async () => {
  let finishApply: (() => void) | undefined;
  const applyResult = vi.fn(() => new Promise<void>((resolve) => {
    finishApply = resolve;
  }));
  const fetchResult = vi.fn().mockResolvedValue('current');

  const { result } = renderHook(() => useAuthoritativeLoad(fetchResult, applyResult));

  await waitFor(() => { expect(applyResult).toHaveBeenCalledWith('current'); });
  expect(result.current.loading).toBe(true);

  act(() => { finishApply?.(); });
  await waitFor(() => { expect(result.current.loading).toBe(false); });
});

it('settles loading when fetching or applying a result rejects', async () => {
  const fetchResult = vi.fn()
    .mockRejectedValueOnce(new Error('fetch failed'))
    .mockResolvedValueOnce('next');
  const applyResult = vi.fn().mockRejectedValueOnce(new Error('apply failed'));

  const { result } = renderHook(() => useAuthoritativeLoad(fetchResult, applyResult));

  await waitFor(() => { expect(result.current.loading).toBe(false); });

  await expect(result.current.reload()).rejects.toThrow('apply failed');
  await waitFor(() => { expect(result.current.loading).toBe(false); });
});

it('aborts superseded requests and lets only the latest request settle loading', async () => {
  const signals: AbortSignal[] = [];
  const resolvers: ((value: string) => void)[] = [];
  const fetchResult = vi.fn((signal: AbortSignal) => {
    signals.push(signal);
    return new Promise<string>((resolve) => { resolvers.push(resolve); });
  });
  const applyResult = vi.fn();

  const { result } = renderHook(() => useAuthoritativeLoad(fetchResult, applyResult));
  await waitFor(() => { expect(fetchResult).toHaveBeenCalledTimes(1); });

  let latestReload: Promise<void> | undefined;
  act(() => { latestReload = result.current.reload(); });
  expect(signals[0]?.aborted).toBe(true);

  await act(async () => {
    resolvers[0]?.('stale');
    await Promise.resolve();
  });
  expect(applyResult).not.toHaveBeenCalled();
  expect(result.current.loading).toBe(true);

  await act(async () => {
    resolvers[1]?.('latest');
    await latestReload;
  });
  expect(applyResult).toHaveBeenCalledWith('latest');
  expect(result.current.loading).toBe(false);
});

it('aborts the active request on unmount', async () => {
  let signal: AbortSignal | undefined;
  const fetchResult = vi.fn((requestSignal: AbortSignal) => {
    signal = requestSignal;
    return new Promise<string>(() => undefined);
  });

  const { unmount } = renderHook(() => useAuthoritativeLoad(fetchResult, vi.fn()));
  await waitFor(() => { expect(signal).toBeDefined(); });
  unmount();

  expect(signal?.aborted).toBe(true);
});
