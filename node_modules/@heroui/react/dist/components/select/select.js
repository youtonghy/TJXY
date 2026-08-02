"use client";
import { selectVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { Button } from 'react-aria-components/Button';
import { Popover } from 'react-aria-components/Popover';
import { Select, SelectStateContext, SelectValue as SelectValue$1 } from 'react-aria-components/Select';
import { dataAttr } from '../../utils/assertion.js';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { FieldSlotsGate } from '../../utils/field-slots-gate.js';
import { IconChevronDown } from '../icons.js';
import { jsx, Fragment } from 'react/jsx-runtime';
import { SurfaceContext } from '../surface/surface.js';

const SelectContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Select Root
 * -----------------------------------------------------------------------------------------------*/

const SelectRoot = ({
  children,
  className,
  fullWidth,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => selectVariants({
    fullWidth,
    variant
  }), [fullWidth, variant]);
  return /*#__PURE__*/jsx(FieldSlotsGate, {
    children: /*#__PURE__*/jsx(SelectContext, {
      value: {
        slots
      },
      children: /*#__PURE__*/jsx(Select, {
        "data-slot": "select",
        ...props,
        className: composeTwRenderProps(className, slots?.base()),
        children: values => /*#__PURE__*/jsx(Fragment, {
          children: typeof children === "function" ? children(values) : children
        })
      })
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Select Trigger
 * -----------------------------------------------------------------------------------------------*/

const SelectTrigger = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(SelectContext);
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, slots?.trigger()),
    "data-slot": "select-trigger",
    ...props,
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Select Value
 * -----------------------------------------------------------------------------------------------*/

const SelectValue = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(SelectContext);
  return /*#__PURE__*/jsx(SelectValue$1, {
    className: composeTwRenderProps(className, slots?.value()),
    "data-slot": "select-value",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Select Indicator
 * -----------------------------------------------------------------------------------------------*/

const SelectIndicator = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(SelectContext);
  const state = use(SelectStateContext);
  if (children && /*#__PURE__*/React__default.isValidElement(children)) {
    return /*#__PURE__*/React__default.cloneElement(children, {
      ...props,
      className: composeSlotClassName(slots?.indicator, className),
      "data-slot": "select-indicator",
      "data-open": dataAttr(state?.isOpen)
    });
  }
  return /*#__PURE__*/jsx(IconChevronDown, {
    className: composeSlotClassName(slots?.indicator, className),
    "data-open": dataAttr(state?.isOpen),
    "data-slot": "select-default-indicator",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * Select Popover
 * -----------------------------------------------------------------------------------------------*/

const SelectPopover = ({
  children,
  className,
  placement = "bottom",
  ...props
}) => {
  const {
    slots
  } = use(SelectContext);
  return /*#__PURE__*/jsx(SurfaceContext, {
    value: {
      variant: "default"
    },
    children: /*#__PURE__*/jsx(Popover, {
      ...props,
      className: composeTwRenderProps(className, slots?.popover()),
      "data-slot": "select-popover",
      placement: placement,
      children: children
    })
  });
};

export { SelectIndicator, SelectPopover, SelectRoot, SelectTrigger, SelectValue };
