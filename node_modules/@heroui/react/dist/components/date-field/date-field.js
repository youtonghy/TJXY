"use client";
import { dateFieldVariants } from '@heroui/styles';
import React__default from 'react';
import { DateField } from 'react-aria-components/DateField';
import { dataAttr } from '../../utils/assertion.js';
import { composeTwRenderProps } from '../../utils/compose.js';
import { jsx, Fragment } from 'react/jsx-runtime';

function DateFieldRoot({
  children,
  className,
  fullWidth,
  ...props
}) {
  const styles = React__default.useMemo(() => dateFieldVariants({
    fullWidth
  }), [fullWidth]);
  return /*#__PURE__*/jsx(DateField, {
    "data-required": dataAttr(props.isRequired),
    "data-slot": "date-field",
    ...props,
    className: composeTwRenderProps(className, styles),
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    })
  });
}

export { DateFieldRoot };
