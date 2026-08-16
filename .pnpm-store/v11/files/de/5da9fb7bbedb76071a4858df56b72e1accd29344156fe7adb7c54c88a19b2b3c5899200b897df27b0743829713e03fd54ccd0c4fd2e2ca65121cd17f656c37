"use client";
import { menuSectionVariants } from '@heroui/styles';
import React__default from 'react';
import { MenuSection } from 'react-aria-components/Menu';
import { jsx } from 'react/jsx-runtime';

const MenuSectionRoot = ({
  children,
  className,
  ...props
}) => {
  const styles = React__default.useMemo(() => menuSectionVariants({
    class: typeof className === "string" ? className : undefined
  }), [className]);
  return /*#__PURE__*/jsx(MenuSection, {
    className: styles,
    ...props,
    children: children
  });
};

export { MenuSectionRoot };
