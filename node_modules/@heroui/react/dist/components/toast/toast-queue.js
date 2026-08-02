"use strict";
import { UNSTABLE_ToastQueue } from 'react-aria-components/Toast';
import { flushSync } from 'react-dom';
import { DEFAULT_RAC_MAX_VISIBLE_TOAST, DEFAULT_TOAST_TIMEOUT } from './constants.js';

/* ------------------------------------------------------------------------------------------------
 * Toast Queue Options
 * --------------------------------------------------------------------------------------------- */

/* ------------------------------------------------------------------------------------------------
 * Toast Queue
 * --------------------------------------------------------------------------------------------- */
/** The underlying react-stately queue passed to `ToastRegion` (not the HeroUI `ToastQueue` wrapper). */

class ToastQueue {
  constructor(options) {
    this.maxVisibleToasts = options?.maxVisibleToasts;

    // Serialize successive ViewTransitions through a tail-extending promise
    // chain. The View Transitions API only allows one active transition at a
    // time — starting a second while the first is still animating aborts the
    // first with a "Transition was skipped" DOMException (which surfaces as an
    // unhandled rejection on .ready). Appending each new transition to the end
    // of the chain (rather than attaching them all to the first active one)
    // keeps them strictly ordered even when three or more mutations stack
    // inside a single microtask, as happens during toast.promise() while the
    // loading toast's own add transition is still in flight.
    let transitionChain = Promise.resolve();
    const defaultWrapUpdate = fn => {
      if (typeof document === "undefined" || !("startViewTransition" in document)) {
        fn();
        return;
      }
      const runNext = () => {
        const transition = document.startViewTransition(() => {
          flushSync(fn);
        });

        // When a transition is superseded or aborted, it's `ready` that
        // rejects (e.g. "AbortError: Transition was skipped" /
        // "InvalidStateError: Transition was aborted because of invalid
        // state"); `finished` still fulfills. Catch `ready` so the rejection
        // doesn't surface as an unhandled promise rejection.
        transition.ready.catch(() => {});

        // Swallow the "skipped" rejection so that chain steps after a
        // superseded transition still run.
        return transition.finished.catch(() => {});
      };
      transitionChain = transitionChain.then(runNext, runNext);
    };
    this.queue = new UNSTABLE_ToastQueue({
      maxVisibleToasts: DEFAULT_RAC_MAX_VISIBLE_TOAST,
      wrapUpdate: options?.wrapUpdate ?? defaultWrapUpdate
    });
  }
  add(content, options) {
    // Apply default timeout if not provided, but respect explicit 0 (persistent toast)
    const timeout = options?.timeout !== undefined ? options.timeout : DEFAULT_TOAST_TIMEOUT;
    return this.queue.add(content, {
      ...options,
      timeout
    });
  }
  close(key) {
    this.queue.close(key);
  }
  pauseAll() {
    this.queue.pauseAll();
  }
  resumeAll() {
    this.queue.resumeAll();
  }
  clear() {
    this.queue.clear();
  }
  subscribe(fn) {
    return this.queue.subscribe(fn);
  }
  get visibleToasts() {
    return this.queue.visibleToasts;
  }
  getQueue() {
    return this.queue;
  }
}

/* ------------------------------------------------------------------------------------------------
 * Toast Queue Instance
 * --------------------------------------------------------------------------------------------- */

// Helper function to create toast
function createToastFunction(queue) {
  const toastFn = (message, options) => {
    // Use default timeout if not provided, but respect explicit 0 (persistent toast)
    const timeout = options?.timeout !== undefined ? options.timeout : DEFAULT_TOAST_TIMEOUT;
    return queue.add({
      title: message,
      description: options?.description,
      indicator: options?.indicator,
      variant: options?.variant || "default",
      actionProps: options?.actionProps,
      isLoading: options?.isLoading
    }, {
      timeout,
      onClose: () => {
        requestAnimationFrame(() => {
          options?.onClose?.();
        });
      }
    });
  };

  // Variant methods
  toastFn.success = (message, options) => {
    return toastFn(message, {
      ...options,
      variant: "success"
    });
  };
  toastFn.danger = (message, options) => {
    return toastFn(message, {
      ...options,
      variant: "danger"
    });
  };
  toastFn.info = (message, options) => {
    return toastFn(message, {
      ...options,
      variant: "accent"
    });
  };
  toastFn.warning = (message, options) => {
    return toastFn(message, {
      ...options,
      variant: "warning"
    });
  };

  // Promise support
  toastFn.promise = (promise, options) => {
    const promiseFn = typeof promise === "function" ? promise() : promise;
    const loadingId = queue.add({
      title: options.loading,
      variant: "default",
      isLoading: true
    }, {
      timeout: 0 // Don't auto-close loading toasts
    });
    promiseFn.then(data => {
      const successMessage = typeof options.success === "function" ? options.success(data) : options.success;
      queue.close(loadingId);
      return toastFn.success(successMessage);
    }).catch(error => {
      const errorMessage = typeof options.error === "function" ? options.error(error) : options.error;
      queue.close(loadingId);
      return toastFn.danger(errorMessage);
    });
    return loadingId;
  };

  // Expose queue methods for advanced usage
  toastFn.getQueue = () => queue.getQueue();
  toastFn.close = key => queue.close(key);
  toastFn.pauseAll = () => queue.pauseAll();
  toastFn.resumeAll = () => queue.resumeAll();
  toastFn.clear = () => queue.clear();
  return toastFn;
}
const toastQueue = new ToastQueue({
  maxVisibleToasts: DEFAULT_RAC_MAX_VISIBLE_TOAST
});
const toast = createToastFunction(toastQueue);

export { ToastQueue, toast, toastQueue };
