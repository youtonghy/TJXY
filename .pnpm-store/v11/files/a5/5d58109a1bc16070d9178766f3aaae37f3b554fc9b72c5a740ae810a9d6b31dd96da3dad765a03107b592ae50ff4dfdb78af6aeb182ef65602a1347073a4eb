"use client";
import { descriptionVariants } from '@heroui/styles';
import { Text } from 'react-aria-components/Text';
import { useHasTextSlot } from '../../utils/use-has-text-slot.js';
import { jsx } from 'react/jsx-runtime';

const DescriptionRoot = ({
  children,
  className,
  ...rest
}) => {
  if (!useHasTextSlot("description")) {
    return null;
  }
  return /*#__PURE__*/jsx(Text, {
    className: descriptionVariants({
      className
    }),
    "data-slot": "description",
    slot: "description",
    ...rest,
    children: children
  });
};

export { DescriptionRoot };
