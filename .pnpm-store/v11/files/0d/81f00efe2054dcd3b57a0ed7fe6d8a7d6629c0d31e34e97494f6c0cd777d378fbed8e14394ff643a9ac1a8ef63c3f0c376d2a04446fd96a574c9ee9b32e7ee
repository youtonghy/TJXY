"use client";
import { fieldErrorVariants } from '@heroui/styles';
import { FieldError } from 'react-aria-components/FieldError';
import { composeTwRenderProps } from '../../utils/compose.js';
import { jsx } from 'react/jsx-runtime';

const FieldErrorRoot = ({
  children,
  className,
  ...rest
}) => {
  return /*#__PURE__*/jsx(FieldError, {
    "data-visible": true,
    className: composeTwRenderProps(className, fieldErrorVariants()),
    "data-slot": "field-error",
    ...rest,
    children: renderProps => typeof children === "function" ? children(renderProps) : children
  });
};

export { FieldErrorRoot };
