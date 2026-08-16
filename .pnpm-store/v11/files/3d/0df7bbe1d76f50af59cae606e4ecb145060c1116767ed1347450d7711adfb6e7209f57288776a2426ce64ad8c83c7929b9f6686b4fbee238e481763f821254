"use client";
import { buttonVariants } from '@heroui/styles';
import { use } from 'react';
import { Button } from 'react-aria-components/Button';
import { jsx } from 'react/jsx-runtime';
import { ButtonGroupContext, BUTTON_GROUP_CHILD } from '../button-group/button-group.js';
import { composeTwRenderProps } from '../../utils/compose.js';

const ButtonRoot = ({
  children,
  className,
  fullWidth,
  isDisabled,
  isIconOnly,
  size,
  slot,
  style,
  variant,
  [BUTTON_GROUP_CHILD]: isButtonGroupChild,
  ...rest
}) => {
  const buttonGroupContext = use(ButtonGroupContext);

  // Only use context if this button is a direct child of ButtonGroup
  const shouldUseContext = isButtonGroupChild === true;

  // Merge props with precedence: direct props > context props
  const finalSize = size ?? (shouldUseContext ? buttonGroupContext?.size : undefined);
  const finalVariant = variant ?? (shouldUseContext ? buttonGroupContext?.variant : undefined);
  const finalIsDisabled = isDisabled ?? (shouldUseContext ? buttonGroupContext?.isDisabled : undefined);
  const finalFullWidth = fullWidth ?? (shouldUseContext ? buttonGroupContext?.fullWidth : undefined);
  const styles = buttonVariants({
    fullWidth: finalFullWidth,
    isIconOnly,
    size: finalSize,
    variant: finalVariant
  });
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, styles),
    "data-slot": "button",
    isDisabled: finalIsDisabled,
    slot: slot,
    style: style,
    ...rest,
    children: renderProps => typeof children === "function" ? children(renderProps) : children
  });
};

export { ButtonRoot };
