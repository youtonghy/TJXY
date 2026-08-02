"use client";
import { colorAreaVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { ColorArea, ColorThumb } from 'react-aria-components/ColorArea';
import { composeTwRenderProps } from '../../utils/compose.js';
import { jsx } from 'react/jsx-runtime';

const ColorAreaContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * ColorArea Root
 * -----------------------------------------------------------------------------------------------*/

const ColorAreaRoot = ({
  children,
  className,
  showDots,
  style,
  ...props
}) => {
  const slots = React__default.useMemo(() => colorAreaVariants({
    showDots
  }), [showDots]);
  return /*#__PURE__*/jsx(ColorAreaContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(ColorArea, {
      ...props,
      className: composeTwRenderProps(className, slots.base()),
      "data-slot": "color-area",
      style: renderProps => {
        const userStyle = typeof style === "function" ? style(renderProps) : style;
        return {
          "--color-area-background": renderProps.defaultStyle.background,
          ...userStyle
        };
      },
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * ColorArea Thumb
 * -----------------------------------------------------------------------------------------------*/

const ColorAreaThumb = ({
  className,
  style,
  ...props
}) => {
  const {
    slots
  } = use(ColorAreaContext);
  return /*#__PURE__*/jsx(ColorThumb, {
    className: composeTwRenderProps(className, slots?.thumb()),
    "data-slot": "color-area-thumb",
    style: renderProps => {
      const userStyle = typeof style === "function" ? style(renderProps) : style;
      return {
        "--color-area-thumb-color": renderProps.defaultStyle.backgroundColor,
        ...userStyle
      };
    },
    ...props
  });
};

export { ColorAreaRoot, ColorAreaThumb };
