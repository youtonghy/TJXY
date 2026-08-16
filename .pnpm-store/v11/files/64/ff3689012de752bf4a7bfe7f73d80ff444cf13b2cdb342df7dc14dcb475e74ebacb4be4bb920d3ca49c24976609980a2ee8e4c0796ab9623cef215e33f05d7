"use client";
import { drawerVariants } from '@heroui/styles';
import { mergeProps } from '@react-aria/utils';
import { useMemo, createContext, use, useRef, useCallback } from 'react';
import { Button } from 'react-aria-components/Button';
import { DialogTrigger, Heading, Dialog, OverlayTriggerStateContext } from 'react-aria-components/Dialog';
import { Modal, ModalOverlay } from 'react-aria-components/Modal';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { CloseButton } from '../close-button/index.js';
import { jsx } from 'react/jsx-runtime';
import { SurfaceContext } from '../surface/surface.js';

/* -------------------------------------------------------------------------------------------------
 * Drawer Drag Hook
 * Tracks pointer events to enable drag-to-dismiss with CSS transforms.
 * Drag only initiates from handle/header/footer areas — body is excluded to avoid scroll conflicts.
 * -----------------------------------------------------------------------------------------------*/

const DRAG_THRESHOLD = 8; // px before drag activates
const DISMISS_FRACTION = 0.3; // dismiss if dragged > 30% of dimension
const VELOCITY_THRESHOLD = 0.5; // px/ms — dismiss on fast flick

function useDrawerDrag(placement, isDismissable) {
  const overlayState = use(OverlayTriggerStateContext);
  const dialogRef = useRef(null);
  const isDragging = useRef(false);
  const isActive = useRef(false);
  const startPos = useRef(0);
  const currentOffset = useRef(0);
  const velocity = useRef(0);
  const lastTime = useRef(0);
  const lastPos = useRef(0);
  const isVertical = placement === "top" || placement === "bottom";
  const getPos = useCallback(e => isVertical ? e.clientY : e.clientX, [isVertical]);
  const clamp = useCallback(delta => {
    // Only allow drag in the dismiss direction
    switch (placement) {
      case "bottom":
        return Math.max(0, delta);
      case "top":
        return Math.min(0, delta);
      case "right":
        return Math.max(0, delta);
      case "left":
        return Math.min(0, delta);
      default:
        return delta;
    }
  }, [placement]);
  const onPointerDown = useCallback(e => {
    if (!isDismissable) return;
    if (e.button !== 0) return;
    const target = e.target;

    // Don't drag from interactive elements or scrollable body
    if (target.closest("input, textarea, button, [role='button'], select, a, [data-slot='drawer-body']")) {
      return;
    }
    isDragging.current = true;
    isActive.current = false;
    startPos.current = getPos(e);
    lastPos.current = startPos.current;
    lastTime.current = Date.now();
    currentOffset.current = 0;
    velocity.current = 0;
  }, [getPos, isDismissable]);
  const onPointerMove = useCallback(e => {
    if (!isDragging.current || !dialogRef.current) return;
    const pos = getPos(e);
    const rawDelta = pos - startPos.current;
    const delta = clamp(rawDelta);

    // Activate only after passing threshold to avoid false starts
    if (!isActive.current) {
      if (Math.abs(rawDelta) < DRAG_THRESHOLD) return;
      isActive.current = true;
      dialogRef.current.style.transition = "none";
      dialogRef.current.setPointerCapture(e.pointerId);
    }
    currentOffset.current = delta;

    // Track velocity for flick detection
    const now = Date.now();
    const dt = now - lastTime.current;
    if (dt > 0) {
      velocity.current = (pos - lastPos.current) / dt;
      lastTime.current = now;
      lastPos.current = pos;
    }
    const axis = isVertical ? "Y" : "X";
    dialogRef.current.style.transform = `translate${axis}(${delta}px)`;
  }, [getPos, clamp, isVertical]);
  const onPointerUp = useCallback(e => {
    if (!isDragging.current) return;
    isDragging.current = false;
    const el = dialogRef.current;
    if (!el || !isActive.current) {
      isActive.current = false;
      return;
    }
    isActive.current = false;

    // Release pointer capture
    try {
      el.releasePointerCapture(e.pointerId);
    } catch {
      // Pointer capture may already be released
    }
    const dimension = isVertical ? el.offsetHeight : el.offsetWidth;
    const absOffset = Math.abs(currentOffset.current);
    const absVelocity = Math.abs(velocity.current);
    const shouldDismiss = absOffset > dimension * DISMISS_FRACTION || absVelocity > VELOCITY_THRESHOLD;
    if (shouldDismiss && overlayState) {
      // Keep the inline transform — it compounds with the content exit animation
      // so the drawer continues sliding from the dragged position
      overlayState.close();
    } else {
      // Snap back with a spring-like ease
      el.style.transition = "transform 300ms cubic-bezier(0.32, 0.72, 0, 1)";
      el.style.transform = "";
      const cleanup = () => {
        el.style.transition = "";
      };
      el.addEventListener("transitionend", cleanup, {
        once: true
      });
    }
    currentOffset.current = 0;
    velocity.current = 0;
  }, [isVertical, overlayState]);
  return {
    dialogRef,
    dragHandlers: isDismissable ? {
      onPointerDown,
      onPointerMove,
      onPointerUp
    } : {}
  };
}

