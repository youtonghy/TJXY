"use client";
import { timeFieldVariants } from '@heroui/styles';
import React__default from 'react';
import { TimeField } from 'react-aria-components/TimeField';
import { dataAttr } from '../../utils/assertion.js';
import { composeTwRenderProps } from '../../utils/compose.js';
import { jsx, Fragment } from 'react/jsx-runtime';

function TimeFieldRoot({
  children,
  className,
  fullWidth,
  ...props
}) {
  const styles = React__default.useMemo(() => timeFieldVariants({
    fullWidth
  }), [fullWidth]);
  return /*#__PURE__*/jsx(TimeField, {
    "data-required": dataAttr(props.isRequired),
    "data-slot": "time-field",
    ...props,
    className: composeTwRenderProps(className, styles),
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    })
  });
}

export { TimeFieldRoot };
