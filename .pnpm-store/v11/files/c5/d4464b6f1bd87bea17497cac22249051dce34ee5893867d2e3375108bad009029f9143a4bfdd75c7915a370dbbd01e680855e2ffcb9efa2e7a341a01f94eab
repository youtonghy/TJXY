"use client";
import { inputVariants } from '@heroui/styles';
import { use } from 'react';
import { Input } from 'react-aria-components/Input';
import { jsx } from 'react/jsx-runtime';
import { TextFieldContext } from '../textfield/textfield.js';
import { ComboBoxContext } from '../combo-box/combo-box.js';
import { composeTwRenderProps } from '../../utils/compose.js';

const InputRoot = ({
  className,
  fullWidth,
  variant: variantProp,
  ...rest
}) => {
  const textFieldContext = use(TextFieldContext);
  const comboBoxContext = use(ComboBoxContext);

  // Use variant from context if not explicitly provided
  const variant = variantProp ?? textFieldContext.variant ?? comboBoxContext.variant;
  return /*#__PURE__*/jsx(Input, {
    className: composeTwRenderProps(className, inputVariants({
      fullWidth,
      variant
    })),
    "data-slot": "input",
    ...rest
  });
};

export { InputRoot };
