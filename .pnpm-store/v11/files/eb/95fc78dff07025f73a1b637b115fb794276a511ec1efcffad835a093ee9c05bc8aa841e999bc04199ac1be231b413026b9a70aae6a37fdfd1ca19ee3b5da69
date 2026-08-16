"use client";
import { listboxSectionVariants } from '@heroui/styles';
import React__default from 'react';
import { ListBoxSection } from 'react-aria-components/ListBox';
import { jsx } from 'react/jsx-runtime';

const ListBoxSectionRoot = ({
  children,
  className,
  ...props
}) => {
  const styles = React__default.useMemo(() => listboxSectionVariants({
    class: typeof className === "string" ? className : undefined
  }), [className]);
  return /*#__PURE__*/jsx(ListBoxSection, {
    className: styles,
    ...props,
    children: children
  });
};

export { ListBoxSectionRoot };