/* -------------------------------------------------------------------------------------------------
 * Drawer Context
 * -----------------------------------------------------------------------------------------------*/

const DrawerContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Drawer Root
 * -----------------------------------------------------------------------------------------------*/

const DrawerRoot = ({
  children,
  state,
  ...props
}) => {
  const drawerContext = useMemo(() => ({
    slots: drawerVariants(),
    placement: undefined,
    isDismissable: true
  }), []);
  const controlledProps = useMemo(() => state ? {
    isOpen: state.isOpen,
    onOpenChange: state.setOpen
  } : {}, [state]);
  return /*#__PURE__*/jsx(DrawerContext, {
    value: drawerContext,
    children: /*#__PURE__*/jsx(DialogTrigger, {
      "data-slot": "drawer-root",
      ...mergeProps(props, controlledProps),
      children: children
    })
  });
};
DrawerRoot.displayName = "HeroUI.Drawer";

/* -------------------------------------------------------------------------------------------------
 * Drawer Trigger
 * -----------------------------------------------------------------------------------------------*/

const DrawerTrigger = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(DrawerContext);
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, slots?.trigger()),
    "data-slot": "drawer-trigger",
    ...props,
    children: children
  });
};
DrawerTrigger.displayName = "HeroUI.Drawer.Trigger";

/* -------------------------------------------------------------------------------------------------
 * Drawer Backdrop
 * -----------------------------------------------------------------------------------------------*/

const DrawerBackdrop = ({
  children,
  className,
  isDismissable = true,
  variant,
  ...props
}) => {
  const {
    slots: contextSlots
  } = use(DrawerContext);
  const updatedSlots = useMemo(() => drawerVariants({
    variant
  }), [variant]);
  const updatedDrawerContext = useMemo(() => ({
    slots: {
      ...contextSlots,
      ...updatedSlots
    },
    isDismissable
  }), [contextSlots, updatedSlots, isDismissable]);
  return /*#__PURE__*/jsx(ModalOverlay, {
    className: composeTwRenderProps(className, updatedSlots?.backdrop()),
    "data-slot": "drawer-backdrop",
    isDismissable: isDismissable,
    ...props,
    children: renderProps => /*#__PURE__*/jsx(DrawerContext, {
      value: updatedDrawerContext,
      children: typeof children === "function" ? children(renderProps) : children
    })
  });
};
DrawerBackdrop.displayName = "HeroUI.Drawer.Backdrop";

/* -------------------------------------------------------------------------------------------------
 * Drawer Content
 * -----------------------------------------------------------------------------------------------*/

const DrawerContent = ({
  children,
  className,
  placement = "bottom",
  ...props
}) => {
  const {
    isDismissable,
    slots: contextSlots
  } = use(DrawerContext);
  const updatedSlots = useMemo(() => drawerVariants({
    placement
  }), [placement]);
  const updatedDrawerContext = useMemo(() => ({
    placement,
    isDismissable,
    slots: {
      ...contextSlots,
      ...updatedSlots
    }
  }), [contextSlots, placement, isDismissable, updatedSlots]);
  return /*#__PURE__*/jsx(Modal, {
    className: composeTwRenderProps(className, updatedSlots?.content()),
    "data-placement": placement,
    "data-slot": "drawer-content",
    ...props,
    children: renderProps => /*#__PURE__*/jsx(DrawerContext, {
      value: updatedDrawerContext,
      children: typeof children === "function" ? children(renderProps) : children
    })
  });
};
DrawerContent.displayName = "HeroUI.Drawer.Content";

