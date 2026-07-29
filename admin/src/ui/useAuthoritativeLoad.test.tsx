import { act, renderHook, waitFor } from '@testing-library/react';
import { StrictMode, type ReactNode } from 'react';

import { useAuthoritativeLoad } from './useAuthoritativeLoad';

it('awaits async result application before settling the current load', async () => {
  const commitResult = vi.fn();
  let finishApply: (() => void) | undefined;
  const prepareResult = vi.fn(() => new Promise<() => void>((resolve) => {
    finishApply = () => { resolve(commitResult); };
  }));
  const fetchResult = vi.fn().mockResolvedValue('current');

  const { result } = renderHook(() => useAuthoritativeLoad(fetchResult, prepareResult));

  await waitFor(() => { expect(prepareResult).toHaveBeenCalledWith('current'); });
  expect(result.current.loading).toBe(true);

  act(() => { finishApply?.(); });
  await waitFor(() => { expect(result.current.loading).toBe(false); });
  expect(commitResult).toHaveBeenCalledOnce();
});

it('settles loading when fetching or applying a result rejects', async () => {
  const fetchResult = vi.fn()
    .mockRejectedValueOnce(new Error('fetch failed'))
    .mockResolvedValueOnce('next');
  const prepareResult = vi.fn().mockRejectedValueOnce(new Error('apply failed'));

  const { result } = renderHook(() => useAuthoritativeLoad(fetchResult, prepareResult));

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
  const commitResult = vi.fn();
  const prepareResult = vi.fn(() => commitResult);

  const { result } = renderHook(() => useAuthoritativeLoad(fetchResult, prepareResult));
  await waitFor(() => { expect(fetchResult).toHaveBeenCalledTimes(1); });

  let latestReload: Promise<void> | undefined;
  act(() => { latestReload = result.current.reload(); });
  expect(signals[0]?.aborted).toBe(true);

  await act(async () => {
    resolvers[0]?.('stale');
    await Promise.resolve();
  });
  expect(prepareResult).not.toHaveBeenCalled();
  expect(result.current.loading).toBe(true);

  await act(async () => {
    resolvers[1]?.('latest');
    await latestReload;
  });
  expect(prepareResult).toHaveBeenCalledWith('latest');
  expect(commitResult).toHaveBeenCalledOnce();
  expect(result.current.loading).toBe(false);
});

it('discards an obsolete commit that finishes preparing after a newer request wins', async () => {
  const staleCommit = vi.fn();
  const latestCommit = vi.fn();
  let finishStalePreparation: (() => void) | undefined;
  const fetchResult = vi.fn()
    .mockResolvedValueOnce('stale')
    .mockResolvedValueOnce('latest');
  const prepareResult = vi.fn((value: string) => (
    value === 'stale'
      ? new Promise<() => void>((resolve) => {
        finishStalePreparation = () => { resolve(staleCommit); };
      })
      : Promise.resolve(latestCommit)
  ));

  const { result } = renderHook(() => useAuthoritativeLoad(fetchResult, prepareResult));
  await waitFor(() => { expect(prepareResult).toHaveBeenCalledWith('stale'); });

  await act(async () => { await result.current.reload(); });
  expect(latestCommit).toHaveBeenCalledOnce();

  act(() => { finishStalePreparation?.(); });
  await waitFor(() => { expect(result.current.loading).toBe(false); });
  expect(staleCommit).not.toHaveBeenCalled();
});

it('does not supersede a long-running request when an idle refresh is requested', async () => {
  const signals: AbortSignal[] = [];
  const resolvers: ((value: string) => void)[] = [];
  const fetchResult = vi.fn((signal: AbortSignal) => {
    signals.push(signal);
    return new Promise<string>((resolve) => { resolvers.push(resolve); });
  });
  const prepareResult = vi.fn(() => () => undefined);
  const { result } = renderHook(() => useAuthoritativeLoad(fetchResult, prepareResult));
  await waitFor(() => { expect(fetchResult).toHaveBeenCalledOnce(); });

  await act(async () => { await result.current.refreshWhenIdle(); });
  expect(fetchResult).toHaveBeenCalledOnce();
  expect(signals[0]?.aborted).toBe(false);

  act(() => { resolvers[0]?.('first'); });
  await waitFor(() => { expect(result.current.loading).toBe(false); });

  act(() => { void result.current.refreshWhenIdle(); });
  await waitFor(() => { expect(fetchResult).toHaveBeenCalledTimes(2); });
});

it('starts one initial request under StrictMode', async () => {
  const fetchResult = vi.fn().mockResolvedValue('current');
  const prepareResult = vi.fn(() => () => undefined);
  const wrapper = ({ children }: { children: ReactNode }) => <StrictMode>{children}</StrictMode>;

  renderHook(() => useAuthoritativeLoad(fetchResult, prepareResult), { wrapper });

  await waitFor(() => { expect(prepareResult).toHaveBeenCalledWith('current'); });
  expect(fetchResult).toHaveBeenCalledOnce();
});

it('aborts the active request on unmount', async () => {
  let signal: AbortSignal | undefined;
  const fetchResult = vi.fn((requestSignal: AbortSignal) => {
    signal = requestSignal;
    return new Promise<string>(() => undefined);
  });

  const { unmount } = renderHook(() => useAuthoritativeLoad(fetchResult, () => () => undefined));
  await waitFor(() => { expect(signal).toBeDefined(); });
  unmount();

  expect(signal?.aborted).toBe(true);
});
