"use client";
import { labelVariants } from '@heroui/styles';
import { Label } from 'react-aria-components/Label';
import { jsx } from 'react/jsx-runtime';

const LabelRoot = ({
  children,
  className,
  isDisabled,
  isInvalid,
  isRequired,
  ...rest
}) => {
  return /*#__PURE__*/jsx(Label, {
    className: labelVariants({
      isRequired,
      isDisabled,
      isInvalid,
      className
    }),
    "data-slot": "label",
    ...rest,
    children: children
  });
};

export { LabelRoot };
