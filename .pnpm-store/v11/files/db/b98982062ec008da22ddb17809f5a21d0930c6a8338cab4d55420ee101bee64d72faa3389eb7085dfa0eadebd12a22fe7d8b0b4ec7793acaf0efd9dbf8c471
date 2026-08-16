"use client";
import { toastVariants } from '@heroui/styles';
import React__default, { use, createContext, useRef, useEffect, useLayoutEffect, useMemo, useCallback } from 'react';
import { Text } from 'react-aria-components/Text';
import { UNSTABLE_ToastStateContext, UNSTABLE_Toast, UNSTABLE_ToastContent, UNSTABLE_ToastRegion } from 'react-aria-components/Toast';
import { dataAttr } from '../../utils/assertion.js';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { Button } from '../button/index.js';
import { CloseButton } from '../close-button/index.js';
import { InfoIcon, DangerIcon, WarningIcon, SuccessIcon } from '../icons.js';
import { Spinner } from '../spinner/index.js';
import { DEFAULT_SCALE_FACTOR, DEFAULT_GAP, DEFAULT_MAX_VISIBLE_TOAST, DEFAULT_TOAST_WIDTH } from './constants.js';
import { toast } from './toast-queue.js';
export { ToastQueue } from './toast-queue.js';
import { jsx, jsxs } from 'react/jsx-runtime';
import { useMeasuredHeight } from '../../hooks/use-measured-height.js';
import { useMediaQuery } from '../../hooks/use-media-query.js';

const ToastContext = /*#__PURE__*/createContext({});

/* ------------------------------------------------------------------------------------------------
 * Toast
 * --------------------------------------------------------------------------------------------- */

const Toast = ({
  children,
  className,
  placement,
  scaleFactor = DEFAULT_SCALE_FACTOR,
  toast,
  variant,
  ...rest
}) => {
  const {
    gap = DEFAULT_GAP,
    heightsByKey,
    maxVisibleToasts = DEFAULT_MAX_VISIBLE_TOAST,
    onToastHeightChange,
    onToastHeightRemove,
    placement: contextPlacement,
    scaleFactor: contextScaleFactor,
    slots
  } = use(ToastContext);
  const finalPlacement = placement ?? contextPlacement;
  const finalScaleFactor = scaleFactor ?? contextScaleFactor;
  const state = use(UNSTABLE_ToastStateContext);
  const visibleToasts = state.visibleToasts;
  const index = visibleToasts.indexOf(toast);
  const isFrontmost = index <= 0;
  const isBottom = finalPlacement?.startsWith("bottom");
  const isHidden = index >= maxVisibleToasts;
  const toastKey = toast?.key;
  const toastRef = useRef(null);
  const {
    height: toastHeight
  } = useMeasuredHeight(toastRef);
  useEffect(() => {
    if (toastKey && typeof toastHeight === "number") {
      onToastHeightChange?.(toastKey, toastHeight);
    }
  }, [toastKey, toastHeight, onToastHeightChange]);

  // Drop this toast's entry from the provider's height map when it unmounts
  // (or when its key changes). Keeps `toastHeights` bounded to currently
  // mounted toasts without reading external mutable state inside a setState
  // updater.
  useEffect(() => {
    if (!toastKey) return;
    return () => {
      onToastHeightRemove?.(toastKey);
    };
  }, [toastKey, onToastHeightRemove]);

  // ToastProps from react-aria-components does not expose tabIndex as a typed
  // prop, so set it imperatively on the underlying DOM node. Only the frontmost
  // toast is reachable via keyboard; stacked/hidden toasts are removed from
  // the tab order.
  useLayoutEffect(() => {
    const el = toastRef.current;
    if (el) {
      el.tabIndex = isFrontmost ? 0 : -1;
    }
  }, [isFrontmost]);
  const style = useMemo(() => {
    const frontToastKey = visibleToasts[0]?.key;
    const frontHeight = (frontToastKey ? heightsByKey?.[frontToastKey] : undefined) ?? toastHeight ?? 0;
    const offset = index * gap;
    const translateY = (isBottom ? -1 : 1) * offset;
    const scale = 1 - index * finalScaleFactor;
    return {
      scale: `${scale}`,
      translate: `0 ${translateY}px 0`,
      viewTransitionName: `toast-${String(toast.key).replace(/[^a-zA-Z0-9]/g, "-")}`,
      zIndex: visibleToasts.length - index,
      ...(frontHeight ? {
        "--front-height": `${frontHeight}px`
      } : null),
      opacity: isHidden ? 0 : 1,
      pointerEvents: isHidden ? "none" : "auto",
      ...rest.style
    };
  }, [finalScaleFactor, gap, heightsByKey, index, isBottom, isFrontmost, isHidden, rest.style, toast?.key, toastHeight, visibleToasts]);
  return /*#__PURE__*/jsx(UNSTABLE_Toast, {
    ref: toastRef,
    "aria-hidden": isHidden,
    className: composeTwRenderProps(className, slots?.toast({
      variant
    })),
    "data-frontmost": dataAttr(isFrontmost),
    "data-hidden": dataAttr(isHidden),
    "data-index": index,
    "data-slot": "toast",
    style: style,
    toast: toast,
    ...rest,
    children: children
  });
};
Toast.displayName = "HeroUI.Toast";

