"use client";
import { colorSwatchPickerVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { ColorSwatchPicker, ColorSwatch, ColorSwatchPickerItem as ColorSwatchPickerItem$1 } from 'react-aria-components/ColorSwatchPicker';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';

const ColorSwatchPickerContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
| * ColorSwatchPickerItem Context
| * -----------------------------------------------------------------------------------------------*/

const ColorSwatchPickerItemContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
| * ColorSwatchPicker Root
| * -----------------------------------------------------------------------------------------------*/

const ColorSwatchPickerRoot = ({
  children,
  className,
  layout,
  size,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => colorSwatchPickerVariants({
    layout,
    size,
    variant
  }), [layout, size, variant]);
  return /*#__PURE__*/jsx(ColorSwatchPickerContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(ColorSwatchPicker, {
      "data-slot": "color-swatch-picker",
      ...props,
      className: composeTwRenderProps(className, slots.base()),
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
| * ColorSwatchPicker Item
| * -----------------------------------------------------------------------------------------------*/

const ColorSwatchPickerItem = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ColorSwatchPickerContext);
  return /*#__PURE__*/jsx(ColorSwatchPickerItem$1, {
    "data-slot": "color-swatch-picker-item",
    ...props,
    className: composeTwRenderProps(className, slots?.item()),
    style: renderProps => ({
      "--color-swatch-current": renderProps.color.toString("css")
    }),
    children: renderProps => /*#__PURE__*/jsx(ColorSwatchPickerItemContext, {
      value: {
        state: renderProps
      },
      children: typeof children === "function" ? children(renderProps) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
| * ColorSwatchPicker Swatch
| * -----------------------------------------------------------------------------------------------*/

const ColorSwatchPickerSwatch = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(ColorSwatchPickerContext);
  return /*#__PURE__*/jsx(ColorSwatch, {
    className: composeTwRenderProps(className, slots?.swatch()),
    "data-slot": "color-swatch-picker-swatch",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
| * ColorSwatchPicker Indicator
| * -----------------------------------------------------------------------------------------------*/

/**
 * Calculate relative luminance of a color
 * Uses the formula: L = 0.2126 * R + 0.7152 * G + 0.0722 * B
 * Returns a value between 0 (black) and 1 (white)
 */
function getColorLuminance(color) {
  // Get RGB values (0-255 range)
  const r = color.getChannelValue("red");
  const g = color.getChannelValue("green");
  const b = color.getChannelValue("blue");

  // Normalize to 0-1 and calculate luminance
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
}
const ColorSwatchPickerIndicator = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(ColorSwatchPickerContext);
  const {
    state
  } = use(ColorSwatchPickerItemContext);

  // Determine if the background color is light (luminance > 0.5)
  // Use white checkmark on dark backgrounds, black on light backgrounds
  const isLightColor = state?.color ? getColorLuminance(state.color) > 0.5 : false;
  const content = typeof children === "function" ? children(state ?? {}) : children ? children : /*#__PURE__*/jsx("svg", {
    "aria-hidden": "true",
    "data-slot": "color-swatch-picker-checkmark",
    fill: "none",
    role: "presentation",
    stroke: "currentColor",
    strokeLinecap: "round",
    strokeLinejoin: "round",
    strokeWidth: 1.5,
    viewBox: "0 0 12 12",
    children: /*#__PURE__*/jsx("polyline", {
      points: "2.5 6 5 8.5 9.5 3"
    })
  });
  return /*#__PURE__*/jsx(dom.span, {
    "aria-hidden": "true",
    className: composeSlotClassName(slots?.indicator, className),
    "data-light-color": isLightColor ? "true" : undefined,
    "data-slot": "color-swatch-picker-indicator",
    ...props,
    children: content
  });
};

export { ColorSwatchPickerIndicator, ColorSwatchPickerItem, ColorSwatchPickerRoot, ColorSwatchPickerSwatch };
