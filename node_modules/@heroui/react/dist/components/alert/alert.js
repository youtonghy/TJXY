"use client";
import { alertVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { InfoIcon, DangerIcon, WarningIcon, SuccessIcon } from '../icons.js';
import { jsx } from 'react/jsx-runtime';
import { SurfaceContext } from '../surface/surface.js';

const AlertContext = /*#__PURE__*/createContext({});

/* ------------------------------------------------------------------------------------------------
 * Alert Root
 * --------------------------------------------------------------------------------------------- */

const AlertRoot = ({
  children,
  className,
  status,
  ...rest
}) => {
  const slots = React__default.useMemo(() => alertVariants({
    status
  }), [status]);
  return /*#__PURE__*/jsx(AlertContext, {
    value: {
      slots,
      status
    },
    children: /*#__PURE__*/jsx(SurfaceContext, {
      value: {
        variant: "default"
      },
      children: /*#__PURE__*/jsx(dom.div, {
        className: slots?.base({
          className
        }),
        "data-slot": "alert-root",
        ...rest,
        children: children
      })
    })
  });
};

/* ------------------------------------------------------------------------------------------------
 * Alert Indicator
 * --------------------------------------------------------------------------------------------- */

const AlertIndicator = ({
  children,
  className,
  ...rest
}) => {
  const {
    slots,
    status
  } = use(AlertContext);

  // Map status to default icons
  const getDefaultIcon = () => {
    switch (status) {
      case "accent":
        return /*#__PURE__*/jsx(InfoIcon, {
          "data-slot": "alert-default-icon"
        });
      case "success":
        return /*#__PURE__*/jsx(SuccessIcon, {
          "data-slot": "alert-default-icon"
        });
      case "warning":
        return /*#__PURE__*/jsx(WarningIcon, {
          "data-slot": "alert-default-icon"
        });
      case "danger":
        return /*#__PURE__*/jsx(DangerIcon, {
          "data-slot": "alert-default-icon"
        });
      default:
        return /*#__PURE__*/jsx(InfoIcon, {
          "data-slot": "alert-default-icon"
        });
    }
  };
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.indicator, className),
    "data-slot": "alert-indicator",
    ...rest,
    children: children ?? getDefaultIcon()
  });
};

/* ------------------------------------------------------------------------------------------------
 * Alert Content
 * --------------------------------------------------------------------------------------------- */

const AlertContent = ({
  children,
  className,
  ...rest
}) => {
  const {
    slots
  } = use(AlertContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.content, className),
    "data-slot": "alert-content",
    ...rest,
    children: children
  });
};

/* ------------------------------------------------------------------------------------------------
 * Alert Title
 * --------------------------------------------------------------------------------------------- */

const AlertTitle = ({
  children,
  className,
  ...rest
}) => {
  const {
    slots
  } = use(AlertContext);
  return /*#__PURE__*/jsx(dom.p, {
    className: composeSlotClassName(slots?.title, className),
    "data-slot": "alert-title",
    ...rest,
    children: children
  });
};

/* ------------------------------------------------------------------------------------------------
 * Alert Description
 * --------------------------------------------------------------------------------------------- */

const AlertDescription = ({
  children,
  className,
  ...rest
}) => {
  const {
    slots
  } = use(AlertContext);
  return /*#__PURE__*/jsx(dom.span, {
    className: composeSlotClassName(slots?.description, className),
    "data-slot": "alert-description",
    ...rest,
    children: children
  });
};

export { AlertContent, AlertDescription, AlertIndicator, AlertRoot, AlertTitle };
