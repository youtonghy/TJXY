"use client";
import { textFieldVariants } from '@heroui/styles';
import React__default, { createContext } from 'react';
import { TextField } from 'react-aria-components/TextField';
import { composeTwRenderProps } from '../../utils/compose.js';
import { jsx, Fragment } from 'react/jsx-runtime';

const TextFieldContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * TextField Root
 * -----------------------------------------------------------------------------------------------*/

const TextFieldRoot = ({
  children,
  className,
  fullWidth,
  variant,
  ...props
}) => {
  const styles = React__default.useMemo(() => textFieldVariants({
    fullWidth
  }), [fullWidth]);
  return /*#__PURE__*/jsx(TextField, {
    "data-slot": "textfield",
    ...props,
    className: composeTwRenderProps(className, styles),
    children: values => /*#__PURE__*/jsx(TextFieldContext, {
      value: {
        variant
      },
      children: /*#__PURE__*/jsx(Fragment, {
        children: typeof children === "function" ? children(values) : children
      })
    })
  });
};

export { TextFieldContext, TextFieldRoot };
