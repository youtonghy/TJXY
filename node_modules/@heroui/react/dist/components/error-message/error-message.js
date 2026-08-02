"use client";
import { errorMessageVariants } from '@heroui/styles';
import { Text } from 'react-aria-components/Text';
import { useHasTextSlot } from '../../utils/use-has-text-slot.js';
import { jsx } from 'react/jsx-runtime';

const ErrorMessageRoot = ({
  children,
  className,
  ...rest
}) => {
  if (!useHasTextSlot("errorMessage")) {
    return null;
  }
  return /*#__PURE__*/jsx(Text, {
    className: errorMessageVariants({
      className
    }),
    "data-slot": "error-message",
    slot: "errorMessage",
    ...rest,
    children: children
  });
};

export { ErrorMessageRoot };
