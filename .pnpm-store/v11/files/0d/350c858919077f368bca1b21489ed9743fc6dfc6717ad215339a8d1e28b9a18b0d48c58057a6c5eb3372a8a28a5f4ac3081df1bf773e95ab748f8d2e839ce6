"use client";
import { emptyStateVariants } from '@heroui/styles';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';

const EmptyStateRoot = ({
  children,
  className,
  ...rest
}) => {
  return /*#__PURE__*/jsx(dom.div, {
    className: emptyStateVariants({
      className
    }),
    "data-slot": "empty-state",
    ...rest,
    children: children || "No results found"
  });
};

export { EmptyStateRoot };
