"use client";
import { colorFieldVariants } from '@heroui/styles';
import React__default from 'react';
import { ColorField } from 'react-aria-components/ColorField';
import { dataAttr } from '../../utils/assertion.js';
import { composeTwRenderProps } from '../../utils/compose.js';
import { jsx, Fragment } from 'react/jsx-runtime';

function ColorFieldRoot({
  children,
  className,
  fullWidth,
  ...props
}) {
  const styles = React__default.useMemo(() => colorFieldVariants({
    fullWidth
  }), [fullWidth]);
  return /*#__PURE__*/jsx(ColorField, {
    "data-required": dataAttr(props.isRequired),
    "data-slot": "color-field",
    ...props,
    className: composeTwRenderProps(className, styles),
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    })
  });
}

export { ColorFieldRoot };
