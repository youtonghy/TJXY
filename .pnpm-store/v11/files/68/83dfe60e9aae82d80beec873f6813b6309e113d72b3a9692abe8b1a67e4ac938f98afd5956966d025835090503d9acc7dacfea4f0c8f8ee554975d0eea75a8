"use client";
import { headerVariants } from '@heroui/styles';
import { Header } from 'react-aria-components/Header';
import { jsx } from 'react/jsx-runtime';

const HeaderRoot = ({
  children,
  className,
  ...rest
}) => {
  return /*#__PURE__*/jsx(Header, {
    className: headerVariants({
      className
    }),
    "data-slot": "header",
    ...rest,
    children: children
  });
};

export { HeaderRoot };
