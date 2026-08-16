"use client";
import { colorSwatchVariants } from '@heroui/styles';
import { ColorSwatch } from 'react-aria-components/ColorSwatch';
import { composeTwRenderProps } from '../../utils/compose.js';
import { jsx } from 'react/jsx-runtime';

const ColorSwatchRoot = ({
  className,
  shape,
  size,
  style,
  ...props
}) => {
  return /*#__PURE__*/jsx(ColorSwatch, {
    ...props,
    className: composeTwRenderProps(className, colorSwatchVariants({
      shape,
      size
    })),
    "data-slot": "color-swatch",
    style: renderProps => {
      const userStyle = typeof style === "function" ? style(renderProps) : style;
      return {
        "--color-swatch-current": renderProps.color.toString("css"),
        ...userStyle
      };
    }
  });
};

export { ColorSwatchRoot };