/* ------------------------------------------------------------------------------------------------
 * Toast Content
 * --------------------------------------------------------------------------------------------- */

const ToastContent = ({
  children,
  className,
  ...rest
}) => {
  const {
    slots
  } = use(ToastContext);
  return /*#__PURE__*/jsx(UNSTABLE_ToastContent, {
    className: composeSlotClassName(slots?.content, className),
    "data-slot": "toast-content",
    ...rest,
    children: children
  });
};

/* ------------------------------------------------------------------------------------------------
 * Toast Indicator
 * --------------------------------------------------------------------------------------------- */

const ToastIndicator = ({
  children,
  className,
  variant,
  ...rest
}) => {
  const {
    slots
  } = use(ToastContext);
  const getDefaultIcon = useCallback(() => {
    switch (variant) {
      case "accent":
        return /*#__PURE__*/jsx(InfoIcon, {
          "data-slot": "toast-default-icon"
        });
      case "success":
        return /*#__PURE__*/jsx(SuccessIcon, {
          "data-slot": "toast-default-icon"
        });
      case "warning":
        return /*#__PURE__*/jsx(WarningIcon, {
          "data-slot": "toast-default-icon"
        });
      case "danger":
        return /*#__PURE__*/jsx(DangerIcon, {
          "data-slot": "toast-default-icon"
        });
      default:
        return /*#__PURE__*/jsx(InfoIcon, {
          "data-slot": "toast-default-icon"
        });
    }
  }, [variant]);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.indicator, className),
    "data-slot": "toast-indicator",
    ...rest,
    children: children ?? getDefaultIcon()
  });
};
ToastIndicator.displayName = "HeroUI.ToastIndicator";

/* ------------------------------------------------------------------------------------------------
 * Toast Title
 * --------------------------------------------------------------------------------------------- */

const ToastTitle = ({
  children,
  className,
  ...rest
}) => {
  const {
    slots
  } = use(ToastContext);
  return /*#__PURE__*/jsx(Text, {
    className: composeSlotClassName(slots?.title, className),
    "data-slot": "toast-title",
    slot: "title",
    ...rest,
    children: children
  });
};
ToastTitle.displayName = "HeroUI.ToastTitle";

/* ------------------------------------------------------------------------------------------------
 * Toast Description
 * --------------------------------------------------------------------------------------------- */

const ToastDescription = ({
  children,
  className,
  ...rest
}) => {
  const {
    slots
  } = use(ToastContext);
  return /*#__PURE__*/jsx(Text, {
    className: composeSlotClassName(slots?.description, className),
    "data-slot": "toast-description",
    slot: "description",
    ...rest,
    children: children
  });
};
ToastDescription.displayName = "HeroUI.ToastDescription";

/* ------------------------------------------------------------------------------------------------
 * Toast Close Button
 * --------------------------------------------------------------------------------------------- */

const ToastCloseButton = ({
  className,
  ...rest
}) => {
  const {
    slots
  } = use(ToastContext);
  return /*#__PURE__*/jsx(CloseButton, {
    className: composeTwRenderProps(className, slots?.close()),
    "data-slot": "toast-close",
    slot: "close",
    ...rest
  });
};
ToastCloseButton.displayName = "HeroUI.ToastCloseButton";

/* ------------------------------------------------------------------------------------------------
 * Toast Action Button
 * --------------------------------------------------------------------------------------------- */

const ToastActionButton = ({
  children,
  className,
  ...rest
}) => {
  const {
    slots
  } = use(ToastContext);
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, slots?.action?.()),
    "data-slot": "toast-action-button",
    ...rest,
    children: children
  });
};
ToastActionButton.displayName = "HeroUI.ToastActionButton";

/* ------------------------------------------------------------------------------------------------
 * Toast Region
 * --------------------------------------------------------------------------------------------- */