/* -------------------------------------------------------------------------------------------------
 * Drawer Dialog
 * -----------------------------------------------------------------------------------------------*/

const DrawerDialog = ({
  children,
  className,
  ...props
}) => {
  const {
    isDismissable = true,
    placement,
    slots
  } = use(DrawerContext);
  const {
    dialogRef,
    dragHandlers
  } = useDrawerDrag(placement, isDismissable);
  return /*#__PURE__*/jsx(SurfaceContext, {
    value: {
      variant: "default"
    },
    children: /*#__PURE__*/jsx(Dialog, {
      ref: dialogRef,
      className: composeSlotClassName(slots?.dialog, className),
      "data-placement": placement,
      "data-slot": "drawer-dialog",
      style: isDismissable ? {
        touchAction: "none"
      } : undefined,
      ...dragHandlers,
      ...props,
      children: children
    })
  });
};
DrawerDialog.displayName = "HeroUI.Drawer.Dialog";

/* -------------------------------------------------------------------------------------------------
 * Drawer Header
 * -----------------------------------------------------------------------------------------------*/

const DrawerHeader = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(DrawerContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.header, className),
    "data-slot": "drawer-header",
    ...props,
    children: children
  });
};
DrawerHeader.displayName = "HeroUI.Drawer.Header";

/* -------------------------------------------------------------------------------------------------
 * Drawer Body
 * -----------------------------------------------------------------------------------------------*/

const DrawerBody = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(DrawerContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.body, className),
    "data-slot": "drawer-body",
    style: {
      touchAction: "pan-y"
    },
    ...props,
    children: children
  });
};
DrawerBody.displayName = "HeroUI.Drawer.Body";

/* -------------------------------------------------------------------------------------------------
 * Drawer Footer
 * -----------------------------------------------------------------------------------------------*/

const DrawerFooter = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(DrawerContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.footer, className),
    "data-slot": "drawer-footer",
    ...props,
    children: children
  });
};
DrawerFooter.displayName = "HeroUI.Drawer.Footer";

/* -------------------------------------------------------------------------------------------------
 * Drawer Heading
 * -----------------------------------------------------------------------------------------------*/

const DrawerHeading = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(DrawerContext);
  return /*#__PURE__*/jsx(Heading, {
    className: composeSlotClassName(slots?.heading, className),
    "data-slot": "drawer-heading",
    slot: "title",
    ...props,
    children: children
  });
};
DrawerHeading.displayName = "HeroUI.Drawer.Heading";

/* -------------------------------------------------------------------------------------------------
 * Drawer Handle
 * -----------------------------------------------------------------------------------------------*/

const DrawerHandle = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(DrawerContext);
  return /*#__PURE__*/jsx(dom.div, {
    "aria-hidden": "true",
    className: composeSlotClassName(slots?.handle, className),
    "data-slot": "drawer-handle",
    ...props,
    children: /*#__PURE__*/jsx("div", {
      "data-slot": "drawer-handle-bar"
    })
  });
};
DrawerHandle.displayName = "HeroUI.Drawer.Handle";

/* -------------------------------------------------------------------------------------------------
 * Drawer Close Trigger
 * -----------------------------------------------------------------------------------------------*/

const DrawerCloseTrigger = ({
  className,
  ...rest
}) => {
  const {
    slots
  } = use(DrawerContext);
  return /*#__PURE__*/jsx(CloseButton, {
    className: composeTwRenderProps(className, slots?.closeTrigger()),
    "data-slot": "drawer-close-trigger",
    slot: "close",
    ...rest
  });
};
DrawerCloseTrigger.displayName = "HeroUI.Drawer.CloseTrigger";

export { DrawerBackdrop, DrawerBody, DrawerCloseTrigger, DrawerContent, DrawerDialog, DrawerFooter, DrawerHandle, DrawerHeader, DrawerHeading, DrawerRoot, DrawerTrigger };
