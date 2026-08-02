"use client";
import { modalVariants } from '@heroui/styles';
import { mergeProps } from '@react-aria/utils';
import { useMemo, createContext, use } from 'react';
import { DialogTrigger, Heading, Dialog } from 'react-aria-components/Dialog';
import { Modal, ModalOverlay, Pressable } from 'react-aria-components/Modal';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { CloseButton } from '../close-button/index.js';
import { jsx, jsxs } from 'react/jsx-runtime';
import { SurfaceContext } from '../surface/surface.js';

const ModalContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Modal Root
 * -----------------------------------------------------------------------------------------------*/

const ModalRoot = ({
  children,
  state,
  ...props
}) => {
  const modalContext = useMemo(() => ({
    slots: modalVariants(),
    placement: undefined
  }), []);
  const controlledProps = useMemo(() => state ? {
    isOpen: state.isOpen,
    onOpenChange: state.setOpen
  } : {}, [state]);
  return /*#__PURE__*/jsx(ModalContext, {
    value: modalContext,
    children: /*#__PURE__*/jsx(DialogTrigger, {
      "data-slot": "modal-root",
      ...mergeProps(props, controlledProps),
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Modal Trigger
 * -----------------------------------------------------------------------------------------------*/

const ModalTrigger = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ModalContext);
  return /*#__PURE__*/jsx(Pressable, {
    children: /*#__PURE__*/jsx(dom.div, {
      className: composeSlotClassName(slots?.trigger, className),
      "data-slot": "modal-trigger",
      role: "button",
      ...props,
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Modal Backdrop
 * -----------------------------------------------------------------------------------------------*/

const ModalBackdrop = ({
  children,
  className,
  isDismissable = true,
  onClick,
  variant,
  ...props
}) => {
  const {
    slots: contextSlots
  } = use(ModalContext);
  const updatedSlots = useMemo(() => modalVariants({
    variant
  }), [variant]);
  const updatedModalContext = useMemo(() => ({
    slots: {
      ...contextSlots,
      ...updatedSlots
    }
  }), [contextSlots, updatedSlots]);
  return /*#__PURE__*/jsx(ModalOverlay, {
    className: composeTwRenderProps(className, updatedSlots?.backdrop()),
    "data-slot": "modal-backdrop",
    isDismissable: isDismissable,
    onClick: e => {
      e.stopPropagation();
      onClick?.(e);
    },
    ...props,
    children: renderProps => /*#__PURE__*/jsxs(ModalContext, {
      value: updatedModalContext,
      children: [typeof children === "function" ? children(renderProps) : children, " "]
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Modal Container
 * -----------------------------------------------------------------------------------------------*/

const ModalContainer = ({
  children,
  className,
  placement = "auto",
  scroll,
  size,
  ...props
}) => {
  const {
    slots: contextSlots
  } = use(ModalContext);
  const updatedSlots = useMemo(() => modalVariants({
    scroll,
    size
  }), [scroll, size]);
  const updatedModalContext = useMemo(() => ({
    placement,
    slots: {
      ...contextSlots,
      ...updatedSlots
    }
  }), [contextSlots, placement, updatedSlots]);
  return /*#__PURE__*/jsx(Modal, {
    className: composeTwRenderProps(className, updatedSlots?.container()),
    "data-placement": placement,
    "data-slot": "modal-container",
    ...props,
    children: renderProps => /*#__PURE__*/jsx(ModalContext, {
      value: updatedModalContext,
      children: typeof children === "function" ? children(renderProps) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Modal Dialog
 * -----------------------------------------------------------------------------------------------*/

const ModalDialog = ({
  children,
  className,
  ...props
}) => {
  const {
    placement,
    slots
  } = use(ModalContext);
  return /*#__PURE__*/jsx(SurfaceContext, {
    value: {
      variant: "default"
    },
    children: /*#__PURE__*/jsx(Dialog, {
      className: composeSlotClassName(slots?.dialog, className),
      "data-placement": placement,
      "data-slot": "modal-dialog",
      ...props,
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Modal Header
 * -----------------------------------------------------------------------------------------------*/

const ModalHeader = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ModalContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.header, className),
    "data-slot": "modal-header",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Modal Body
 * -----------------------------------------------------------------------------------------------*/

const ModalBody = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ModalContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.body, className),
    "data-slot": "modal-body",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Modal Footer
 * -----------------------------------------------------------------------------------------------*/

const ModalFooter = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ModalContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.footer, className),
    "data-slot": "modal-footer",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Modal Heading
 * -----------------------------------------------------------------------------------------------*/

const ModalHeading = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ModalContext);
  return /*#__PURE__*/jsx(Heading, {
    className: composeSlotClassName(slots?.heading, className),
    "data-slot": "modal-heading",
    slot: "title",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Icon
 * -----------------------------------------------------------------------------------------------*/

const ModalIcon = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ModalContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.icon, className),
    "data-slot": "modal-icon",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Modal Close Trigger
 * -----------------------------------------------------------------------------------------------*/

const ModalCloseTrigger = ({
  className,
  ...rest
}) => {
  const {
    slots
  } = use(ModalContext);
  return /*#__PURE__*/jsx(CloseButton, {
    className: composeTwRenderProps(className, slots?.closeTrigger()),
    "data-slot": "modal-close-trigger",
    slot: "close",
    ...rest
  });
};

export { ModalBackdrop, ModalBody, ModalCloseTrigger, ModalContainer, ModalDialog, ModalFooter, ModalHeader, ModalHeading, ModalIcon, ModalRoot, ModalTrigger };
