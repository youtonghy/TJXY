"use client";
import { colorPickerVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { Button } from 'react-aria-components/Button';
import { ColorPicker } from 'react-aria-components/ColorPicker';
import { DialogTrigger, Popover } from 'react-aria-components/Popover';
import { composeSlotClassName, composeTwRenderProps } from '../../utils/compose.js';
import { jsx, Fragment } from 'react/jsx-runtime';
import { SurfaceContext } from '../surface/surface.js';

const ColorPickerContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * ColorPicker Root
 * -----------------------------------------------------------------------------------------------*/

const ColorPickerRoot = ({
  children,
  className,
  ...props
}) => {
  const slots = React__default.useMemo(() => colorPickerVariants(), []);
  return /*#__PURE__*/jsx(ColorPickerContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(ColorPicker, {
      ...props,
      children: /*#__PURE__*/jsx(DialogTrigger, {
        children: /*#__PURE__*/jsx("div", {
          className: composeSlotClassName(slots?.base, className),
          "data-slot": "color-picker",
          children: children
        })
      })
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * ColorPicker Trigger
 * -----------------------------------------------------------------------------------------------*/

const ColorPickerTrigger = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ColorPickerContext);
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, slots?.trigger()),
    "data-slot": "color-picker-trigger",
    ...props,
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * ColorPicker Popover
 * -----------------------------------------------------------------------------------------------*/

const ColorPickerPopover = ({
  children,
  className,
  placement = "bottom left",
  ...props
}) => {
  const {
    slots
  } = use(ColorPickerContext);
  return /*#__PURE__*/jsx(SurfaceContext, {
    value: {
      variant: "default"
    },
    children: /*#__PURE__*/jsx(Popover, {
      ...props,
      className: composeTwRenderProps(className, slots?.popover()),
      "data-slot": "color-picker-popover",
      placement: placement,
      children: children
    })
  });
};

export { ColorPickerPopover, ColorPickerRoot, ColorPickerTrigger };
