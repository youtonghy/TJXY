"use client";
import { alertDialogVariants } from '@heroui/styles';
import { useMemo, createContext, use } from 'react';
import { DialogTrigger, Heading, Dialog } from 'react-aria-components/Dialog';
import { Modal, ModalOverlay, Pressable } from 'react-aria-components/Modal';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { CloseButton } from '../close-button/index.js';
import { DangerIcon, WarningIcon, SuccessIcon, InfoIcon } from '../icons.js';
import { jsx, jsxs } from 'react/jsx-runtime';

const AlertDialogContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Root
 * -----------------------------------------------------------------------------------------------*/

const AlertDialogRoot = ({
  children,
  ...props
}) => {
  const alertDialogContext = useMemo(() => ({
    slots: alertDialogVariants(),
    placement: undefined
  }), []);
  return /*#__PURE__*/jsx(AlertDialogContext, {
    value: alertDialogContext,
    children: /*#__PURE__*/jsx(DialogTrigger, {
      "data-slot": "alert-dialog-root",
      ...props,
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Trigger
 * -----------------------------------------------------------------------------------------------*/

const AlertDialogTrigger = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(AlertDialogContext);
  return /*#__PURE__*/jsx(Pressable, {
    children: /*#__PURE__*/jsx("div", {
      className: composeSlotClassName(slots?.trigger, className),
      "data-slot": "alert-dialog-trigger",
      role: "button",
      ...props,
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Backdrop
 * -----------------------------------------------------------------------------------------------*/

const AlertDialogBackdrop = ({
  children,
  className,
  isDismissable = false,
  isKeyboardDismissDisabled = true,
  onClick,
  variant,
  ...props
}) => {
  const {
    slots: contextSlots
  } = use(AlertDialogContext);
  const updatedSlots = useMemo(() => alertDialogVariants({
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
    "data-slot": "alert-dialog-backdrop",
    isDismissable: isDismissable,
    isKeyboardDismissDisabled: isKeyboardDismissDisabled,
    onClick: e => {
      e.stopPropagation();
      onClick?.(e);
    },
    ...props,
    children: renderProps => /*#__PURE__*/jsxs(AlertDialogContext, {
      value: updatedModalContext,
      children: [typeof children === "function" ? children(renderProps) : children, " "]
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Container
 * -----------------------------------------------------------------------------------------------*/

const AlertDialogContainer = ({
  children,
  className,
  placement = "auto",
  size,
  ...props
}) => {
  const {
    slots: contextSlots
  } = use(AlertDialogContext);
  const updatedSlots = useMemo(() => alertDialogVariants({
    size
  }), [size]);
  const updatedContext = useMemo(() => ({
    placement,
    slots: {
      ...contextSlots,
      ...updatedSlots
    }
  }), [placement, contextSlots, updatedSlots]);
  return /*#__PURE__*/jsx(Modal, {
    className: composeTwRenderProps(className, updatedSlots?.container()),
    "data-placement": placement,
    "data-slot": "alert-dialog-container",
    ...props,
    children: renderProps => /*#__PURE__*/jsx(AlertDialogContext, {
      value: updatedContext,
      children: typeof children === "function" ? children(renderProps) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Dialog
 * -----------------------------------------------------------------------------------------------*/

const AlertDialogDialog = ({
  children,
  className,
  ...props
}) => {
  const {
    placement,
    slots
  } = use(AlertDialogContext);
  return /*#__PURE__*/jsx(Dialog, {
    className: composeSlotClassName(slots?.dialog, className),
    "data-placement": placement,
    "data-slot": "alert-dialog-dialog",
    role: "alertdialog",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Header
 * -----------------------------------------------------------------------------------------------*/

const AlertDialogHeader = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(AlertDialogContext);
  return /*#__PURE__*/jsx("div", {
    className: composeSlotClassName(slots?.header, className),
    "data-slot": "alert-dialog-header",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Heading
 * -----------------------------------------------------------------------------------------------*/

const AlertDialogHeading = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(AlertDialogContext);
  return /*#__PURE__*/jsx(Heading, {
    className: composeSlotClassName(slots?.heading, className),
    "data-slot": "alert-dialog-heading",
    slot: "title",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Body
 * -----------------------------------------------------------------------------------------------*/

const AlertDialogBody = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(AlertDialogContext);
  return /*#__PURE__*/jsx("div", {
    className: composeSlotClassName(slots?.body, className),
    "data-slot": "alert-dialog-body",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Footer
 * -----------------------------------------------------------------------------------------------*/

const AlertDialogFooter = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(AlertDialogContext);
  return /*#__PURE__*/jsx("div", {
    className: composeSlotClassName(slots?.footer, className),
    "data-slot": "alert-dialog-footer",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Icon
 * -----------------------------------------------------------------------------------------------*/

const AlertDialogIcon = ({
  children,
  className,
  status = "danger",
  ...props
}) => {
  const slots = useMemo(() => alertDialogVariants({
    status
  }), [status]);
  const getDefaultIcon = () => {
    switch (status) {
      case "default":
        return /*#__PURE__*/jsx(InfoIcon, {
          "data-slot": "alert-dialog-default-icon"
        });
      case "accent":
        return /*#__PURE__*/jsx(InfoIcon, {
          "data-slot": "alert-dialog-default-icon"
        });
      case "success":
        return /*#__PURE__*/jsx(SuccessIcon, {
          "data-slot": "alert-dialog-default-icon"
        });
      case "warning":
        return /*#__PURE__*/jsx(WarningIcon, {
          "data-slot": "alert-dialog-default-icon"
        });
      case "danger":
        return /*#__PURE__*/jsx(DangerIcon, {
          "data-slot": "alert-dialog-default-icon"
        });
      default:
        return /*#__PURE__*/jsx(DangerIcon, {
          "data-slot": "alert-dialog-default-icon"
        });
    }
  };
  return /*#__PURE__*/jsx(dom.div, {
    className: slots?.icon({
      className
    }),
    "data-slot": "alert-dialog-icon",
    ...props,
    children: children ?? getDefaultIcon()
  });
};

/* -------------------------------------------------------------------------------------------------
 * AlertDialog Close Trigger
 * -----------------------------------------------------------------------------------------------*/

const AlertDialogCloseTrigger = ({
  className,
  ...rest
}) => {
  const {
    slots
  } = use(AlertDialogContext);
  return /*#__PURE__*/jsx(CloseButton, {
    className: composeTwRenderProps(className, slots?.closeTrigger()),
    "data-slot": "alert-dialog-close-trigger",
    slot: "close",
    ...rest
  });
};

export { AlertDialogBackdrop, AlertDialogBody, AlertDialogCloseTrigger, AlertDialogContainer, AlertDialogDialog, AlertDialogFooter, AlertDialogHeader, AlertDialogHeading, AlertDialogIcon, AlertDialogRoot, AlertDialogTrigger };