const ToastProvider = ({
  children,
  className,
  gap = DEFAULT_GAP,
  maxVisibleToasts,
  placement = "bottom",
  queue: queueProp,
  scaleFactor = DEFAULT_SCALE_FACTOR,
  width = DEFAULT_TOAST_WIDTH,
  ...rest
}) => {
  const slots = useMemo(() => toastVariants({
    placement
  }), [placement]);
  const isMobile = useMediaQuery("(max-width: 768px)");
  const [toastHeights, setToastHeights] = React__default.useState({});
  const toastQueue = useMemo(() => {
    if (queueProp) {
      // Region consumes the underlying react-stately queue, not the HeroUI wrapper.
      return queueProp.getQueue();
    }
    return toast.getQueue();
  }, [queueProp]);
  const resolvedMaxVisibleToasts = useMemo(() => {
    const queueLimit = queueProp && "maxVisibleToasts" in queueProp ? queueProp.maxVisibleToasts : undefined;
    return maxVisibleToasts ?? queueLimit ?? DEFAULT_MAX_VISIBLE_TOAST;
  }, [maxVisibleToasts, queueProp]);
  const handleToastHeightChange = useCallback((key, height) => {
    setToastHeights(prev => {
      if (prev[key] === height) {
        return prev;
      }
      return {
        ...prev,
        [key]: height
      };
    });
  }, []);

  // Removes a toast's height entry when it unmounts (called from each Toast's
  // effect cleanup). Keeps `toastHeights` bounded to currently mounted toasts.
  const handleToastHeightRemove = useCallback(key => {
    setToastHeights(prev => {
      if (!(key in prev)) {
        return prev;
      }
      const next = {
        ...prev
      };
      delete next[key];
      return next;
    });
  }, []);
  const getDefaultChildren = useCallback(renderProps => {
    const {
      actionProps,
      description,
      indicator,
      isLoading,
      title,
      variant
    } = renderProps.toast.content ?? {};
    return /*#__PURE__*/jsxs(Toast, {
      placement: placement,
      scaleFactor: scaleFactor,
      toast: renderProps.toast,
      variant: variant,
      children: [indicator === null ? null : isLoading ? /*#__PURE__*/jsx(ToastIndicator, {
        variant: variant,
        children: /*#__PURE__*/jsx(Spinner, {
          color: "current",
          size: "sm"
        })
      }) : /*#__PURE__*/jsx(ToastIndicator, {
        variant: variant,
        children: indicator
      }), /*#__PURE__*/jsxs(ToastContent, {
        children: [!!title && /*#__PURE__*/jsx(ToastTitle, {
          children: title
        }), !!description && /*#__PURE__*/jsx(ToastDescription, {
          children: description
        }), isMobile && actionProps?.children ? /*#__PURE__*/jsx(ToastActionButton, {
          ...actionProps,
          children: actionProps.children
        }) : null]
      }), !isMobile && actionProps?.children ? /*#__PURE__*/jsx(ToastActionButton, {
        ...actionProps,
        children: actionProps.children
      }) : null, /*#__PURE__*/jsx(ToastCloseButton, {})]
    });
  }, [isMobile, placement, scaleFactor]);
  return /*#__PURE__*/jsx(UNSTABLE_ToastRegion, {
    className: composeTwRenderProps(className, slots?.region()),
    "data-slot": "toast-region",
    queue: toastQueue,
    style: {
      // @ts-expect-error - CSS variables
      "--gap": `${gap}px`,
      "--placement": placement,
      "--scale-factor": scaleFactor,
      "--toast-width": typeof width === "number" ? `${width}px` : width
    },
    ...rest,
    children: renderProps => {
      const content = renderProps.toast.content;
      const renderPropsWithIsLoading = {
        ...renderProps,
        isLoading: content?.isLoading ?? false
      };
      return /*#__PURE__*/jsx(ToastContext, {
        value: {
          gap,
          heightsByKey: toastHeights,
          maxVisibleToasts: resolvedMaxVisibleToasts,
          onToastHeightChange: handleToastHeightChange,
          onToastHeightRemove: handleToastHeightRemove,
          placement,
          scaleFactor,
          slots,
          width
        },
        children: typeof children === "undefined" ? getDefaultChildren(renderProps) : typeof children === "function" ? children(renderPropsWithIsLoading) : children
      });
    }
  });
};
ToastProvider.displayName = "HeroUI.ToastProvider";

export { Toast, ToastActionButton, ToastCloseButton, ToastContent, ToastDescription, ToastIndicator, ToastProvider, ToastTitle };
