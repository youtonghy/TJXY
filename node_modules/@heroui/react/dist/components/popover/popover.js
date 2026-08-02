"use client";
import { popoverVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { DialogTrigger, Heading, Dialog } from 'react-aria-components/Dialog';
import { Popover, OverlayArrow, Pressable } from 'react-aria-components/Popover';
import { composeSlotClassName, composeTwRenderProps } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';
import { SurfaceContext } from '../surface/surface.js';

const PopoverContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Popover Root
 * -----------------------------------------------------------------------------------------------*/

const PopoverRoot = ({
  children,
  ...props
}) => {
  const slots = React__default.useMemo(() => popoverVariants(), []);
  return /*#__PURE__*/jsx(PopoverContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(DialogTrigger, {
      "data-slot": "popover-root",
      ...props,
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Popover Content
 * -----------------------------------------------------------------------------------------------*/

const PopoverContent = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(PopoverContext);
  return /*#__PURE__*/jsx(PopoverContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(SurfaceContext, {
      value: {
        variant: "default"
      },
      children: /*#__PURE__*/jsx(Popover, {
        ...props,
        className: composeTwRenderProps(className, slots?.base()),
        children: children
      })
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Popover Arrow
 * -----------------------------------------------------------------------------------------------*/

const PopoverArrow = ({
  children,
  className,
  ...props
}) => {
  const defaultArrow = /*#__PURE__*/jsx("svg", {
    "data-slot": "popover-overlay-arrow",
    fill: "none",
    height: "12",
    viewBox: "0 0 12 12",
    width: "12",
    xmlns: "http://www.w3.org/2000/svg",
    children: /*#__PURE__*/jsx("path", {
      d: "M0 0C5.48483 8 6.5 8 12 0Z"
    })
  });
  const arrow = /*#__PURE__*/React__default.isValidElement(children) ? /*#__PURE__*/React__default.cloneElement(children, {
    "data-slot": "popover-overlay-arrow"
  }) : defaultArrow;
  return /*#__PURE__*/jsx(OverlayArrow, {
    "data-slot": "popover-overlay-arrow-group",
    ...props,
    className: className,
    children: arrow
  });
};

/* -------------------------------------------------------------------------------------------------
 * Popover Dialog
 * -----------------------------------------------------------------------------------------------*/

const PopoverDialog = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(PopoverContext);
  return /*#__PURE__*/jsx(Dialog, {
    "data-slot": "popover-dialog",
    ...props,
    className: composeSlotClassName(slots?.dialog, className),
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Popover Trigger
 * -----------------------------------------------------------------------------------------------*/

const PopoverTrigger = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(PopoverContext);
  return /*#__PURE__*/jsx(Pressable, {
    children: /*#__PURE__*/jsx(dom.div, {
      className: composeSlotClassName(slots?.trigger, className),
      "data-slot": "popover-trigger",
      role: "button",
      ...props,
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Popover Heading
 * -----------------------------------------------------------------------------------------------*/

const PopoverHeading = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(PopoverContext);
  return /*#__PURE__*/jsx(Heading, {
    slot: "title",
    ...props,
    className: composeSlotClassName(slots?.heading, className),
    children: children
  });
};

export { PopoverArrow, PopoverContent, PopoverDialog, PopoverHeading, PopoverRoot, PopoverTrigger };
